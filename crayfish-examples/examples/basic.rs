use crayfish::activity::ActivityId;
use crayfish::activity::FunctionLabel;
use crayfish::activity::HelperByType;
use crayfish::activity::TaskItem;
use crayfish::activity::TaskItemBuilder;
use crayfish::activity::TaskItemExtracter;
use crayfish::args::RemoteSend;
use crayfish::essence;
use crayfish::global_id;
use crayfish::global_id::ActivityIdMethods;
use crayfish::logging::*;
use crayfish::place::Place;
use crayfish::runtime::wait_all;
use crayfish::runtime::wait_single;
use crayfish::runtime::ApgasContext;
use crayfish::runtime::ConcreteContext;
use crayfish::runtime_meta::FunctionMetaData;
use crayfish::runtime_meta::SquashHelperMeta;
use crayfish::inventory;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde::Serialize;
use std::any::TypeId;
use std::cmp::Ordering;
use std::convert::TryInto;
use std::panic::AssertUnwindSafe;

extern crate crayfish;
extern crate futures;
extern crate serde;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct A {
    pub value: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AOut {
    last: usize,
    diffs: Vec<usize>,
}

impl RemoteSend for A {
    type Output = AOut;
    fn fold(&self, acc: &mut Self::Output) {
        assert!(acc.last <= self.value);
        acc.diffs.push((self.value - acc.last).try_into().unwrap());
        acc.last = self.value;
    }
    fn extract(out: &mut Self::Output) -> Option<Self> {
        out.diffs.pop().map(|x| {
            let ret = out.last;
            out.last = out.last - x as usize;
            A { value: ret }
        })
    }
    fn reorder(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

crayfish::inventory::submit! {
    SquashHelperMeta::new(TypeId::of::<A>(), Box::new(HelperByType::<A>::default()))
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct B {
    pub value: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct BOut {
    list: Vec<u8>,
}
impl RemoteSend for B {
    type Output = BOut;
    fn fold(&self, acc: &mut Self::Output) {
        acc.list.push(self.value);
    }
    fn extract(out: &mut Self::Output) -> Option<Self> {
        out.list.pop().map(|value| B { value })
    }
    fn reorder(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

crayfish::inventory::submit! {
    SquashHelperMeta::new(TypeId::of::<B>(), Box::new(HelperByType::<B>::default()))
}

#[derive(Debug, Serialize, Deserialize)]
struct R {
    a: A,
    b: B,
    c: i32,
}

impl RemoteSend for R {
    type Output = ();
    fn fold(&self, _acc: &mut Self::Output) {
        panic!()
    }
    fn extract(_out: &mut Self::Output) -> Option<Self> {
        panic!()
    }
    fn reorder(&self, _other: &Self) -> Ordering {
        panic!()
    }
    fn is_squashable() -> bool {
        false
    }
}

async fn real_fn(ctx: &mut impl ApgasContext, a: A, b: B, c: i32) -> R {
    // macro
    debug!("execute func with args: {:?}, {:?}, {}", a, b, c);
    if c < 200 {
        let here = global_id::here();
        let world_size = global_id::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        async_create_no_wait_for_fn_id_0(ctx.spawn(), dst_place, a.clone(), b.clone(), c + 1);
    }
    R { a, b, c: c + 1 }
}

// block until real function finished
async fn execute_and_send_fn0(my_activity_id: ActivityId, waited: bool, a: A, b: B, c: i32) {
    let fn_id = 0; // macro
    let finish_id = my_activity_id.get_finish_id();
    let mut ctx = ConcreteContext::inherit(finish_id);
    // ctx seems to be unwind safe
    let future = AssertUnwindSafe(real_fn(&mut ctx, a, b, c)); //macro
    let result = future.catch_unwind().await;
    essence::send_activity_result(ctx, my_activity_id, fn_id, waited, result);
}

// the one executed by worker
fn real_fn_wrap_execute_from_remote(item: TaskItem) -> BoxFuture<'static, ()> {
    async move {
        let waited = item.is_waited();
        let mut e = TaskItemExtracter::new(item);
        let my_activity_id = e.activity_id();

        // wait until function return
        trace!(
            "Got activity:{} from {}",
            my_activity_id,
            my_activity_id.get_spawned_place()
        );
        execute_and_send_fn0(my_activity_id, waited, e.arg(), e.arg(), e.arg()).await;
        // macro
    }
    .boxed()
}

crayfish::inventory::submit! {
    FunctionMetaData::new(0, real_fn_wrap_execute_from_remote,
                          String::from("basic"),
                          String::from(file!()),
                          line!(),
                          String::from(module_path!())
                          )
}

// the desugered at async and wait
fn async_create_for_fn_id_0(
    // TODO: dont' use &mut ctx, for boxed lifetime
    my_activity_id: ActivityId,
    dst_place: Place,
    a: A,
    b: B,
    c: i32,
) -> impl futures::Future<Output = R> {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single::<R>(my_activity_id); // macro
    if dst_place == global_id::here() {
        crayfish::spawn(execute_and_send_fn0(my_activity_id, true, a, b, c)); // macro
    } else {
        trace!("spawn activity:{} at place: {}", my_activity_id, dst_place);
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(a); //  macro
        builder.arg(b); // macro
        builder.arg(c); //macro
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
    a: A,
    b: B,
    c: i32,
) {
    // macro
    let fn_id: FunctionLabel = 0; // macro

    if dst_place == global_id::here() {
        // no wait, set flag = flase
        crayfish::spawn(execute_and_send_fn0(my_activity_id, false, a, b, c)); // macro
    } else {
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg(a); //  macro
        builder.arg(b); // macro
        builder.arg(c); //macro
        let item = builder.build_box();
        ConcreteContext::send(item);
    }
}

// desugered finish
async fn finish() {
    if global_id::here() == 0 {
        let mut ctx = ConcreteContext::new_frame();
        // ctx contains a new finish id now
        //
        let here = global_id::here();
        let world_size = global_id::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        // let f = async_create_for_fn_id_0(&mut ctx, dst_place, A { value: 1 }, B { value: 2 }, 3);
        //
        // debug!("waiting return of the function");
        // let ret = f.await; // if await, remove it from activity list this finish block will wait
        // debug!("got return value {:?}", ret);
        async_create_no_wait_for_fn_id_0(ctx.spawn(), dst_place, A { value: 2 }, B { value: 3 }, 1);

        wait_all(ctx).await;
        info!("Main finished")
    }
}

pub fn main() {
    essence::genesis(finish());
}
