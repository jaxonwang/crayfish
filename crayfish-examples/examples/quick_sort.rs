use crayfish::activity::ActivityId;
use crayfish::activity::FunctionLabel;
use crayfish::activity::TaskItem;
use crayfish::activity::TaskItemBuilder;
use crayfish::activity::TaskItemExtracter;
use crayfish::essence;
use crayfish::global_id;
use crayfish::global_id::here;
use crayfish::global_id::world_size;
use crayfish::global_id::ActivityIdMethods;
use crayfish::logging::*;
use crayfish::place::Place;
use crayfish::runtime::wait_all;
use crayfish::runtime::wait_single;
use crayfish::runtime::ApgasContext;
use crayfish::runtime::ConcreteContext;
use crayfish::runtime_meta::FunctionMetaData;
use crayfish::inventory;
use futures::future::BoxFuture;
use futures::FutureExt;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::panic::AssertUnwindSafe;

extern crate crayfish;
extern crate futures;
extern crate rand;

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
    let fn_id: FunctionLabel = 0; // macro
    let finish_id = my_activity_id.get_finish_id();
    let mut ctx = ConcreteContext::inherit(finish_id);
    // ctx seems to be unwind safe
    let future = AssertUnwindSafe(quick_sort(&mut ctx, a)); //macro
    let result = future.catch_unwind().await;
    essence::send_activity_result(ctx, my_activity_id, fn_id, waited, result);
}

// the one executed by worker
fn real_fn_wrap_execute_from_remote(item: TaskItem) -> BoxFuture<'static, ()> {
    async move {
        let waited = item.is_waited();
        let mut e = TaskItemExtracter::new(item);
        let activity_id = e.activity_id();

        // wait until function return
        trace!(
            "Got activity:{} from {}",
            activity_id,
            activity_id.get_spawned_place()
        );
        execute_and_send_fn0(activity_id, waited, e.arg()).await; // macro
    }
    .boxed()
}

crayfish::inventory::submit! {
    FunctionMetaData::new(0, real_fn_wrap_execute_from_remote,
                          String::from("quick_sort"),
                          String::from(file!()),
                          line!(),
                          String::from(module_path!())
                          )
}

// the desugered at async and wait
fn async_create_for_fn_id_0(
    my_activity_id: ActivityId,
    dst_place: Place,
    nums: Vec<usize>,
) -> impl futures::Future<Output = Vec<usize>> {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single(my_activity_id); // macro
    if dst_place == global_id::here() {
        crayfish::spawn(execute_and_send_fn0(my_activity_id, true, nums)); // macro
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
    my_activity_id: ActivityId,
    dst_place: Place,
    nums: Vec<usize>,
) {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    if dst_place == global_id::here() {
        // no wait, set flag = flase
        crayfish::spawn(execute_and_send_fn0(my_activity_id, false, nums)); // macro
    } else {
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(nums); //macro
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
}

// desugered finish
async fn finish() -> Result<(), std::io::Error> {
    if global_id::here() == 0 {
        let mut ctx = ConcreteContext::new_frame();
        // ctx contains a new finish id now
        let mut rng = rand::rngs::StdRng::from_entropy();
        let mut nums: Vec<usize> = (0..1000).collect();
        nums.shuffle(&mut rng);
        info!("before sorting: {:?}", nums);
        let sorted = quick_sort(&mut ctx, nums).await;
        info!("sorted: {:?}", sorted);

        wait_all(ctx).await;
        info!("Main finished");
    }
    Ok(())
}

pub fn main() -> Result<(), std::io::Error> {
    essence::genesis(finish())
}
