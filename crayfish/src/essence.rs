use crate::activity::copy_panic_payload;
use crate::activity::ActivityId;
use crate::activity::FunctionLabel;
use crate::activity::SquashBufferFactory;
use crate::activity::TaskItem;
use crate::activity::TaskItemBuilder;
use crate::args::RemoteSend;
use crate::collective;
use crate::executor;
use crate::global_id;
use crate::global_id::ActivityIdMethods;
use crate::global_id::FinishIdMethods;
use crate::logging;
use crate::logging::*;
use crate::meta_data;
use crate::network::context::CommunicationContext;
use crate::network::MessageHandler;
use crate::network::Rank;
use crate::runtime::init_task_item_channels;
use crate::runtime::init_worker_task_queue;
use crate::runtime::message_recv_callback;
use crate::runtime::take_message_buffer_receiver;
use crate::runtime::take_worker_task_receiver;
use crate::runtime::ApgasContext;
use crate::runtime::ConcreteContext;
use crate::runtime::Distributor;
use crate::runtime::ExecutionHub;
use crate::runtime_meta;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use futures::Future;
use std::thread;

pub fn send_activity_result<T: RemoteSend>(
    ctx: impl ApgasContext,
    a_id: ActivityId,
    fn_id: FunctionLabel,
    waited: bool,
    result: std::thread::Result<T>,
) {
    let finish_id = a_id.get_finish_id();
    let stripped_result = match &result {
        // copy payload
        Ok(_) => thread::Result::<()>::Ok(()),
        Err(e) => thread::Result::<()>::Err(copy_panic_payload(e)),
    };

    // TODO panic all or panic single?
    // should set dst place of return to it's finishid, to construct calling tree
    let mut builder = TaskItemBuilder::new(fn_id, finish_id.get_place(), a_id);
    let spawned_activities = ctx.spawned(); // get activity spawned in real_fn
    builder.ret(stripped_result); // strip return value
    builder.sub_activities(spawned_activities.clone());
    let item = builder.build_box();
    ConcreteContext::send(item);
    // send to the place waited (spawned)
    if waited {
        // two ret must be identical if dst is the same place
        let mut builder = TaskItemBuilder::new(fn_id, a_id.get_spawned_place(), a_id);
        builder.ret(result); // macro
        builder.sub_activities(spawned_activities);
        builder.waited();
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
}

fn worker_dispatch(item: TaskItem) -> BoxFuture<'static, ()> {
    let fn_id = item.function_id();
    let resovled = runtime_meta::get_func_table().get(&fn_id).unwrap().fn_ptr;
    resovled(item)
}

// make this public for integration test
pub fn init_collective_operator<T: MessageHandler>(ctx: &CommunicationContext<T>) {
    collective::set_coll(Box::new(ctx.collective_operator()));
}

pub fn genesis<F, FOUT, MOUT>(main: F) -> MOUT
where
    F: FnOnce(Vec<String>) -> FOUT,
    FOUT: Future<Output = MOUT> + Send + 'static,
    MOUT: Send + 'static,
{
    // logger
    logging::setup_logger().unwrap();

    // print setting
    meta_data::show_data();

    // the function table for task dispatch by fn_id
    runtime_meta::init_func_table();

    // register dynamic operations for squahable
    runtime_meta::init_helpers();

    // prepare callback
    let msg_recv_callback =
        |src: Rank, data: &[u8]| message_recv_callback::<SquashBufferFactory>(src, data);

    // start network context
    let mut context = CommunicationContext::new(msg_recv_callback);
    let world_size = context.world_size();
    let main_fut = main(context.cmd_args().to_vec());

    // prepare factories
    let factory = Box::new(SquashBufferFactory::new());

    // init static data for communications
    init_worker_task_queue();
    init_task_item_channels();
    global_id::init_here(context.here().as_place());
    global_id::init_world_size(world_size);

    // init collective operator, which will be used by main
    init_collective_operator(&context);

    // prepare distributor
    let sender = context.single_sender();
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
    let rt = executor::runtime::Builder::new_multi_thread()
        .worker_threads(*meta_data::NUM_CPUS)
        .thread_name("crayfish-worker")
        .build()
        .unwrap();

    // worker loop
    let worker_loop = async move {
        let mut task_receiver = take_worker_task_receiver();
        while let Some(task) = task_receiver.recv().await {
            executor::spawn(worker_dispatch(*task));
        }
        debug!("worker task loop stops");
    };

    // main
    let ret = rt.block_on(async move {
        executor::spawn(worker_loop);
        executor::spawn(main_fut).await.unwrap()
    });

    // should not perform any network before init done
    init_done_r.recv().unwrap();
    // wait till all finishes;
    let mut coll = collective::take_coll();
    rt.block_on(coll.barrier().map(|r| r.unwrap()));
    trigger.stop();
    drop(coll); // drop coll to stop network context

    hub_thread.join().unwrap();
    network_thread.join().unwrap();
    drop(rt);
    crate::trace::print_profiling();
    info!("exit gracefully");

    ret
}
