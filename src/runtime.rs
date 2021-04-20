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
use crate::logging::*;
use crate::meta_data;
use crate::place::Place;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::any::Any;
use std::cell::Cell;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::oneshot;

extern crate futures;
extern crate once_cell;

fn init_worker_task_queue() {
    let mut q = WORKER_TASK_QUEUE.lock().unwrap();
    let (task_tx, task_rx) = channel();
    *q = (Some(task_tx), Some(task_rx));
}

fn init_task_item_channels() {
    let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
    channels.clear();
    for _ in 0..*meta_data::NUM_CPUS {
        // TODO: configurable thread num
        let (task_tx, task_rx) = channel();
        let (wait_tx, wait_rx) = channel();
        channels.push((Some(task_tx), Some(task_rx), Some(wait_tx), Some(wait_rx)));
    }
}

// must access thess mutex in order
static WORKER_TASK_QUEUE: Lazy<
    Mutex<(
        Option<Sender<Box<TaskItem>>>,
        Option<Receiver<Box<TaskItem>>>,
    )>,
> = Lazy::new(|| Mutex::new((None, None)));
static TASK_ITEM_CHANNELS: Lazy<
    Mutex<
        Vec<(
            Option<Sender<Box<TaskItem>>>,
            Option<Receiver<Box<TaskItem>>>,
            Option<Sender<WaitRequest>>,
            Option<Receiver<WaitRequest>>, // a channel to send channel
        )>,
    >,
> = Lazy::new(|| Mutex::new(vec![]));

#[derive(Debug)]
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

#[derive(Debug)]
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
        get_task_item_sender_ref().send(item).unwrap();
    }
}

pub fn wait_single_squash<T: Squashable>(
    ctx: &mut ConcreteContext,
    wait_this: ActivityId,
) -> impl futures::Future<Output = T> {
    ctx.sub_activities.remove(&wait_this);

    // TODO dup code
    async move {
        let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
        get_wait_request_sender_ref()
            .send(Box::new((WaitItem::One(wait_this), tx)))
            .unwrap();
        let item = rx.await.unwrap();
        let mut ex = TaskItemExtracter::new(*item);
        let ret = ex.ret_squash::<T>();
        ret.unwrap() // assert no panic here TODO: deal with panic payload
    }
}

pub fn wait_single<T: RemoteSend>(
    ctx: &mut ConcreteContext,
    wait_this: ActivityId,
) -> impl futures::Future<Output = T> {
    ctx.sub_activities.remove(&wait_this);

    // TODO dup code
    async move {
        let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
        get_wait_request_sender_ref()
            .send(Box::new((WaitItem::One(wait_this), tx)))
            .unwrap();
        let item = rx.await.unwrap();
        let mut ex = TaskItemExtracter::new(*item);
        let ret = ex.ret::<T>();
        ret.unwrap() // assert no panic here TODO: deal with panic payload
    }
}

