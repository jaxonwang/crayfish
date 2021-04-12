use futures::task::FutureObj;
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::any::Any;
use std::any::TypeId;
use std::cmp::Ordering;
use std::cmp::PartialOrd;
use std::convert::TryInto;

extern crate serde;

trait RemoteSend: Serialize + DeserializeOwned + Send + 'static {}
impl<T> RemoteSend for T where T: Serialize + DeserializeOwned + Send + 'static {}

#[rustfmt::skip]
mod function{
    use super::*;
    // pub trait AsyncFunction{
    //     type Args; // tuple
    //     fn call(arg:Args);
    // }
    pub struct PackedRet{
    }
    // type ConcrateAsyncFunction0<R,> where R: RemoteSend, = fn() -> R;
    // type ConcrateAsyncFunction1<R,T0> where R: RemoteSend,T0:RemoteSend = fn(T0) -> R;
}

type PackedValue = Box<dyn Any + Send + 'static>;
type PanicPayload = String;
pub struct PackedCall {
    args: Vec<PackedValue>,
    fn_id: FunctionLabel,
}
type FunctionLabel = u32; // function label is line
type ActivityReturn = std::result::Result<PackedValue, PanicPayload>;

pub trait Squashable : 'static{
    // squash type not necessarily to be serde
    type Squashed: RemoteSend + Default;
    fn fold(&self, acc: &mut Self::Squashed);
    fn extract(out: &mut Self::Squashed) -> Option<Self>
    where
        Self: Sized;
    fn arrange<L>(_list: &mut [(Box<Self>, L)]) { // default: no_op
    }
}

// TODO: Auto impl Squash macro by derive, squash
//
type OrderLabel = u32; // lower 8 bit is for the position in side a function

// TODO: should impl serialize
struct CallBuffer {
    args_list: Vec<Vec<u8>>, // has been serialized
    fn_ids: Vec<FunctionLabel>,
    next_label: OrderLabel,
    squashable_map: FxHashMap<TypeId, Vec<(PackedValue, OrderLabel)>>,
    squashed_map: FxHashMap<TypeId, (PackedValue, Vec<OrderLabel>)>,
    ordered_squashable: Vec<Vec<PackedValue>>,
}

impl CallBuffer {
    pub fn next_label(&mut self) -> OrderLabel {
        self.next_label = ((self.next_label >> 8) + 1) << 8;
        self.next_label
    }
}

trait DispatchByTypeID {
    fn call<T>(&mut self);
}

fn dispatch<T: DispatchByTypeID>(typeid: TypeId, callee: &mut T) {
    // this is generated
    if typeid == TypeId::of::<A>() {
        callee.call::<A>()
    } else if typeid == TypeId::of::<B>() {
        callee.call::<B>()
    }
}

struct SquashDispath<'a> {
    buf: &'a mut CallBuffer,
    typeid: TypeId,
}

impl<'a> DispatchByTypeID for SquashDispath<'a> {
    fn call<T>(&mut self) {
        panic!()
    }
    fn call<T:Squashable>(&mut self) {
        do_squash_one_in_buf::<T>(self.buf, self.typeid);
    }
}

fn do_squash_all(buf: &mut CallBuffer) {
    let typeids: Vec<_> = buf.squashable_map.keys().cloned().collect();
    for typeid in typeids {
        let mut d = SquashDispath { buf, typeid };
        dispatch(typeid, &mut d);
    }
}

fn do_squash_one_in_buf<T: Squashable>(buf: &mut CallBuffer, typeid: TypeId) {
    let to_squash = buf.squashable_map.remove(&typeid).unwrap();
    let to_squash: Vec<_> = to_squash
        .into_iter()
        .map(|(s, a)| (s.downcast::<T>().unwrap(), a))
        .collect();
    let (a ,b) = do_squash::<T>(to_squash);
    let a = a as PackedValue;
    buf.squashed_map.insert(
        typeid,
        (a, b)
        // do_squash::<T>(to_squash) as (PackedValue, Vec<OrderLabel>),
    );
}

fn do_squash<T: Squashable>(
    to_squash: Vec<(Box<T>, OrderLabel)>,
) -> (Box<T::Squashed>, Vec<OrderLabel>) {
    T::arrange::<OrderLabel>(&mut to_squash[..]);
    let mut squashed = Box::new(T::Squashed::default());
    let mut labels = Vec::<_>::with_capacity(to_squash.len());
    for (s, order) in to_squash {
        s.fold(&mut *squashed);
        labels.push(order);
    }
    return (squashed, labels);
}

fn do_extract_ordered<T: Squashable>(
    to_inflate: (Box<T::Squashed>, Vec<OrderLabel>),
) -> Vec<Box<T>> {
    let (mut squashed, labels) = to_inflate;
    let mut inflated = Vec::<_>::with_capacity(labels.len());
    let mut count = 0;
    let borrow: &mut T::Squashed = &mut *squashed;
    while let Some(s) = T::extract(borrow) {
        inflated.push((Box::new(s), labels[count]));
        count += 1;
    }
    debug_assert_eq!(count, labels.len() - 1);
    inflated.sort_unstable_by_key(|x| (*x).1);
    inflated.into_iter().map(|(s, _)| s).collect()
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Ord)]
struct A {
    value: usize,
}
impl PartialOrd for A {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.value.cmp(&other.value))
    }
}

#[derive(Serialize, Deserialize, Default)]
struct AOut {
    last: usize,
    diffs: Vec<u8>,
}

impl Squashable for A {
    type Squashed = AOut;
    fn fold(&self, acc: &mut Self::Squashed) {
        assert!(acc.last < self.value);
        acc.diffs.push((self.value - acc.last).try_into().unwrap());
        acc.last = self.value;
    }
    fn extract(out: &mut Self::Squashed) -> Option<Self> {
        out.diffs.pop().map(|x| {
            let ret = out.last - x as usize;
            out.last = ret;
            A { value: ret }
        })
    }
    fn arrange<L>(l: &mut [(Box<Self>, L)]) {
        l.sort_by_key(|x| *x.0);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct B {
    value: u8,
}
#[derive(Serialize, Deserialize, Default)]
struct BOut {
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

#[derive(Serialize, Deserialize)]
struct R {
    a: A,
    b: B,
    c: i32,
}

fn user_func_A(a: A, b: B, c: i32) -> R {
    println!("A {:?} B {:?} C {}", a, b, c);
    R { a, b, c }
}
fn user_func_B(a: A) -> A {
    println!("---- {:?}", a);
    a
}

pub fn gen_func_A_call_push(a: A, b: B, c: i32) {

    //      get lock
    //
    // buffer.a.push()
}
