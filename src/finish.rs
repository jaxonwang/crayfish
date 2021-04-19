use crate::activity::ActivityId;
use crate::activity::FunctionLabel;
use crate::activity::TaskItem;
use crate::activity::TaskItemExtracter;
use crate::place::Place;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

pub use crate::global_id::FinishId;

#[derive(Debug, Clone)]
struct FrameInfo {
    fn_id: FunctionLabel,
    place: Place,
    panic_payload: Option<String>,
}

#[derive(Debug, Clone)]
struct CallingTreeNode {
    id: ActivityId,
    frame: Option<FrameInfo>,
    parent: Option<ActivityId>,
    children: FxHashSet<ActivityId>,
}

#[derive(Debug)]
pub struct CallingTree {
    lookup_table: FxHashMap<ActivityId, CallingTreeNode>,
    panic_backtrace: Vec<FrameInfo>,
    panic_backtrace_top: Option<ActivityId>,
}

const ROOT_ID: ActivityId = 0;

impl CallingTree {
    pub fn new(initial_call: Vec<ActivityId>) -> Self {
        let mut tree = CallingTree {
            lookup_table: FxHashMap::default(),
            panic_backtrace: Vec::new(),
            panic_backtrace_top: None,
        };
        tree.new_root(ROOT_ID, &initial_call[..], None);
        tree
    }
    fn new_root(&mut self, id: ActivityId, children: &[ActivityId], frame: Option<FrameInfo>) {
        self.lookup_table.insert(
            id,
            CallingTreeNode {
                id,
                frame,
                parent: None,
                children: children.iter().cloned().collect(),
            },
        ); // could overide virtual node
        for child in children.iter() {
            if !self.lookup_table.contains_key(child) {
                // child might exist
                self.new_root(*child, &[], None);
            }
            self.link_parent_child(id, *child);
        }
    }
    fn link_parent_child(&mut self, parent: ActivityId, child: ActivityId) {
        self.lookup_table
            .get_mut(&parent)
            .unwrap()
            .children
            .insert(child);
        self.lookup_table.get_mut(&child).unwrap().parent = Some(parent);
    }

    pub fn all_done(&self) -> bool {
        if self.lookup_table.len() == 1 {
            // only root left
            debug_assert!(self.lookup_table.contains_key(&ROOT_ID));
            return true;
        }
        false
    }

    pub fn activity_done(&mut self, item: Box<TaskItem>) {
        let mut ex = TaskItemExtracter::new(*item);

        let fn_id = ex.fn_id();
        let place = ex.place();
        let activity_id = ex.activity_id();
        let sub_activities = ex.sub_activities();
        let panic_payload = match ex.ret_panic() {
            Ok(()) => None,
            Err(e) => Some(e),
        };

        let frame = FrameInfo {
            fn_id,
            place,
            panic_payload,
        };
        match self.lookup_table.get_mut(&activity_id) {
            Some(node) => {
                let parent_id = node.parent.unwrap();
                // new root will override existing and link
                self.new_root(activity_id, &sub_activities[..], Some(frame));
                self.link_parent_child(parent_id, activity_id);
            }
            None => self.new_root(activity_id, &sub_activities[..], Some(frame)),
        }

        if sub_activities.is_empty() {
            // purge self
            self.try_purge(activity_id);
        } else {
            // current node won't be purged unless only one child left
            for child in sub_activities.iter() {
                // only purge real child
                if self.lookup_table.get(child).unwrap().frame.is_some() {
                    self.try_purge(*child);
                }
            }
        }
    }

    pub fn try_purge(&mut self, mut current_node_id: ActivityId) {
        // purge single branch
        // only remove has parent and children is empty
        loop {
            let node = self.lookup_table.get(&current_node_id).unwrap();
            if !node.children.is_empty() || node.parent.is_none() {
                break;
            }
            let node = self.lookup_table.remove(&current_node_id).unwrap();
            let parent_id = node.parent.unwrap();
            let node_id = node.id;

            // node must have frame now
            let frame = node.frame.unwrap();
            // NOTE: only record the first panic encountered, not the closest panic
            if frame.panic_payload.is_some() && self.panic_backtrace_top.is_none() {
                self.panic_backtrace_top = Some(node_id);
            }
            if let Some(top) = self.panic_backtrace_top.as_ref() {
                if *top == node_id {
                    self.panic_backtrace.push(frame);
                    self.panic_backtrace_top = Some(parent_id);
                }
            }
            let parent = self.lookup_table.get_mut(&parent_id).unwrap();
            parent.children.remove(&current_node_id);
            current_node_id = parent.id;
        }
    }