pub async fn wait_all(ctx: ConcreteContext) {
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref()
        .send(Box::new((WaitItem::All(ctx), tx)))
        .unwrap();
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
pub struct ExecutionHub {
    task_item_receivers: Vec<Option<Receiver<Box<TaskItem>>>>,
    wait_request_receivers: Vec<Option<Receiver<WaitRequest>>>,
    return_item_sender: FxHashMap<FinishId, oneshot::Sender<Box<TaskItem>>>,
    calling_trees: FxHashMap<FinishId, CallingTree>,
    free_items: FxHashMap<ActivityIdLower, Box<TaskItem>>, // all return value got
    single_wait: FxHashMap<ActivityIdLower, oneshot::Sender<Box<TaskItem>>>,
    stop: Arc<AtomicBool>,
    remote_item_receiver: RemoteItemReceiver,
    worker_task_queue: Sender<Box<TaskItem>>,
}

pub struct ExecutionHubTrigger {
    stop: Arc<AtomicBool>,
}
impl ExecutionHubTrigger {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl ExecutionHub {
    pub fn new() -> Self {
        let worker_task_queue = {
            let mut q = WORKER_TASK_QUEUE.lock().unwrap();
            q.0.take().unwrap()
        };
        let mut hub = ExecutionHub {
            task_item_receivers: vec![],
            wait_request_receivers: vec![],
            return_item_sender: FxHashMap::default(),
            calling_trees: FxHashMap::default(),
            free_items: FxHashMap::default(),
            single_wait: FxHashMap::default(),
            stop: Arc::new(AtomicBool::new(false)),
            remote_item_receiver: RemoteItemReceiver {},
            worker_task_queue,
        };
        {
            let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
            for i in 0..channels.len() {
                hub.task_item_receivers
                    .push(Some(channels[i].1.take().unwrap()));
                hub.wait_request_receivers
                    .push(Some(channels[i].3.take().unwrap()));
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
        sender.send(Box::new(b.build())).unwrap();
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
        while !self.stop.load(Ordering::Acquire) {
            // receive task item
            for i in 0..self.task_item_receivers.len() {
                let mut to_remove = false;
                if let Some(r) = self.task_item_receivers[i].as_ref() {
                    match r.try_recv() {
                        Ok(item) => self.handle_item(item),
                        Err(mpsc::TryRecvError::Empty) => (),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            debug!("worker:{} task_item_receiver closed", i);
                            to_remove = true;
                        }
                    }
                }
                if to_remove {
                    self.task_item_receivers[i] = None;
                }
            }
            // reveive wait request
            for i in 0..self.wait_request_receivers.len() {
                let mut to_remove = false;
                if let Some(r) = self.wait_request_receivers[i].as_ref() {
                    match r.try_recv() {
                        Ok(wr) => self.handle_wait_request(wr),
                        Err(mpsc::TryRecvError::Empty) => (),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            debug!("worker:{} wait_request_channel closed", i);
                            to_remove = true;
                        }
                    }
                }
                if to_remove {
                    self.wait_request_receivers[i] = None;
                }
            }

            while let Some(item) = self.remote_item_receiver.recv() {
                self.handle_item(item);
            }
        }
    }

    pub fn get_trigger(&self) -> ExecutionHubTrigger {
        ExecutionHubTrigger {
            stop: self.stop.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::global_id::test::global_id_reset_everything;
    use crate::global_id::test::TestGuardForStatic;
    use crate::global_id::*;
    use futures::executor;
    use futures::task::Poll;
    use std::pin::Pin;
    use std::sync::MutexGuard;
    use std::thread;
    use std::time;

    struct RuntimeTestGuard<'a> {
        _guard: TestGuardForStatic<'a>,
    }

    impl<'a> RuntimeTestGuard<'a> {
        fn new() -> Self {
            let ret = RuntimeTestGuard {
                _guard: TestGuardForStatic::new(),
            };
            init_worker_task_queue();
            init_task_item_channels();
            TASK_ITEM_SENDER.with(|s| s.set(None));
            WAIT_SENDER.with(|s| s.set(None));
            ret
        }
    }
    // impl<'a> Drop for RuntimeTestGuard<'a>{
    //     fn drop(&mut self){
    //         println!("stop");
    //     }
    // }

    struct ShouldNotReturnUntil {
        join_handle: Option<thread::JoinHandle<()>>,
        can_return: Arc<AtomicBool>,
    }

    impl ShouldNotReturnUntil {
        fn new<F>(f: F) -> Self
        where
            F: FnOnce() + Send + 'static,
        {
            let mut ret = ShouldNotReturnUntil {
                join_handle: None,
                can_return: Arc::new(AtomicBool::new(false)),
            };
            let can_return = ret.can_return.clone();
            ret.join_handle = Some(thread::spawn(move || {
                let can_return = can_return;
                f();
                assert_eq!(
                    can_return.load(Ordering::Acquire),
                    true,
                    "should not return!"
                );
            }));
            ret
        }
        fn can_return_now(&self) {
            self.can_return.store(true, Ordering::Release);
        }
    }
    impl Drop for ShouldNotReturnUntil {
        fn drop(&mut self) {
            self.join_handle.take().unwrap().join().unwrap();
        }
    }

    struct ExecutorHubSetUp<'a> {
        test_guard: RuntimeTestGuard<'a>,
        join_handle: Option<thread::JoinHandle<()>>,
        trigger: ExecutionHubTrigger,
    }
    impl<'a> ExecutorHubSetUp<'a> {
        fn new() -> Self {
            let test_guard = RuntimeTestGuard::new();
            let mut hub = ExecutionHub::new();
            let trigger = hub.get_trigger();
            let join_handle = Some(thread::spawn(move || hub.run()));
            ExecutorHubSetUp {
                test_guard,
                trigger,
                join_handle,
            }
        }
    }
    impl<'a> Drop for ExecutorHubSetUp<'a> {
        fn drop(&mut self) {
            // stop all
            self.trigger.stop();
            self.join_handle.take().unwrap().join().unwrap();
        }
    }
    // impl Drop for ExecutionHub{
    //     fn drop(&mut self){
    //         println!("i amd function stop!");
    //     }
    // }
    #[test]
    fn test_single_wait_local_send_after_wait_no_panic() {
        let _e = ExecutorHubSetUp::new();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let here = global_id::here();
        assert_eq!(here, ctx.finish_id.get_place());
        // register a sub activity
        let new_aid = ctx.spwan(here);
        let f = wait_single::<usize>(&mut ctx, new_aid);
        let return_value = 123456;
        let can_ret =
            ShouldNotReturnUntil::new(move || assert_eq!(executor::block_on(f), return_value));
        // prepare and send return
        let mut builder = TaskItemBuilder::new(0, here, new_aid);
        builder.ret(thread::Result::<usize>::Ok(return_value));
        let item = builder.build_box();
        thread::sleep(time::Duration::from_millis(1));
        can_ret.can_return_now();
        ctx.send(item);
        drop(can_ret);
        // wait all
        executor::block_on(wait_all(ctx));
    }

    #[test]
    fn test_single_wait_local_send_many_wait_no_panic() {
        use crate::activity::test::A; // TODO: write test utilities
        let _e = ExecutorHubSetUp::new();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let here = global_id::here();
        let mut wait_these = vec![];
        let mut wait_squash = vec![];
        // prepare and send return
        for return_value in 0..1024 {
            // register a sub activity
            let new_aid = ctx.spwan(here);
            wait_these.push((wait_single::<usize>(&mut ctx, new_aid), return_value));
            let mut builder = TaskItemBuilder::new(0, here, new_aid);
            builder.ret(thread::Result::<usize>::Ok(return_value));
            let item = builder.build_box();
            ctx.send(item);
        }

        for value in 0..1024 {
            let new_aid = ctx.spwan(here);
            let return_a = A{value};
            wait_squash.push((wait_single_squash::<A>(&mut ctx, new_aid), return_a.clone()));
            let mut builder = TaskItemBuilder::new(0, here, new_aid);
            builder.ret_squash(thread::Result::<A>::Ok(return_a));
            let item = builder.build_box();
            ctx.send(item);
        }
        for (f, v) in wait_these {
            assert_eq!(executor::block_on(f), v);
        }
        for (f, v) in wait_squash{
            assert_eq!(executor::block_on(f), v);
        }
        // wait all
        executor::block_on(wait_all(ctx));
    }

    #[test]
    #[should_panic]
    fn test_single_wait_local_should_panic() {
        let _e = ExecutorHubSetUp::new();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let here = global_id::here();

        let new_aid = ctx.spwan(here);
        let f = wait_single::<usize>(&mut ctx, new_aid);
        let mut builder = TaskItemBuilder::new(0, here, new_aid);
        let j = thread::spawn(|| panic!("I panic!"));
        builder.ret(j.join());
        let item = builder.build_box();
        ctx.send(item);

        executor::block_on(f);
    }
}
