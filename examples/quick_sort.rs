use futures::future::BoxFuture;
use futures::FutureExt;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rust_apgas::activity::copy_panic_payload;
use rust_apgas::activity::ActivityId;
use rust_apgas::activity::FunctionLabel;
use rust_apgas::activity::ProperDispatcher;
use rust_apgas::activity::SquashBufferFactory;
use rust_apgas::activity::SquashOperation;
use rust_apgas::activity::TaskItem;
use rust_apgas::activity::TaskItemBuilder;
use rust_apgas::activity::TaskItemExtracter;
use rust_apgas::global_id;
use rust_apgas::global_id::here;
use rust_apgas::global_id::world_size;
use rust_apgas::global_id::ActivityIdMethods;
use rust_apgas::global_id::FinishIdMethods;
use rust_apgas::logging;
use rust_apgas::logging::*;
use rust_apgas::network;
use rust_apgas::network::CollectiveOperator;
use rust_apgas::network::Rank;
use rust_apgas::place::Place;
use rust_apgas::runtime::init_task_item_channels;
use rust_apgas::runtime::init_worker_task_queue;
use rust_apgas::runtime::message_recv_callback;
use rust_apgas::runtime::take_message_buffer_receiver;
use rust_apgas::runtime::take_worker_task_receiver;
use rust_apgas::runtime::wait_all;
use rust_apgas::runtime::wait_single;
use rust_apgas::runtime::ApgasContext;
use rust_apgas::runtime::ConcreteContext;
use rust_apgas::runtime::Distributor;
use rust_apgas::runtime::ExecutionHub;
use serde::Deserialize;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use std::thread;

extern crate futures;
extern crate rand;
extern crate rust_apgas;
extern crate serde;
extern crate tokio;

use rust_apgas::activity::SquashDispatch;
use rust_apgas::activity::SquashedMapValue;
use serde::de::MapAccess;
use serde::ser::SerializeMap;
use std::any::TypeId;
#[derive(Serialize, Deserialize)]
pub struct ConcreteDispatch {}
impl ProperDispatcher for ConcreteDispatch {}
impl<Op> SquashDispatch<Op> for ConcreteDispatch
where
    Op: SquashOperation,
{
    fn dispatch_for_squashable(typeid: TypeId, t: Op::DataType) -> Op::ReturnType {
        panic!()
    }
    fn dispatch_serialize_entry<S>(
        typeid: TypeId,
        v: &SquashedMapValue,
        ser: &mut S,
    ) -> Result<(), S::Error>
    where
        S: SerializeMap,
    {
        panic!()
    }
    fn dispatch_deserialize_entry<'de, ACC>(
        typeid: TypeId,
        access: ACC,
    ) -> Result<SquashedMapValue, ACC::Error>
    where
        ACC: MapAccess<'de>,
    {
        panic!()
    }
}

fn quick_sort<'a>(
    ctx: &'a mut impl ApgasContext,
    mut nums: Vec<usize>,
) -> BoxFuture<'a, Vec<usize>> {
    async move {
        info!("sorting vector of len: {}", nums.len());
        if nums.len() < 10 {
            nums.sort();
            return nums;
        }
        let pivot = nums[0];
        let rest = &nums[1..];
        let left: Vec<_> = rest.iter().filter(|n| **n < pivot).cloned().collect();
        let right: Vec<_> = rest.iter().filter(|n| **n >= pivot).cloned().collect();

        let neighbor = (here() as usize + 1) % world_size();

        // wait for result of sub activities
        let left_sorted_future = async_create_for_fn_id_0(ctx.spawn(), neighbor as Place, left);
        let right_sorted_future = async_create_for_fn_id_0(ctx.spawn(), here(), right);
        // overlap the local & remote computing
        let right_sorted = right_sorted_future.await;
        let mut left_sorted = left_sorted_future.await;
        left_sorted.push(pivot);
        left_sorted.extend_from_slice(&right_sorted[..]);
        left_sorted
    }
    .boxed()
}

