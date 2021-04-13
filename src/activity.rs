use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
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

pub trait RemoteSend: Serialize + DeserializeOwned + Send + 'static {}
impl<T> RemoteSend for T where T: Serialize + DeserializeOwned + Send + 'static {}

type PackedValue = Box<dyn Any + Send + 'static>;
type PanicPayload = String;
pub struct PackedCall {
    args: Vec<PackedValue>,
    fn_id: FunctionLabel,
}
type FunctionLabel = u32; // function label is line
type ActivityReturn = std::result::Result<PackedValue, PanicPayload>;

pub trait Squashable: Send + 'static {
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
/// Lower 9 bits stands for the type of argument
type OrderLabel = u32; // lower 8 bit is for the position in side a function
const ARGUMENT_ORDER_BITS: u32 = 8;

// TODO: should impl serialize
#[derive(Default)]
struct SquashBuffer {
    args_list: Vec<Vec<u8>>, // has been serialized, must vec![] is there is no args
    fn_ids: Vec<FunctionLabel>,
    next_label: OrderLabel,
    squashable_map: FxHashMap<TypeId, Vec<(PackedValue, OrderLabel)>>, // type id of squashable, must not contain empty vector
    squashed_map: FxHashMap<TypeId, (PackedValue, Vec<OrderLabel>)>,   // type id of squashed, ditto
    ordered_squashable: Vec<Vec<(PackedValue, OrderLabel)>>, // still preserve label, for I don't want extra squash item struct
}

impl SquashBuffer {
    fn next_label(&mut self) -> OrderLabel {
        self.next_label = ((self.next_label >> ARGUMENT_ORDER_BITS) + 1) << ARGUMENT_ORDER_BITS;
        self.next_label
    }

    /// number of calls, used to determined when to send
    pub fn calls_num(&self) -> usize {
        self.fn_ids.len()
    }

    /// the label in squashable should be prepared by caller
    pub fn push(&mut self, item: SquashBufferItem) {
        let mut item = item;
        let next_label = self.next_label();
        for (_, l) in item.squashable.iter_mut() {
            *l |= next_label;
        }
        let SquashBufferItem {
            fn_id,
            args,
            squashable,
        } = item;
        self.fn_ids.push(fn_id);
        self.args_list.push(args);
        for (v, l) in squashable.into_iter() {
            match self.squashable_map.get_mut(&v.type_id()) {
                Some(list) => list.push((v, l)),
                None => {
                    self.squashable_map.insert(v.type_id(), vec![(v, l)]);
                }
            }
        }
    }

    pub fn pop(&mut self) -> Option<SquashBufferItem> {
        // the buffer must be in extracted state
        debug_assert!(self.squashable_map.is_empty());
        debug_assert!(self.squashed_map.is_empty());
        debug_assert_eq!(self.args_list.len(), self.fn_ids.len());
        let fn_id = self.fn_ids.pop()?;
        let args = self.args_list.pop().unwrap();
        let inflated = &self.ordered_squashable;
        let squashable = if !inflated.is_empty()
            && inflated[inflated.len() - 1][0].1 >> ARGUMENT_ORDER_BITS == self.fn_ids.len() as u32
        {
            self.ordered_squashable.pop().unwrap()
        } else {
            vec![]
        };
        Some(SquashBufferItem {
            args,
            fn_id,
            squashable,
        })
    }
}

#[derive(Default)]
struct SquashBufferItem {
    fn_id: FunctionLabel,
    args: Vec<u8>,
    squashable: Vec<(PackedValue, OrderLabel)>,
}

trait InsertSquashBufferIterm {
    fn construct(self, item: &mut SquashBufferItem, order: OrderLabel);
}
impl<T> InsertSquashBufferIterm for Vec<T>
where
    T: RemoteSend,
{
    fn construct(self, item: &mut SquashBufferItem, _order: OrderLabel) {
        use std::io::Write;
        serialize_into(&mut item.args, &self).unwrap();
    }
}
impl<T> InsertSquashBufferIterm for Box<T>
where
    T: Squashable,
{
    fn construct(self, item: &mut SquashBufferItem, order: OrderLabel) {
        item.squashable.push((self as PackedValue, order));
    }
}

trait SquashOperation {
    type DataType;
    fn call<T: Squashable>(buf: &mut Self::DataType);
}

struct SquashOneType;
impl SquashOperation for SquashOneType {
    type DataType = SquashBuffer;
    fn call<T: Squashable>(buf: &mut Self::DataType) {
        squash_one_type::<T>(buf);
    }
}

fn squash_all(buf: &mut SquashBuffer) {
    let typeids: Vec<_> = buf.squashable_map.keys().cloned().collect();
    for typeid in typeids {
        dispatch_for_squashable::<SquashOneType>(typeid, buf);
    }
}

