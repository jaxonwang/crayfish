use crate::activity::AbstractSquashBuffer;
use crate::activity::AbstractSquashBufferFactory;
use crate::activity::ActivityId;
use crate::activity::RemoteSend;
use crate::activity::Squashable;
use crate::activity::StaticSquashBufferFactory;
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
use crate::network::MessageSender;
use crate::network::Rank;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use std::any::Any;
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::time;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

extern crate futures;
extern crate once_cell;

pub fn init_worker_task_queue() {
    let mut q = WORKER_TASK_QUEUE.lock().unwrap();
    let (task_tx, task_rx) = unbounded_channel();
    *q = (Some(task_tx), Some(task_rx));
}

pub fn init_task_item_channels() {
    let mut channels = TASK_ITEM_CHANNELS.lock().unwrap();
    channels.clear();
    for _ in 0..*meta_data::NUM_CPUS {
        // TODO: configurable thread num
        let (task_tx, task_rx) = channel();
        let (wait_tx, wait_rx) = channel();
        channels.push((Some(task_tx), Some(task_rx), Some(wait_tx), Some(wait_rx)));
    }
}
type MaybeTaskChannel = (
    Option<UnboundedSender<Box<TaskItem>>>,
    Option<UnboundedReceiver<Box<TaskItem>>>,
);
type TaskWaitChannels = Vec<(
    Option<Sender<Box<TaskItem>>>,
    Option<Receiver<Box<TaskItem>>>,
    Option<Sender<Box<WaitRequest>>>,
    Option<Receiver<Box<WaitRequest>>>, // a channel to send channel
)>;
type MaybeBufferChannel = (
    // send squash buffer from network layer to distrbutor
    Option<Sender<Box<dyn AbstractSquashBuffer>>>,
    Option<Receiver<Box<dyn AbstractSquashBuffer>>>,
);
// must access these mutexes in order
static WORKER_TASK_QUEUE: Lazy<Mutex<MaybeTaskChannel>> = Lazy::new(|| Mutex::new((None, None)));
static TASK_ITEM_CHANNELS: Lazy<Mutex<TaskWaitChannels>> = Lazy::new(|| Mutex::new(vec![]));
static NETWORK_BUFFER_CHANNEL: Lazy<Mutex<MaybeBufferChannel>> = Lazy::new(|| {
    let (tx, rx) = mpsc::channel();
    Mutex::new((Some(tx), Some(rx)))
});

pub fn message_recv_callback<F: StaticSquashBufferFactory>(_src: Rank, data: &[u8]) {
    thread_local! { // this is hold in network thread
        static BUFFER_SENDER: Cell<Option<Sender<Box<dyn AbstractSquashBuffer>>>> = Cell::new(None);
    };

    let get_ref = || {
        BUFFER_SENDER.with(|s| loop {
            let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };
            if let Some(s_ref) = maybe_ref.as_ref() {
                break s_ref;
            } else {
                let mut channel = NETWORK_BUFFER_CHANNEL.lock().unwrap();
                let sender = channel.0.take().unwrap();
                s.set(Some(sender));
            }
        })
    };

    get_ref().send(F::deserialize_from(data)).unwrap();
}

// take the receiver from static, to init distributor
pub fn take_message_buffer_receiver() -> Receiver<Box<dyn AbstractSquashBuffer>> {
    NETWORK_BUFFER_CHANNEL.lock().unwrap().1.take().unwrap()
}

pub fn take_worker_task_receiver() -> UnboundedReceiver<Box<TaskItem>> {
    WORKER_TASK_QUEUE.lock().unwrap().1.take().unwrap()
}

#[derive(Debug)]
enum WaitItem {
    One(ActivityId),
    All(ConcreteContext),
}
type WaitRequest = (WaitItem, oneshot::Sender<Box<TaskItem>>);

