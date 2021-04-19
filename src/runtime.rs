use crate::activity::ActivityId;
use crate::activity::RemoteSend;
use crate::activity::Squashable;
use crate::activity::TaskItem;
use crate::activity::TaskItemBuilder;
use crate::activity::TaskItemExtracter;
use crate::finish::CallingTree;
use crate::finish::FinishId;
use crate::global_id;
use crate::global_id::ActivityIdLower;
use crate::global_id::ActivityIdMethods;
use crate::meta_data;
use crate::place::Place;
use lazy_static::lazy_static;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::any::Any;
use std::cell::Cell;
use std::sync::mpsc;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use tokio::sync::oneshot;

extern crate futures;
extern crate lazy_static;

// must access thess mutex in order
lazy_static! {
    static ref WORKDER_TASK_QUEUE: Mutex<(
        Option<Sender<Box<TaskItem>>>,
        Option<Receiver<Box<TaskItem>>>,
        )> = {
        let (task_tx, task_rx) = channel();
        Mutex::new((Some(task_tx), Some(task_rx)))
    };
    static ref TASK_ITEM_CHANNELS: Mutex<
        Vec<(
            Option<Sender<Box<TaskItem>>>,
            Option<Receiver<Box<TaskItem>>>,
            Option<Sender<WaitRequest>>,
            Option<Receiver<WaitRequest>>, // a channel to send channel
        )>,
    > = {
        let mut channels = vec![];
        for _ in 0..*meta_data::NUM_CPUS {
            let (task_tx, task_rx) = channel();
            let (wait_tx, wait_rx) = channel();
            channels.push((Some(task_tx), Some(task_rx), Some(wait_tx), Some(wait_rx)));
        }
        Mutex::new(channels)
    };
}

enum WaitItem {
    One(ActivityId),
    All(ConcreteContext),
}
type WaitRequest = Box<(WaitItem, oneshot::Sender<Box<TaskItem>>)>;

thread_local! {
    static TASK_ITEM_SENDER: Cell<Option<Sender<Box<TaskItem>>>> = Cell::new(None);
    static WAIT_SENDER: Cell<Option<Sender<WaitRequest>>> = Cell::new(None);
}

fn get_task_item_sender_ref() -> &'static Sender<Box<TaskItem>> {
    TASK_ITEM_SENDER.with(|s| {
        let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };

        if let Some(s_ref) = maybe_ref.as_ref() {
            s_ref
        } else {
            let wid = global_id::my_worker_id();
            let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
            let sender = channels[wid as usize].0.take().unwrap();
            s.set(Some(sender));
            get_task_item_sender_ref()
        }
    })
}

fn get_wait_request_sender_ref() -> &'static Sender<WaitRequest> {
    WAIT_SENDER.with(|s| {
        // TODO: dup code
        let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };

        if let Some(s_ref) = maybe_ref.as_ref() {
            s_ref
        } else {
            let wid = global_id::my_worker_id();
            let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
            let sender = channels[wid as usize].2.take().unwrap();
            s.set(Some(sender));
            get_wait_request_sender_ref()
        }
    })
}

pub trait ApgasContext {
    fn inherit(finish_id: FinishId) -> Self;
    fn new_frame() -> Self;
    fn spwaned(&self) -> Vec<ActivityId>;
    fn spwan(&mut self, place: Place) -> ActivityId;
    fn send(&self, item: Box<TaskItem>);
}

pub struct ConcreteContext {
    sub_activities: FxHashSet<ActivityId>,
    finish_id: FinishId,
}

impl ApgasContext for ConcreteContext {
    fn inherit(finish_id: FinishId) -> Self {
        ConcreteContext {
            sub_activities: FxHashSet::default(),
            finish_id,
        }
    }
    fn new_frame() -> Self {
        ConcreteContext {
            sub_activities: FxHashSet::default(),
            finish_id: global_id::new_global_finish_id(),
        }
    }
    fn spwaned(&self) -> Vec<ActivityId> {
        self.sub_activities.iter().cloned().collect()
    }

    fn spwan(&mut self, place: Place) -> ActivityId {
        let aid = global_id::new_global_activity_id(self.finish_id, place);
        self.sub_activities.insert(aid);
        aid
    }
    fn send(&self, item: Box<TaskItem>) {
        get_task_item_sender_ref().send(item);
    }
}

async fn wait_single_squash<T: Squashable>(ctx: &mut ConcreteContext, wait_this: ActivityId) -> T {
    ctx.sub_activities.remove(&wait_this);

    // TODO dup code
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref().send(Box::new((WaitItem::One(wait_this), tx)));
    let item = rx.await.unwrap();
    let mut ex = TaskItemExtracter::new(*item);
    let ret = ex.ret_squash::<T>();
    ret.unwrap() // assert no panic here TODO: deal with panic payload
}

async fn wait_single<T: RemoteSend>(ctx: &mut ConcreteContext, wait_this: ActivityId) -> T {
    ctx.sub_activities.remove(&wait_this);

    // TODO dup code
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref().send(Box::new((WaitItem::One(wait_this), tx)));
    let item = rx.await.unwrap();
    let mut ex = TaskItemExtracter::new(*item);
    let ret = ex.ret::<T>();
    ret.unwrap() // assert no panic here TODO: deal with panic payload
}

async fn wait_all(ctx: ConcreteContext) {
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref().send(Box::new((WaitItem::All(ctx), tx)));
    let item = rx.await.unwrap();
    let mut ex = TaskItemExtracter::new(*item);
    let ret = ex.ret_panic();
    ret.unwrap() // assert no panic here TODO: deal with panic payload
}