// block until real function finished
async fn execute_and_send_fn0(my_activity_id: ActivityId, waited: bool, a: Vec<usize>) {
    let fn_id = 0; // macro
    let finish_id = my_activity_id.get_finish_id();
    let mut ctx = ConcreteContext::inherit(finish_id);
    // ctx seems to be unwind safe
    let future = AssertUnwindSafe(quick_sort(&mut ctx, a)); //macro
    let result = future.catch_unwind().await;
    let stripped_result = match &result {
        // copy payload
        Ok(_) => thread::Result::<()>::Ok(()),
        Err(e) => thread::Result::<()>::Err(copy_panic_payload(e)),
    };

    // TODO panic all or panic single?
    // should set dst place of return to it's finishid, to construct calling tree
    let mut builder = TaskItemBuilder::new(fn_id, finish_id.get_place(), my_activity_id);
    let spawned_activities = ctx.spawned(); // get activity spawned in real_fn
    builder.ret(stripped_result); // strip return value
    builder.sub_activities(spawned_activities.clone());
    let item = builder.build_box();
    ConcreteContext::send(item);
    // send to the place waited (spawned)
    if waited {
        // two ret must be identical if dst is the same place
        let mut builder =
            TaskItemBuilder::new(fn_id, my_activity_id.get_spawned_place(), my_activity_id);
        builder.ret(result); // macro
        builder.sub_activities(spawned_activities);
        builder.waited();
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
}

// the one executed by worker
async fn real_fn_wrap_execute_from_remote(item: TaskItem) {
    let waited = item.is_waited();
    let mut e = TaskItemExtracter::new(item);
    let my_activity_id = e.activity_id();
    let _fn_id = e.fn_id(); // dispatch

    // wait until function return
    trace!(
        "Got activity:{} from {}",
        my_activity_id,
        my_activity_id.get_spawned_place()
    );
    execute_and_send_fn0(my_activity_id, waited, e.arg()).await; // macro
}

// the desugered at async and wait
fn async_create_for_fn_id_0(
    my_activity_id: ActivityId,
    dst_place: Place,
    nums: Vec<usize>,
) -> impl futures::Future<Output = Vec<usize>> {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single::<Vec<usize>>(my_activity_id); // macro
    if dst_place == global_id::here() {
        tokio::spawn(execute_and_send_fn0(my_activity_id, true, nums)); // macro
    } else {
        trace!("spawn activity:{} at place: {}", my_activity_id, dst_place);
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(nums); //macro
        builder.waited();
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
    f
}

// the desugered at async no wait
fn async_create_no_wait_for_fn_id_0(
    ctx: &mut impl ApgasContext,
    dst_place: Place,
    nums: Vec<usize>,
) {
    // macro
    let my_activity_id = ctx.spawn(); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    if dst_place == global_id::here() {
        // no wait, set flag = flase
        tokio::spawn(execute_and_send_fn0(my_activity_id, false, nums)); // macro
    } else {
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(nums); //macro
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
}

// desugered finish
async fn finish() {
    let mut ctx = ConcreteContext::new_frame();
    // ctx contains a new finish id now
    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut nums: Vec<usize> = (0..1000).collect();
    nums.shuffle(&mut rng);
    info!("before sorting: {:?}", nums);
    let sorted = quick_sort(&mut ctx, nums).await;
    info!("sorted: {:?}", sorted);

    wait_all(ctx).await;
    info!("Main finished")
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    // logger
    logging::setup_logger().unwrap();

    // prepare callback
    let msg_recv_callback = |src: Rank, data: &[u8]| {
        message_recv_callback::<SquashBufferFactory<ConcreteDispatch>>(src, data)
    };

    // start network context
    let mut context = network::context::CommunicationContext::new(msg_recv_callback);
    let world_size = context.world_size();

    // prepare factories
    let factory = Box::new(SquashBufferFactory::<ConcreteDispatch>::new());

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
    let rt = tokio::runtime::Runtime::new()?;

    // worker loop
    let worker_loop = async {
        let mut task_receiver = take_worker_task_receiver();
        while let Some(task) = task_receiver.recv().await {
            tokio::spawn(real_fn_wrap_execute_from_remote(*task));
        }
        debug!("worker task loop stops");
    };

    if global_id::here() == 0 {
        // main
        rt.block_on(async move {
            tokio::spawn(worker_loop);
            tokio::spawn(finish()).await.unwrap(); // main will return only when everything
        });
    } else {
        rt.block_on(async move {
            tokio::spawn(worker_loop);
        });
    }

    // should not perform any network before init done
    init_done_r.recv().unwrap();
    // wait till all finishes;
    coll.barrier();
    trigger.stop();
    drop(coll); // drop coll to stop network context

    hub_thread.join().unwrap();
    network_thread.join().unwrap();
    info!("exit gracefully");

    Ok(())
}