thread_local! {
    static TASK_ITEM_SENDER: Cell<Option<Sender<Box<TaskItem>>>> = Cell::new(None);
    static WAIT_SENDER: Cell<Option<Sender<Box<WaitRequest>>>> = Cell::new(None);
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

fn get_wait_request_sender_ref() -> &'static Sender<Box<WaitRequest>> {
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

pub trait ApgasContext: Send {
    fn inherit(finish_id: FinishId) -> Self;
    fn new_frame() -> Self;
    fn spawned(self) -> Vec<ActivityId>;
    fn spawn(&mut self) -> ActivityId;
    fn send(item: Box<TaskItem>);
}

#[derive(Debug)]
pub struct ConcreteContext {
    sub_activities: Vec<ActivityId>,
    finish_id: FinishId,
}

impl ApgasContext for ConcreteContext {
    fn inherit(finish_id: FinishId) -> Self {
        ConcreteContext {
            sub_activities: vec![],
            finish_id,
        }
    }
    fn new_frame() -> Self {
        ConcreteContext {
            sub_activities: vec![],
            finish_id: global_id::new_global_finish_id(),
        }
    }
    fn spawned(self) -> Vec<ActivityId> {
        self.sub_activities
    }

    fn spawn(&mut self) -> ActivityId {
        let aid = global_id::new_global_activity_id(self.finish_id);
        self.sub_activities.push(aid);
        aid
    }

    fn send(item: Box<TaskItem>) {
        get_task_item_sender_ref().send(item).unwrap();
    }
}

pub async fn wait_single<T: RemoteSend>(wait_this: ActivityId) -> T {
    // TODO dup code
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref()
        .send(Box::new((WaitItem::One(wait_this), tx)))
        .unwrap();
    let item = rx.await.unwrap();
    let mut ex = TaskItemExtracter::new(*item);
    let ret = ex.ret::<T>();
    ret.unwrap() // assert no panic here TODO: deal with panic payload
}

pub async fn wait_single_squash<T: Squashable>(wait_this: ActivityId) -> T {
    // TODO dup code
    let (tx, rx) = oneshot::channel::<Box<TaskItem>>();
    get_wait_request_sender_ref()
        .send(Box::new((WaitItem::One(wait_this), tx)))
        .unwrap();
    let item = rx.await.unwrap();
    let mut ex = TaskItemExtracter::new(*item);
    let ret = ex.ret_squash::<T>();
    ret.unwrap() // assert no panic here TODO: deal with panic payload
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

pub trait AbstractDistributor: Send + 'static {
    fn recv(&mut self) -> Option<Box<TaskItem>>;
    fn send(&mut self, item: Box<TaskItem>);
    fn poll(&mut self);
}

// I guess 1 ms is the RTT for ethernet
const MAX_BUFFER_LIFETIME: time::Duration = time::Duration::from_millis(1);
// who will perfrom squash and inflate
pub struct Distributor<S: MessageSender> {
    out_buffers: Vec<(Box<dyn AbstractSquashBuffer>, time::Instant)>,
    sender: S,
    receiver: Receiver<Box<dyn AbstractSquashBuffer>>,
    in_buffers: VecDeque<Box<dyn AbstractSquashBuffer>>,
}

impl<S> Distributor<S>
where
    S: MessageSender,
{
    pub fn new(
        buffer_factory: Box<dyn AbstractSquashBufferFactory>,
        world_size: usize,
        sender: S,
        receiver: Receiver<Box<dyn AbstractSquashBuffer>>,
    ) -> Self {
        let out_buffers: Vec<_> = (0..world_size)
            .map(|_| (buffer_factory.new_buffer(), time::Instant::now()))
            .collect();
        Distributor {
            out_buffers,
            sender,
            receiver,
            in_buffers: VecDeque::with_capacity(512),
        }
    }

    // check whether buffer is goint te be send will squash and send
    fn poll_one(&mut self, idx: usize) {
        let dst_buffer = &mut self.out_buffers[idx];
        if dst_buffer.0.is_empty() || dst_buffer.1 + MAX_BUFFER_LIFETIME > time::Instant::now() {
            return; // young enough, do nothing
        }
        trace!("buffer:{} timeout. send", idx);
        dst_buffer.0.squash_all();
        let bytes = dst_buffer.0.serialize_and_clear();
        self.sender.send_msg(Rank::from_usize(idx), bytes);
    }
}

impl<S> AbstractDistributor for Distributor<S>
where
    S: MessageSender,
{
    // will extract
    fn recv(&mut self) -> Option<Box<TaskItem>> {
        loop {
            let front = match self.in_buffers.front_mut() {
                None => match self.receiver.try_recv() {
                    // in buffers are emtpy, try to receive new squash buffers
                    Ok(buffer) => {
                        let mut buffer = buffer;
                        debug_assert!(!buffer.is_empty());
                        buffer.extract_all();
                        self.in_buffers.push_back(buffer);
                        continue;
                    }
                    Err(mpsc::TryRecvError::Empty) => return None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        panic!("network recive channel closed")
                    }
                },
                Some(f) => f,
            };
            let maybe_item = front.pop();
            match maybe_item {
                None => {
                    // the head is empty now, pop it, and try next
                    self.in_buffers.pop_front();
                    continue;
                }
                Some(item) => return Some(Box::new(item)),
            }
        }
    }

    #[allow(clippy::boxed_local)] // allow since TaskItem is going to be sent
    fn send(&mut self, item: Box<TaskItem>) {
        // should never send to local, in my implementation
        debug_assert!(item.place() != global_id::here());
        let idx = item.place() as usize;
        let dst_buffer = &mut self.out_buffers[idx];
        if dst_buffer.0.is_empty() {
            // touch the time when the first item comes in
            dst_buffer.1 = time::Instant::now();
        }
        dst_buffer.0.push(*item);
        self.poll_one(idx);
    }

    fn poll(&mut self) {
        for i in 0..self.out_buffers.len() {
            self.poll_one(i);
        }
    }
}

