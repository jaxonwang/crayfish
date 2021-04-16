use crate::activity::ActivityId;
use crate::activity::FunctionLabel;
use crate::activity::ReturnInfo;
use crate::activity::StrippedTaskItem;
use crate::activity::TaskItem;
use crate::place::Place;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;

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
struct CallingTree {
    lookup_table: FxHashMap<ActivityId, CallingTreeNode>,
    panic_backtrace: Vec<FrameInfo>,
}

const ROOT_ID: ActivityId = 0;

impl CallingTree {
    pub fn new(initial_call: Vec<ActivityId>) -> Self {
        let mut tree = CallingTree {
            lookup_table: FxHashMap::default(),
            panic_backtrace: Vec::new(),
        };
        tree.new_root(ROOT_ID, &initial_call[..], None);
        tree
    }
    fn new_root(&mut self, id: ActivityId, children: &[ActivityId], frame: Option<FrameInfo>) {
        let ret = self.lookup_table.insert(
            id,
            CallingTreeNode {
                id,
                frame,
                parent: None,
                children: children.iter().cloned().collect(),
            },
        );
        debug_assert!(ret.is_none()); // root is not a child of any node
        for child in children.iter() {
            if !self.lookup_table.contains_key(child){ // child might exist
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
        self.lookup_table.is_empty()
    }

    pub fn activity_done(&mut self, item: Box<TaskItem>) {
        let TaskItem {
            inner:
                StrippedTaskItem {
                    fn_id,
                    place,
                    activity_id,
                    ret,
                    args: _,
                },
            squashable: _,
        } = *item;
        let ReturnInfo {
            result,
            sub_activities,
        } = ret.unwrap();
        let panic_payload = match result {
            Ok(()) => None,
            Err(payload) => Some(payload),
        };

        let frame = FrameInfo {
            fn_id,
            place,
            panic_payload,
        };
        match self.lookup_table.get_mut(&activity_id) {
            Some(node) => {
                node.frame = Some(frame);
                debug_assert!(node.parent.is_some());
                node.children = sub_activities.iter().cloned().collect()
            }
            None => self.new_root(activity_id, &sub_activities[..], Some(frame)),
        }

        for child in sub_activities.iter() {
            if !self.lookup_table.contains_key(child) {
                self.lookup_table.insert(
                    *child,
                    CallingTreeNode {
                        id: *child,
                        parent: Some(activity_id),
                        children: FxHashSet::default(),
                        frame: None,
                    },
                );
            }
            self.link_parent_child(activity_id, *child)
        }

        // start roll back
        let mut current_node_id = activity_id;
        let mut find_panic = false;
        let mut backtrace = vec![];
        while self
            .lookup_table
            .get(&current_node_id)
            .unwrap()
            .children
            .is_empty()
        {
            let node = self.lookup_table.remove(&current_node_id).unwrap();
            if let Some(frame) = node.frame.clone() {
                if frame.panic_payload.is_some() {
                    find_panic = true;
                }
                if find_panic {
                    backtrace.push(frame);
                }
            }
            match node.parent {
                None => break,
                Some(p) => {
                    self.lookup_table
                        .get_mut(&p)
                        .unwrap()
                        .children
                        .remove(&current_node_id);
                    current_node_id = p;
                }
            };
        }
        if !backtrace.is_empty() && self.panic_backtrace.is_empty() {
            // only store the fist panic backtrace
            self.panic_backtrace = backtrace;
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
    use crate::activity::ActivityResult;
    use rand;
    #[test]
    pub fn test_calling_tree() {
        let build_chain = || {
            let build_one = |fn_id, place, a_id, sub_activity_id| {
                let mut b = TaskItemBuilder::new(fn_id, place, a_id);
                b.ret(std::thread::Result::Ok(1usize));
                b.sub_activities(vec![sub_activity_id]);
                b.build()
            };

            let mut items: Vec<_> = (1..24)
                .map(|i| {
                    Box::new(build_one(
                        i as FunctionLabel,
                        i as Place,
                        i as ActivityId,
                        (i + 1) as ActivityId,
                    ))
                })
                .collect();
            let last_idx = items.len() - 1;
            (*items[last_idx])
                .inner
                .ret
                .as_mut()
                .unwrap()
                .sub_activities = vec![];
            items
        };

        /*
        // test a chain
        let items = build_chain();
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());

        // test a chain with panic
        let mut items = build_chain();
            (*items[10])
                .inner
                .ret
                .as_mut().unwrap().result = ActivityResult::Err(String::from("panic here"));
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        
        assert!(tree.all_done());
        let bt =tree.panic_backtrace();
        // println!("{}", bt.unwrap());
        assert!(bt.is_some());
        */

        // test reorder
        let mut items = build_chain();    
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        items.shuffle(&mut rng);
        let mut tree = CallingTree::new(vec![1 as ActivityId]);
        for item in items {
            assert!(!tree.all_done());
            tree.activity_done(item);
        }
        println!("{:?}", tree);
        assert!(tree.all_done());
        assert!(tree.panic_backtrace().is_none());
    }
}
