use futures::FutureExt;
use rust_apgas::activity::copy_panic_payload;
use rust_apgas::activity::deserialize_entry_to_do_avoid_conflict;
use rust_apgas::activity::serialize_packed_to_do_avoid_conflict;
use rust_apgas::activity::ActivityId;
use rust_apgas::activity::FunctionLabel;
use rust_apgas::activity::ProperDispatcher;
use rust_apgas::activity::SquashOperation;
use rust_apgas::activity::Squashable;
use rust_apgas::activity::TaskItem;
use rust_apgas::activity::TaskItemBuilder;
use rust_apgas::activity::TaskItemExtracter;
use rust_apgas::global_id;
use rust_apgas::global_id::ActivityIdMethods;
use rust_apgas::global_id::FinishIdMethods;
use rust_apgas::logging::*;
use rust_apgas::place::Place;
use rust_apgas::runtime::wait_all;
use rust_apgas::runtime::wait_single;
use rust_apgas::runtime::ApgasContext;
use rust_apgas::runtime::ConcreteContext;
use serde::Deserialize;
use serde::Serialize;
use std::convert::TryInto;
use std::panic::AssertUnwindSafe;
use std::thread;

use rust_apgas::essence::genesis;

extern crate futures;
extern crate rust_apgas;
extern crate serde;
extern crate tokio;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct A {
    pub value: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AOut {
    last: usize,
    diffs: Vec<usize>,
}

impl Squashable for A {
    type Squashed = AOut;
    fn fold(&self, acc: &mut Self::Squashed) {
        assert!(acc.last <= self.value);
        acc.diffs.push((self.value - acc.last).try_into().unwrap());
        acc.last = self.value;
    }
    fn extract(out: &mut Self::Squashed) -> Option<Self> {
        out.diffs.pop().map(|x| {
            let ret = out.last;
            out.last = out.last - x as usize;
            A { value: ret }
        })
    }
    fn arrange<L>(l: &mut [(Box<Self>, L)]) {
        l.sort_by(|(a0, _), (a1, _)| a0.cmp(a1));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
pub struct B {
    pub value: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct BOut {
    list: Vec<u8>,
}
impl Squashable for B {
    type Squashed = BOut;
    fn fold(&self, acc: &mut Self::Squashed) {
        acc.list.push(self.value);
    }
    fn extract(out: &mut Self::Squashed) -> Option<Self> {
        out.list.pop().map(|value| B { value })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct R {
    a: A,
    b: B,
    c: i32,
}

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
        if typeid == TypeId::of::<A>() {
            Op::call::<A>(t)
        } else if typeid == TypeId::of::<B>() {
            Op::call::<B>(t)
        } else {
            panic!()
        }
    }
    fn dispatch_serialize_entry<S>(
        typeid: TypeId,
        v: &SquashedMapValue,
        ser: &mut S,
    ) -> Result<(), S::Error>
    where
        S: SerializeMap,
    {
        if typeid == TypeId::of::<A>() {
            serialize_packed_to_do_avoid_conflict::<A, S>(v, ser)
        } else if typeid == TypeId::of::<B>() {
            serialize_packed_to_do_avoid_conflict::<B, S>(v, ser)
        } else {
            panic!()
        }
    }
    fn dispatch_deserialize_entry<'de, ACC>(
        typeid: TypeId,
        access: ACC,
    ) -> Result<SquashedMapValue, ACC::Error>
    where
        ACC: MapAccess<'de>,
    {
        if typeid == TypeId::of::<A>() {
            deserialize_entry_to_do_avoid_conflict::<A, ACC>(access)
        } else if typeid == TypeId::of::<B>() {
            deserialize_entry_to_do_avoid_conflict::<B, ACC>(access)
        } else {
            panic!()
        }
    }
}

async fn real_fn(ctx: &mut impl ApgasContext, a: A, b: B, c: i32) -> R {
    // macro
    debug!("execute func with args: {:?}, {:?}, {}", a, b, c);
    if c < 200 {
        let here = global_id::here();
        let world_size = global_id::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        async_create_no_wait_for_fn_id_0(ctx, dst_place, a.clone(), b.clone(), c + 1);
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
    execute_and_send_fn0(
        my_activity_id,
        waited,
        e.arg_squash(),
        e.arg_squash(),
        e.arg(),
    )
    .await; // macro
}

// the desugered at async and wait
fn async_create_for_fn_id_0(
    // TODO: dont' use &mut ctx, for boxed lifetime
    ctx: &mut impl ApgasContext,
    dst_place: Place,
    a: A,
    b: B,
    c: i32,
) -> impl futures::Future<Output = R> {
    // macro
    let my_activity_id = ctx.spawn(); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    let f = wait_single::<R>(my_activity_id); // macro
    if dst_place == global_id::here() {
        tokio::spawn(execute_and_send_fn0(my_activity_id, true, a, b, c)); // macro
    } else {
        trace!("spawn activity:{} at place: {}", my_activity_id, dst_place);
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg_squash(a); //  macro
        builder.arg_squash(b); // macro
        builder.arg(c); //macro
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
    a: A,
    b: B,
    c: i32,
) {
    // macro
    let my_activity_id = ctx.spawn(); // register to remote
    let fn_id: FunctionLabel = 0; // macro

    if dst_place == global_id::here() {
        // no wait, set flag = flase
        tokio::spawn(execute_and_send_fn0(my_activity_id, false, a, b, c)); // macro
    } else {
        let mut builder = TaskItemBuilder::new(fn_id, dst_place, my_activity_id);
        builder.arg_squash(a); //  macro
        builder.arg_squash(b); // macro
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
        async_create_no_wait_for_fn_id_0(&mut ctx, dst_place, A { value: 2 }, B { value: 3 }, 1);

        wait_all(ctx).await;
        info!("Main finished")
    }
}

pub fn main() {
    genesis(
        finish(),
        real_fn_wrap_execute_from_remote,
        ConcreteDispatch {},
    );
}
