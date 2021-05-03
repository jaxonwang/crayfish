use crate::activity::SquashBufferFactory;
use crate::activity::TaskItem;
use crate::global_id;
use crate::logging;
use crate::logging::*;
use crate::network;
use crate::network::CollectiveOperator;
use crate::network::Rank;
use crate::runtime::init_task_item_channels;
use crate::runtime::init_worker_task_queue;
use crate::runtime::message_recv_callback;
use crate::runtime::take_message_buffer_receiver;
use crate::runtime::take_worker_task_receiver;
use crate::runtime::Distributor;
use crate::runtime::ExecutionHub;
use futures::Future;
use std::thread;

extern crate tokio;

pub fn genesis<F, MOUT, WD, WDF>(main: F, worker_dispatch: WD) -> MOUT
where
    F: Future<Output = MOUT> + Send + 'static,
    MOUT: Send + 'static,
    WDF: Future<Output = ()> + Send + 'static,
    WD: Send + 'static + Fn(TaskItem) -> WDF,
{
    // logger
    logging::setup_logger().unwrap();

    // prepare callback
    let msg_recv_callback =
        |src: Rank, data: &[u8]| message_recv_callback::<SquashBufferFactory>(src, data);

    // start network context
    let mut context = network::context::CommunicationContext::new(msg_recv_callback);
    let world_size = context.world_size();

    // prepare factories
    let factory = Box::new(SquashBufferFactory::new());

    // init static data for communications
    init_worker_task_queue();
    init_task_item_channels();
    global_id::init_here(context.here());
    global_id::init_world_size(world_size);

    // prepare distributor
    let sender = context.single_sender();
    let coll = context.collective_operator();
    let buffer_receiver = take_message_buffer_receiver();
    let distributor = Distributor::new(factory, world_size, sender, buffer_receiver);

    // prepare execution hub
    let mut hub = ExecutionHub::new(distributor);
    let trigger = hub.get_trigger();

    // start network loop
    info!("start network loop");
    // coll must not perform barrier before init
    let (init_done_s, init_done_r) = std::sync::mpsc::channel::<()>();
    // this thread is the only thread perfroming send/recv
    let network_thread = thread::spawn(move || {
        // global barrier to init network, must init and run in the same thread,
        // otherwise the callback would be invoked at different thread, result in
        // fetch channel twice
        context.init();
        init_done_s.send(()).unwrap();
        context.run();
    }); // run in a independent thread

    // start hub loop
    info!("start execution hub");
    let hub_thread = thread::spawn(move || hub.run());

    // start workers
    let rt = tokio::runtime::Runtime::new().unwrap();

    // worker loop
    let worker_loop = async move {
        let mut task_receiver = take_worker_task_receiver();
        while let Some(task) = task_receiver.recv().await {
            tokio::spawn(worker_dispatch(*task));
        }
        debug!("worker task loop stops");
    };

    // main
    let ret = rt.block_on(async move {
        tokio::spawn(worker_loop);
        tokio::spawn(main).await.unwrap()
    });

    // should not perform any network before init done
    init_done_r.recv().unwrap();
    // wait till all finishes;
    coll.barrier();
    trigger.stop();
    drop(coll); // drop coll to stop network context

    hub_thread.join().unwrap();
    network_thread.join().unwrap();
    info!("exit gracefully");

    ret
}