fn squash_one_type<T: Squashable>(buf: &mut SquashBuffer) {
    let typeid = TypeId::of::<T>();
    let to_squash = buf.squashable_map.remove(&typeid).unwrap();
    let to_squash: Vec<_> = to_squash
        .into_iter()
        .map(|(s, a)| (s.downcast::<T>().unwrap(), a))
        .collect();
    let (a, b) = squash::<T>(to_squash);
    let a = a as PackedValue;
    buf.squashed_map.insert(
        typeid,
        (a, b), // squash::<T>(to_squash) as (PackedValue, Vec<OrderLabel>),
    );
}

fn squash<T: Squashable>(
    to_squash: Vec<(Box<T>, OrderLabel)>,
) -> (Box<T::Squashed>, Vec<OrderLabel>) {
    let mut to_squash = to_squash;
    T::arrange::<OrderLabel>(&mut to_squash[..]);
    let mut squashed = Box::new(T::Squashed::default());
    let mut labels = Vec::<_>::with_capacity(to_squash.len());
    for (s, order) in to_squash {
        s.fold(&mut *squashed);
        labels.push(order);
    }
    (squashed, labels)
}

struct ExtractOneType;
impl SquashOperation for ExtractOneType {
    type DataType = SquashBuffer;
    fn call<T: Squashable>(buf: &mut Self::DataType) {
        extract_one_type::<T>(buf);
    }
}

fn extract_all(buf: &mut SquashBuffer) {
    let typeids: Vec<_> = buf.squashable_map.keys().cloned().collect();
    for typeid in typeids {
        dispatch_for_squashable::<ExtractOneType>(typeid, buf);
    }
}

fn extract_one_type<T: Squashable>(buf: &mut SquashBuffer) {
    let typeid = TypeId::of::<T>();
    let (packed, labels) = buf.squashed_map.remove(&typeid).unwrap();
    let packed = packed.downcast::<T::Squashed>().unwrap();
    let extrated = extract_ordered::<T>((packed, labels));
    let extrated_packed: Vec<_> = extrated
        .into_iter()
        .map(|(v, l)| (v as PackedValue, l))
        .collect();
    buf.ordered_squashable.push(extrated_packed);
}

fn extract_ordered<T: Squashable>(
    to_inflate: (Box<T::Squashed>, Vec<OrderLabel>),
) -> Vec<(Box<T>, OrderLabel)> {
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
    inflated
}

fn dispatch_for_squashable<Op: SquashOperation>(typeid: TypeId, t: &mut Op::DataType) {
    if typeid == TypeId::of::<A>() {
        Op::call::<A>(t);
    } else if typeid == TypeId::of::<B>() {
        Op::call::<B>(t);
    } else {
        panic!();
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
struct A {
    value: usize,
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
        l.sort_by(|(a0, _), (a1, _)| a0.cmp(a1));
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

#[cfg(test)]
mod test {
    use super::*;
    use rand::prelude::*;

    #[test]
    pub fn test_squashbuffer_next_label() {
        let mut a = SquashBuffer::default();
        assert_eq!(a.next_label(), 256);
        assert_eq!(a.next_label(), 512);
    }

    impl Clone for SquashBufferItem {
        fn clone(&self) -> Self {
            let cbox = |v: &(PackedValue, OrderLabel)| {
                let (a, b) = v;
                (
                    Box::new((a.downcast_ref::<usize>().unwrap()).clone()) as PackedValue,
                    *b,
                )
            };

            SquashBufferItem {
                fn_id: self.fn_id,
                args: self.args.clone(),
                squashable: self.squashable.iter().map(cbox).collect(),
            }
        }
    }
    impl Eq for SquashBufferItem {}
    impl PartialEq for SquashBufferItem {
        fn eq(&self, i: &Self) -> bool {
            let mut ret = self.fn_id == i.fn_id
                && self.args == i.args
                && self.squashable.len() == i.squashable.len();
            for index in 0..self.squashable.len() {
                ret = ret && self.squashable[index].1 == i.squashable[index].1;
                ret = ret
                    && self.squashable[index].0.downcast_ref::<usize>().unwrap()
                        == i.squashable[index].0.downcast_ref::<usize>().unwrap();
                if !ret {
                    return ret;
                }
            }
            ret
        }
    }

    #[test]
    pub fn test_squashbuffer_push_pop() {
        let mut buf = SquashBuffer::default();
        let mut rng = thread_rng();
        let mut items: Vec<SquashBufferItem> = vec![];
        for _ in 0..50 {
            let i = SquashBufferItem {
                fn_id: rng.gen(),
                args: (0..64).map(|_| rng.gen()).collect(),
                squashable: vec![],
            };
            items.push(i);
        }
        for i in items.iter() {
            buf.push((*i).clone());
        }
        for i in items.iter().rev() {
            let last = buf.pop().unwrap();
        }
    }
}
// fn user_func_A(a: A, b: B, c: i32) -> R {
//     println!("A {:?} B {:?} C {}", a, b, c);
//     R { a, b, c }
// }
// fn user_func_B(a: A) -> A {
//     println!("---- {:?}", a);
//     a
// }
//
// fn gen_func_A_call_push(a: A, b: B, c: i32) {
//
//     //      get lock
//     //
//     // buffer.a.push()
// }