#[derive(Debug)]
pub struct ExecutionHub<D>
where
    D: AbstractDistributor,
{
    task_item_receivers: Vec<Option<Receiver<Box<TaskItem>>>>,
    wait_request_receivers: Vec<Option<Receiver<Box<WaitRequest>>>>,
    return_item_sender: FxHashMap<FinishId, oneshot::Sender<Box<TaskItem>>>,
    calling_trees: FxHashMap<FinishId, CallingTree>,
    #[allow(clippy::vec_box)] // I think store a pointer is faster
    free_items: FxHashMap<FinishId, Vec<Box<TaskItem>>>, // all return value got
    single_wait_free_items: FxHashMap<ActivityIdLower, Box<TaskItem>>, // all return value got
    single_wait: FxHashMap<ActivityIdLower, oneshot::Sender<Box<TaskItem>>>,
    stop: Arc<AtomicBool>,
    distributor: D,
    worker_task_queue: UnboundedSender<Box<TaskItem>>,
}

pub struct ExecutionHubTrigger {
    stop: Arc<AtomicBool>,
}
impl ExecutionHubTrigger {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl<D> ExecutionHub<D>
where
    D: AbstractDistributor,
{
    pub fn new(distributor: D) -> Self {
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
            single_wait_free_items: FxHashMap::default(),
            stop: Arc::new(AtomicBool::new(false)),
            distributor,
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
                trace!("got return item to self {:?}", item);
                // if is finish but waited here, the item.place() == here()
                let activity_id = item.activity_id();
                let finish_id = activity_id.get_finish_id();
                let mut tree_all_done = false;
                if item.is_waited() {
                    trace!("waited single {:?}", item);
                    // waited by a single wait
                    if let Some(sender) = self.single_wait.remove(&activity_id.get_lower()) {
                        sender.send(item).unwrap();
                    } else {
                        // not yet waited, go to free item
                        self.single_wait_free_items
                            .insert(activity_id.get_lower(), item);
                    }
                // waited by an all wait
                } else if let Some(tree) = self.calling_trees.get_mut(&finish_id) {
                    trace!("waited all {:?}", item);
                    tree.activity_done(*item);
                    if tree.all_done() {
                        tree_all_done = true; // use another flag to pass borrow checker
                    }
                } else {
                    trace!("waited free {:?}", item);
                    // not waited, go to free items
                    if let Some(task_items) = self.free_items.get_mut(&finish_id) {
                        task_items.push(item);
                    } else {
                        self.free_items.insert(finish_id, vec![item]);
                    }
                }
                // clean up and send back
                if tree_all_done {
                    let tree = self.calling_trees.remove(&finish_id).unwrap();
                    let sender = self.return_item_sender.remove(&finish_id).unwrap();
                    Self::calling_tree_complete_send_return(tree, sender);
                }
            } else {
                // is request
                self.worker_task_queue.send(item).unwrap(); // executor quit first
            }
        } else {
            trace!("got item to remote: {:?}", item);
            // not local, send to remote
            self.distributor.send(item);
        }
    }

    fn calling_tree_complete_send_return(
        tree: CallingTree,
        sender: oneshot::Sender<Box<TaskItem>>,
    ) {
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
        let (w_item, w_sender) = wr;

        match w_item {
            WaitItem::One(aid) => {
                trace!("got single request {}", aid);
                if let Some(task_item) = self.single_wait_free_items.remove(&aid.get_lower()) {
                    // already finished, directly send back
                    w_sender.send(task_item).unwrap();
                } else {
                    self.single_wait.insert(aid.get_lower(), w_sender);
                }
            }
            WaitItem::All(ctx) => {
                trace!("got all request :{:?}", ctx);
                let finish_id = ctx.finish_id;
                let mut new_tree = CallingTree::new(ctx.sub_activities);
                // if some free item already exist
                if let Some(task_items) = self.free_items.remove(&finish_id) {
                    for task_item in task_items {
                        new_tree.activity_done(*task_item);
                    }
                }
                if new_tree.all_done() {
                    Self::calling_tree_complete_send_return(new_tree, w_sender);
                } else {
                    self.calling_trees.insert(finish_id, new_tree);
                    self.return_item_sender.insert(finish_id, w_sender);
                }
            }
        };
    }

    pub fn run(&mut self) {
        while !self.stop.load(Ordering::Acquire) {
            // TODO: backoff
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
                        Ok(wr) => self.handle_wait_request(*wr),
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

            // receive from remote
            while let Some(item) = self.distributor.recv() {
                trace!("got item from remote {:?}", item);
                self.handle_item(item);
            }

            // send pending buffers
            self.distributor.poll();
        }
        debug!("Execution Hub shuts down");
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
    use crate::activity::test::TestGuardForStatic as ATestGuard;
    use crate::activity::test::A;
    use crate::global_id::test::TestGuardForStatic as GTestGuard;
    use crate::global_id::*;
    use futures::executor;
    use std::thread;
    use std::time; // TODO: write test utilities

    struct RuntimeTestGuard<'a> {
        _guard_a: ATestGuard<'a>,
        _guard_g: GTestGuard<'a>,
    }

    impl<'a> RuntimeTestGuard<'a> {
        fn new() -> Self {
            let ret = RuntimeTestGuard {
                _guard_a: ATestGuard::new(),
                _guard_g: GTestGuard::new(),
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

    struct FakeDistributor {}

    impl AbstractDistributor for FakeDistributor {
        fn recv(&mut self) -> Option<Box<TaskItem>> {
            None
        }
        fn send(&mut self, _item: Box<TaskItem>) {}
        fn poll(&mut self) {}
    }

    struct ExecutorHubSetUp<'a> {
        _test_guard: RuntimeTestGuard<'a>,
        join_handle: Option<thread::JoinHandle<()>>,
        trigger: ExecutionHubTrigger,
    }
    impl<'a> ExecutorHubSetUp<'a> {
        fn new(distrbutor: impl AbstractDistributor) -> Self {
            let _test_guard = RuntimeTestGuard::new();
            let mut hub = ExecutionHub::new(distrbutor);
            let trigger = hub.get_trigger();
            let join_handle = Some(thread::spawn(move || hub.run()));
            ExecutorHubSetUp {
                _test_guard,
                trigger,
                join_handle,
            }
        }

        fn new_with_fake() -> Self {
            let _test_guard = RuntimeTestGuard::new();
            let mut hub = ExecutionHub::new(FakeDistributor {});
            let trigger = hub.get_trigger();
            let join_handle = Some(thread::spawn(move || hub.run()));
            ExecutorHubSetUp {
                _test_guard,
                trigger,
                join_handle,
            }
        }
    }
    impl<'a> Drop for ExecutorHubSetUp<'a> {
        fn drop(&mut self) {
            // stop all
            self.trigger.stop();
            // unwinder panic will sigill!
            if self.join_handle.take().unwrap().join().is_err() {
                eprintln!("executor thread quit with error")
            }
        }
    }

    fn send_2_local<T: RemoteSend>(
        aid: ActivityId,
        result_value: T,
        sub_activities: Vec<ActivityId>,
    ) {
        let here = global_id::here();
        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret(thread::Result::<()>::Ok(())); // for tree, no ret
        builder.sub_activities(sub_activities);
        ConcreteContext::send(builder.build_box());

        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret(thread::Result::<T>::Ok(result_value));
        builder.waited();
        ConcreteContext::send(builder.build_box());
    }

    fn send_2_local_panic(aid: ActivityId, sub_activities: Vec<ActivityId>) {
        let here = global_id::here();
        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret(thread::Result::<()>::Ok(())); // for tree, no ret
        builder.sub_activities(sub_activities);
        ConcreteContext::send(builder.build_box());

        let mut builder = TaskItemBuilder::new(0, here, aid);
        let j = thread::spawn(|| panic!("I panic!"));
        builder.ret(j.join());
        builder.waited();
        ConcreteContext::send(builder.build_box());
    }

    fn send_2_squash_local<T: Squashable>(
        aid: ActivityId,
        result_value: T,
        sub_activities: Vec<ActivityId>,
    ) {
        let here = global_id::here();
        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret(thread::Result::<()>::Ok(())); // for tree, no ret
        builder.sub_activities(sub_activities);
        ConcreteContext::send(builder.build_box());

        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret_squash(thread::Result::<T>::Ok(result_value));
        builder.waited();
        ConcreteContext::send(builder.build_box());
    }

    fn send_1_local(aid: ActivityId, sub_activities: Vec<ActivityId>) {
        let here = global_id::here();
        let mut builder = TaskItemBuilder::new(0, here, aid);
        builder.ret(thread::Result::<()>::Ok(())); // for tree, no ret
        builder.sub_activities(sub_activities);
        ConcreteContext::send(builder.build_box());
    }

    #[test]
    fn test_single_wait_local_send_after_wait() {
        let _e = ExecutorHubSetUp::new_with_fake();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let here = global_id::here();
        assert_eq!(here, ctx.finish_id.get_place());
        // register a sub activity
        let new_aid = ctx.spawn();
        let f = wait_single::<usize>(new_aid);
        let return_value = 123456;
        let can_ret =
            ShouldNotReturnUntil::new(move || assert_eq!(executor::block_on(f), return_value));
        // prepare and send return
        thread::sleep(time::Duration::from_millis(1));
        can_ret.can_return_now();
        // send back to single wait and finish block owner
        send_2_local(new_aid, return_value, vec![]);
        drop(can_ret);
        // wait all
        executor::block_on(wait_all(ctx));
    }

    #[test]
    fn test_single_wait_local_send_many_wait() {
        let _e = ExecutorHubSetUp::new_with_fake();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let mut wait_these = vec![];
        let mut wait_squash = vec![];
        // prepare and send return
        for return_value in 0..1024 {
            // register a sub activity
            let new_aid = ctx.spawn();
            wait_these.push((wait_single::<usize>(new_aid), return_value));
            send_2_local(new_aid, return_value, vec![]);
        }

        for value in 0..1024 {
            let new_aid = ctx.spawn();
            let return_a = A { value };
            wait_squash.push((wait_single_squash::<A>(new_aid), return_a.clone()));
            send_2_squash_local(new_aid, return_a, vec![]);
        }
        for (f, v) in wait_these {
            assert_eq!(executor::block_on(f), v);
        }
        for (f, v) in wait_squash {
            assert_eq!(executor::block_on(f), v);
        }
        // wait all
        executor::block_on(wait_all(ctx));
    }

    #[test]
    fn test_single_wait_local_should_panic() {
        let _e = ExecutorHubSetUp::new_with_fake();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        std::panic::set_hook(Box::new(|_| {})); // silence backtrace
        let new_aid = ctx.spawn();
        let f = wait_single::<usize>(new_aid);
        send_2_local_panic(new_aid, vec![]);

        use std::panic::{self, AssertUnwindSafe};
        // catch manually to avoid poison the lock
        let ret = panic::catch_unwind(AssertUnwindSafe(move || {
            executor::block_on(f);
            executor::block_on(wait_all(ctx));
        }));
        let _ = std::panic::take_hook();
        assert!(ret.is_err());
    }

    fn activity_tree(
        ctx: &mut ConcreteContext,
        current_depth: usize,
        depth_max: usize,
        degree: usize,
        activities: &mut Vec<(ActivityId, Vec<ActivityId>)>,
    ) {
        for _ in 0..degree {
            let new_aid = ctx.spawn();
            if current_depth != depth_max {
                let mut new_ctx = ConcreteContext::inherit(new_aid.get_finish_id());
                activity_tree(
                    &mut new_ctx,
                    current_depth + 1,
                    depth_max,
                    degree,
                    activities,
                );
                activities.push((new_aid, new_ctx.spawned()));
            } else {
                activities.push((new_aid, vec![]));
            }
        }
    }

    #[test]
    fn test_many_local() {
        let _e = ExecutorHubSetUp::new_with_fake();
        // new context
        let mut ctx = ConcreteContext::new_frame();
        let mut activities: Vec<(ActivityId, Vec<ActivityId>)> = vec![];
        activity_tree(&mut ctx, 0, 5, 3, &mut activities);

        // should not return until
        let can_ret = ShouldNotReturnUntil::new(move || executor::block_on(wait_all(ctx)));
        let mut count = 0;
        let total_num = activities.len();
        for (aid, sub) in activities {
            if count == total_num - 1 {
                // set before the last
                can_ret.can_return_now();
            }
            send_1_local(aid, sub);
            count += 1;
        }
    }

    #[test]
    fn test_local_worker_task_queue() {
        let _e = ExecutorHubSetUp::new_with_fake();
        let mut worker_receiver = take_worker_task_receiver();
        let mut builder = TaskItemBuilder::new(1, global_id::here(), 3);
        builder.arg(1usize);
        builder.arg(0.5f64);
        builder.arg_squash(A { value: 123 });
        ConcreteContext::send(builder.build_box());
        let item = worker_receiver.blocking_recv().unwrap();
        let mut e = TaskItemExtracter::new(*item);
        assert_eq!(e.fn_id(), 1);
        assert_eq!(e.place(), global_id::here());
        assert_eq!(e.activity_id(), 3);
        assert_eq!(e.arg::<usize>(), 1);
        assert_eq!(e.arg::<f64>(), 0.5);
        assert_eq!(e.arg_squash::<A>(), A { value: 123 });
    }

    use crate::activity::test::_clone;
    use crate::activity::test::_eq;
    use crate::activity::FunctionLabel;
    use crate::activity::SquashBufferFactory;

    fn distributor_block_recv(d: &mut impl AbstractDistributor) -> Box<TaskItem> {
        loop {
            if let Some(t) = d.recv() {
                break t;
            }
        }
    }
    struct MockSender {
        sender: Sender<Vec<u8>>,
    }

    impl MessageSender for MockSender {
        fn send_msg(&self, _dst: Rank, message: Vec<u8>) {
            self.sender.send(message).unwrap()
        }
    }

    #[test]
    fn test_distributor() {
        let _e = ExecutorHubSetUp::new_with_fake();

        let (bytes_t, bytes_r) = mpsc::channel::<Vec<u8>>();
        let (buf_t, buf_r) = mpsc::channel::<Box<dyn AbstractSquashBuffer>>();

        let factory = Box::new(SquashBufferFactory::new());
        let factory1 = Box::new(SquashBufferFactory::new());
        let distrbutor = Distributor::new(factory, 10, MockSender { sender: bytes_t }, buf_r);

        let t = thread::spawn(move || {
            // fake network layer
            while let Ok(bytes) = bytes_r.recv() {
                let buffer = factory1.deserialize_from(&bytes[..]);
                buf_t.send(buffer).unwrap();
            }
        });

        // prepare task a
        let mut a_task = vec![];
        for i in 0..100usize {
            let mut b = TaskItemBuilder::new(i as FunctionLabel, 9, i as ActivityId);
            b.arg(i);
            b.arg_squash(A { value: i });
            a_task.push(b.build_box());
        }

        {
            let mut distrbutor = distrbutor; // move, to drop the channel
                                             // send 1 receive 1
            for task in a_task.iter() {
                distrbutor.send(Box::new(_clone::<A>(task)));
                thread::sleep(time::Duration::from_millis(1));
                distrbutor.poll();
                assert!(_eq::<A>(&*distributor_block_recv(&mut distrbutor), task));
            }

            // send all receive all
            for task in a_task.iter() {
                distrbutor.send(Box::new(_clone::<A>(task)));
            }
            thread::sleep(MAX_BUFFER_LIFETIME);
            distrbutor.poll(); // flush all

            let mut received = vec![]; // since squash, order changes
            for _ in 0..a_task.len() {
                let got = *distributor_block_recv(&mut distrbutor);
                received.push(got);
            }
            assert!(distrbutor.recv().is_none());
            received.sort_by_key(|item| item.activity_id());
            for i in 0..a_task.len() {
                assert!(_eq::<A>(&received[i], &*a_task[i]));
            }
        }

        t.join().unwrap();
    }

    #[test]
    fn test_execuctionhub_with_distributor() {
        let (bytes_t, bytes_r) = mpsc::channel::<Vec<u8>>();
        let (buf_t, buf_r) = mpsc::channel::<Box<dyn AbstractSquashBuffer>>();

        let factory = Box::new(SquashBufferFactory::new());
        let factory1 = Box::new(SquashBufferFactory::new());
        let distrbutor = Distributor::new(factory, 10, MockSender { sender: bytes_t }, buf_r);

        let dst_place: Place = 1;
        let fid_handle_usize = 1;
        let fid_handle_squash = 2;

        let t = thread::spawn(move || {
            // fake network layer
            while let Ok(bytes) = bytes_r.recv() {
                let mut buffer_got = factory1.deserialize_from(&bytes[..]);
                let mut buffer_send = factory1.new_buffer();
                buffer_got.extract_all();

                while let Some(item) = buffer_got.pop() {
                    let mut e = TaskItemExtracter::new(item);
                    let aid = e.activity_id();
                    let src_place = aid.get_spawned_place();
                    let finish_place = aid.get_finish_id().get_place();
                    assert_eq!(e.place(), dst_place); // dst should be correct

                    let mut build_for_wait_all = TaskItemBuilder::new(e.fn_id(), finish_place, aid);
                    let mut build_for_wait_single = TaskItemBuilder::new(e.fn_id(), src_place, aid);

                    build_for_wait_all.ret(thread::Result::<()>::Ok(()));
                    if e.fn_id() == fid_handle_usize {
                        let ret = e.arg::<usize>() - 1; // calculation
                        build_for_wait_single.ret(thread::Result::<usize>::Ok(ret));
                    } else if e.fn_id() == fid_handle_squash {
                        let mut ret_a = e.arg_squash::<A>();
                        ret_a.value -= 1;
                        build_for_wait_single.ret_squash(thread::Result::<A>::Ok(ret_a));
                    } else {
                        panic!()
                    }
                    build_for_wait_single.waited();
                    buffer_send.push(build_for_wait_all.build());
                    buffer_send.push(build_for_wait_single.build());
                }

                buffer_send.squash_all();
                buf_t.send(buffer_send).unwrap();
            }
        });

        {
            let _e = ExecutorHubSetUp::new(distrbutor); // executor should quit first
            let mut ctx = ConcreteContext::new_frame();
            let mut wait_these = vec![];
            let mut wait_squash = vec![];
            // prepare and send return
            for return_value in 0..1024usize {
                // register a sub activity
                let new_aid = ctx.spawn();
                wait_these.push((wait_single::<usize>(new_aid), return_value));
                // prepare request
                let mut builder = TaskItemBuilder::new(fid_handle_usize, dst_place, new_aid);
                builder.arg(return_value + 1); // remote should set the value -= 1
                builder.waited();
                ConcreteContext::send(builder.build_box());
            }

            for value in 0..1024 {
                let new_aid = ctx.spawn();
                let return_a = A { value };
                wait_squash.push((wait_single_squash::<A>(new_aid), return_a.clone()));
                // prepare request
                let mut builder = TaskItemBuilder::new(fid_handle_squash, dst_place, new_aid);
                builder.arg_squash(A { value: value + 1 });
                builder.waited();
                ConcreteContext::send(builder.build_box());
            }

            for (f, v) in wait_these {
                assert_eq!(executor::block_on(f), v);
            }
            for (f, v) in wait_squash {
                assert_eq!(executor::block_on(f), v);
            }
            // wait all
            executor::block_on(wait_all(ctx));
        }
        assert!(t.join().is_ok())
    }
}