    pub fn panic_backtrace(self) -> Option<String> {
        let format_frame = |frameinfo: FrameInfo| {
            let mut line = format!(
                "At place: {} function: {}",
                frameinfo.place, frameinfo.fn_id
            );
            if let Some(msg) = frameinfo.panic_payload {
                line.push_str(&format!(" panic: {}", msg));
            }
            line
        };

        debug_assert!(self.all_done());
        if self.panic_backtrace.is_empty() {
            None
        } else {
            Some(
                self.panic_backtrace
                    .into_iter()
                    .map(format_frame)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::activity::TaskItemBuilder;
    use rand::seq::SliceRandom;
    use std::any::Any;
    use std::thread::Result;

    fn build_chain(panic_at: Option<ActivityId>) -> Vec<Box<TaskItem>> {
        let build_one = |fn_id, place, a_id, sub_activity_id| {
            let mut b = TaskItemBuilder::new(fn_id, place, a_id);
            match panic_at.as_ref() {
                Some(id) if *id == a_id => {
                    let panic_payload = Box::new(String::from("panic here"));
                    b.ret(Result::<usize>::Err(
                        panic_payload as Box<dyn Any + Send + 'static>,
                    ));
                }
                _ => b.ret(Result::Ok(1usize)),
            }
            b.sub_activities(vec![sub_activity_id]);

            b.build()
        };

        let size: usize = 24;
        let mut items: Vec<_> = (1..size) // size - 1 items
            .map(|i| {
                Box::new(build_one(
                    i as FunctionLabel,
                    i as Place,
                    i as ActivityId,
                    (i + 1) as ActivityId,
                ))
            })
            .collect();

        // last one without sub activities
        let mut b = TaskItemBuilder::new(size as FunctionLabel, size as Place, size as ActivityId);
        b.ret(std::thread::Result::Ok(1usize));
        items.push(Box::new(b.build()));

        items
    }

    #[test]
    pub fn test_calling_tree_chain_ordered() {
        // test a chain
        let items = build_chain(None);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());

        // test a chain with panic
        let items = build_chain(Some(10));
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }

        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 10);
        let bt = tree.panic_backtrace();
        // println!("{}", bt.as_ref().unwrap());
        assert!(bt.is_some());
    }

    #[test]
    pub fn test_calling_tree_chain_reordered() {
        let mut rng = rand::thread_rng();

        // test reorder
        let mut items = build_chain(None);
        items.shuffle(&mut rng);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());

        // test reorder panic
        let mut items = build_chain(Some(10));
        items.shuffle(&mut rng);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 10);
        let bt = tree.panic_backtrace();
        assert!(bt.is_some());
    }

    fn build_tree(
        layer: usize,
        degree: usize,
        panic_at: Option<Vec<ActivityId>>,
    ) -> Vec<Box<TaskItem>> {
        let build_one = |fn_id, place, a_id, sub_activities: Vec<usize>| {
            let fn_id = fn_id as FunctionLabel;
            let place = place as Place;
            let a_id = a_id as ActivityId;
            let sub_activities: Vec<_> = sub_activities
                .into_iter()
                .map(|i| i as ActivityId)
                .collect();
            let mut b = TaskItemBuilder::new(fn_id, place, a_id);
            match panic_at.as_ref() {
                Some(ids) if ids.iter().any(|id| *id == a_id) => {
                    let panic_payload = Box::new(String::from("panic here"));
                    b.ret(Result::<usize>::Err(
                        panic_payload as Box<dyn Any + Send + 'static>,
                    ));
                }
                _ => b.ret(Result::Ok(1usize)),
            }
            b.sub_activities(sub_activities);

            Box::new(b.build())
        };
        let mut items = vec![];
        let mut build_layer = |current_layer: usize, is_last_layer: bool| {
            // layer start from 1
            let current_layer = current_layer as u32;
            let start = (degree.pow(current_layer - 1) - 1) / (degree - 1) + 1;
            let end = (degree.pow(current_layer) - 1) / (degree - 1);
            let mut sub_activitie_counter = end + 1;
            let mut get_sub_activities = || {
                let ret: Vec<_> = (sub_activitie_counter..sub_activitie_counter + degree).collect();
                sub_activitie_counter += degree;
                ret
            };
            for id in start..end + 1 {
                let sub_activities = if !is_last_layer {
                    get_sub_activities()
                } else {
                    vec![]
                };
                items.push(build_one(id, id, id, sub_activities));
            }
        };

        for l in 1..layer {
            build_layer(l, false);
        }
        build_layer(layer, true);

        items
    }

    #[test]
    pub fn test_binary_tree() {
        let items = build_tree(5, 2, None);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());

        let items = build_tree(5, 2, Some(vec![31]));
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 5);
        let bt = tree.panic_backtrace();
        // println!("{}", bt.as_ref().unwrap());
        assert!(bt.is_some());
    }

    #[test]
    pub fn test_binary_tree_reordered() {
        let mut rng = rand::thread_rng();

        let mut items = build_tree(5, 2, None);
        items.shuffle(&mut rng);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());

        // panic
        let mut items = build_tree(5, 2, Some(vec![31]));
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        items.shuffle(&mut rng);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 5);
        let bt = tree.panic_backtrace();
        // println!("{}", bt.as_ref().unwrap());
        assert!(bt.is_some());
    }

    #[test]
    pub fn test_binary_tree_reordered_multi_panic() {
        let mut rng = rand::thread_rng();

        let mut items = build_tree(7, 2, Some(vec![127, 70, 90]));
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        items.shuffle(&mut rng);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 7);
        let bt = tree.panic_backtrace();
        // println!("{}", bt.as_ref().unwrap());
        assert!(bt.is_some());
    }

    #[test]
    pub fn test_tree_reordered() {
        let mut rng = rand::thread_rng();

        let mut items = build_tree(6, 5, Some(vec![3000]));
        items.shuffle(&mut rng);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert_eq!(tree.panic_backtrace.len(), 6);
        let bt = tree.panic_backtrace();
        // println!("{}", bt.as_ref().unwrap());
        assert!(bt.is_some());
    }
}