#[derive(Debug)]
struct RemoteItemReceiver {}

impl RemoteItemReceiver {
    fn recv(&self) -> Option<Box<TaskItem>> {
        None // TODO:
    }
}

#[derive(Debug)]
struct ExecutionHub {
    task_item_receivers: Vec<Receiver<Box<TaskItem>>>,
    wait_request_receivers: Vec<Receiver<WaitRequest>>,
    return_item_sender: FxHashMap<FinishId, oneshot::Sender<Box<TaskItem>>>,
    calling_trees: FxHashMap<FinishId, CallingTree>,
    free_items: FxHashMap<ActivityIdLower, Box<TaskItem>>, // all return value got
    single_wait: FxHashMap<ActivityIdLower, oneshot::Sender<Box<TaskItem>>>,
    stop: bool,
    remote_item_receiver: RemoteItemReceiver,
    worker_task_queue: Sender<Box<TaskItem>>,
}

impl ExecutionHub {
    pub fn new() -> Self {
        let worker_task_queue = {
            let mut q = WORKDER_TASK_QUEUE.lock().unwrap();
            q.0.take().unwrap()
        };
        let mut hub = ExecutionHub {
            task_item_receivers: vec![],
            wait_request_receivers: vec![],
            return_item_sender: FxHashMap::default(),
            calling_trees: FxHashMap::default(),
            free_items: FxHashMap::default(),
            single_wait: FxHashMap::default(),
            stop: false,
            remote_item_receiver: RemoteItemReceiver {},
            worker_task_queue,
        };
        {
            let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
            for i in 0..channels.len() {
                hub.task_item_receivers.push(channels[i].1.take().unwrap());
                hub.wait_request_receivers
                    .push(channels[i].3.take().unwrap());
            }
        }
        hub
    }

    // item is received from local or remote
    fn handle_item(&mut self, item: Box<TaskItem>) {
        if item.place() == global_id::here() {
            if item.is_ret() {
                let activity_id = item.activity_id();
                let finish_id = activity_id.get_finish_id();
                let mut tree_all_done = false;
                // waited by a single wait
                if let Some(sender) = self.single_wait.remove(&activity_id.get_lower()) {
                    sender.send(item).unwrap();
                // waited by an all wait
                } else if let Some(tree) = self.calling_trees.get_mut(&finish_id) {
                    tree.activity_done(item);
                    if tree.all_done() {
                        tree_all_done = true;
                    }
                } else {
                    self.free_items.insert(activity_id.get_lower(), item);
                }
                // clean up and send back
                if tree_all_done {
                    let tree = self.calling_trees.remove(&finish_id).unwrap();
                    let sender = self.return_item_sender.remove(&finish_id).unwrap();
                    Self::wait_all_finish_and_send_return(tree, sender);
                }
            } else {
                self.worker_task_queue.send(item).unwrap();
            }
        } else {
            // TODO: send to remote
        }
    }

    fn wait_all_finish_and_send_return(tree: CallingTree, sender: oneshot::Sender<Box<TaskItem>>) {
        let mut b = TaskItemBuilder::new(0, 0, 0);
        match tree.panic_backtrace() {
            None => b.ret(Ok(())), // build an empty ret
            Some(panic_payload) => b.ret(std::thread::Result::<()>::Err(
                Box::new(panic_payload) as Box<dyn Any + Send + 'static>
            )),
        }
        sender.send(Box::new(b.build()));
    }

    fn handle_wait_request(&mut self, wr: WaitRequest) {
        let (w_item, w_sender) = *wr;

        match w_item {
            WaitItem::One(aid) => {
                if let Some(task_item) = self.free_items.remove(&aid.get_lower()) {
                    // already finished, directly send back
                    w_sender.send(task_item).unwrap();
                } else {
                    self.single_wait.insert(aid.get_lower(), w_sender);
                }
            }
            WaitItem::All(ctx) => {
                let finish_id = ctx.finish_id;
                let mut new_tree = CallingTree::new(ctx.sub_activities.iter().cloned().collect());
                for sub_activity in ctx.sub_activities.iter() {
                    // if some free item already exist
                    if let Some(task_item) = self.free_items.remove(&sub_activity.get_lower()) {
                        new_tree.activity_done(task_item);
                    }
                }
                if new_tree.all_done() {
                    Self::wait_all_finish_and_send_return(new_tree, w_sender);
                } else {
                    self.calling_trees.insert(finish_id, new_tree);
                    self.return_item_sender.insert(finish_id, w_sender);
                }
            }
        };
    }

    pub fn run(&mut self) {
        loop {
            // receive task item
            for i in 0..self.task_item_receivers.len() {
                match self.task_item_receivers[i].try_recv() {
                    Ok(item) => {
                        if item.place() == global_id::here() {
                            self.handle_item(item);
                        } else {
                            // TODO: send remote
                            panic!("shit!");
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => (),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        panic!("{} task_item_channel closed", i)
                    }
                }
            }
            // reveive wait request
            for i in 0..self.wait_request_receivers.len() {
                match self.wait_request_receivers[i].try_recv() {
                    Ok(wr) => self.handle_wait_request(wr),
                    Err(mpsc::TryRecvError::Empty) => (),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        panic!("{} wait_request_channel closed", i)
                    }
                }
            }

            while let Some(item) = self.remote_item_receiver.recv() {
                self.handle_item(item);
            }
        }
    }
}
