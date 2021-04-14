use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use futures::task::FutureObj;
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::any::Any;
use std::any::TypeId;

extern crate serde;

pub trait RemoteSend: Serialize + DeserializeOwned + Send + 'static {}
impl<T> RemoteSend for T where T: Serialize + DeserializeOwned + Send + 'static {}

type PackedValue = Box<dyn Any + Send + 'static>;
type PanicPayload = String;
type FunctionLabel = u32; // function label is line
type ActivityId = u64;
type ActivityReturn = std::result::Result<PackedValue, PanicPayload>;

// Squashable is for large size type. Small size type should be wrapped in a largger one
//
// Reason 1: I attach each squashable a order label to indicate the original order for the
// possibility of reordering. Non-arrange squashable in squashed buffer must call different
// extracting functions for each function signature. Hard to implement
//
// Reason 2: for a small primitive type like i32, we want distinguish as certain i32 usage
// to other i32, since the pattern of a certain usage has compress ratio. So either:
// 1. wrapped in a unit struct
// 2. attach a id label. That voliate the intention for compression
// Primitive type with a pattern (ex. monotonous, same) usually relys on the order of occuring
// Multi thread generation for such a type would break that pattern. So it's better to wrap
// in a lager struct where a order label is negligible
//
pub trait Squashable: Any + Send + 'static {
    // squash type not necessarily to be serde
    type Squashed: RemoteSend + Default;
    /// folding is from left to right
    fn fold(&self, acc: &mut Self::Squashed);
    /// folding is from right to left
    fn extract(out: &mut Self::Squashed) -> Option<Self>
    where
        Self: Sized;
    fn arrange<L>(_list: &mut [(Box<Self>, L)]) { // default: no_op
    }
}

// TODO: Auto impl Squash macro by derive, squash
//
/// Lower 8 bits stands for the type of argument
type OrderLabel = u32; // lower 8 bit is for the position in side a function
const ARGUMENT_ORDER_BITS: u32 = 8;

// TODO: should impl serialize

#[derive(Default, Debug)]
struct SquashBuffer {
    args_list: Vec<Vec<u8>>, // has been serialized, must vec![] is there is no args
    fn_ids: Vec<FunctionLabel>,
    squashable_map: FxHashMap<TypeId, Vec<(PackedValue, OrderLabel)>>, // type id of squashable, must not contain empty vector
    squashed_map: FxHashMap<TypeId, (PackedValue, Vec<OrderLabel>)>,   // type id of squashed, ditto
    ordered_squashable: Vec<(PackedValue, OrderLabel)>, // still preserve label, for I don't want extra squash item struct
}

impl SquashBuffer {
    /// number of calls, used to determined when to send
    pub fn calls_num(&self) -> usize {
        self.fn_ids.len()
    }

    /// the label in squashable should be prepared by caller
    pub fn push(&mut self, item: SquashBufferItem) {
        let mut item = item;
        // the label of a argument is always == fn_ids.len() - 1
        let next_label = (self.fn_ids.len() << ARGUMENT_ORDER_BITS) as OrderLabel;
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
        // only change buf when squashable is not empty
        for (v, l) in squashable.into_iter() {
            // careful, should not get the type id of box!
            let type_id_inside_box: TypeId = (*v).type_id();
            match self.squashable_map.get_mut(&type_id_inside_box) {
                Some(list) => list.push((v, l)),
                None => {
                    self.squashable_map.insert(type_id_inside_box, vec![(v, l)]);
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
        let inflated = &mut self.ordered_squashable;
        let mut squashable = vec![];
        let current_label = self.fn_ids.len();
        while !inflated.is_empty()
            && inflated[inflated.len() - 1].1 >> ARGUMENT_ORDER_BITS == current_label as u32
        {
            let (a, b) = inflated.pop().unwrap();
            let b = b & ((1 << ARGUMENT_ORDER_BITS) - 1); // remove upper bits
            squashable.push((a, b));
        }
        squashable.reverse();
        Some(SquashBufferItem {
            args,
            fn_id,
            squashable,
        })
    }
}

#[derive(Default, Debug)]
struct SquashBufferItem {
    fn_id: FunctionLabel,
    args: Vec<u8>,
    squashable: Vec<(PackedValue, OrderLabel)>,
}
struct SquashBufferItemExtracter {
    position: usize,
    item: SquashBufferItem,
}
impl SquashBufferItemExtracter {
    pub fn new(item: SquashBufferItem) -> Self {
        let mut item = item;
        item.squashable.reverse(); // to have the same order as arg, reverse since pop from behind
        SquashBufferItemExtracter { position: 0, item }
    }
    pub fn extract_arg<T: RemoteSend>(&mut self) -> T {
        let mut read = &self.item.args[self.position..];
        let t = deserialize_from(&mut read).expect("Failed to deserizalize function argument");
        self.position = unsafe {
            read.as_ptr().offset_from(self.item.args.as_ptr()) as usize
        };
        t
    }
    pub fn extract_arg_squash<T: Squashable>(&mut self) -> T {
        *self
            .item
            .squashable
            .pop()
            .unwrap()
            .0
            .downcast::<T>()
            .unwrap()
    }
}

#[derive(Default, Debug)]
struct SquashBufferItemBuilder {
    next_label: OrderLabel,
    item: SquashBufferItem,
}
impl SquashBufferItemBuilder {
    pub fn new(fn_id: FunctionLabel, a_id: ActivityId) -> Self {
        let mut item = SquashBufferItem::default();
        item.fn_id = fn_id;
        SquashBufferItemBuilder {
            next_label: 0,
            item,
        }
    }
    fn next_label(&mut self) -> OrderLabel {
        let ret = self.next_label;
        debug_assert!(ret < 1 << ARGUMENT_ORDER_BITS);
        self.next_label += 1;
        ret
    }
    pub fn arg<T: RemoteSend>(&mut self, t: T) {
        serialize_into(&mut self.item.args, &t).expect("Failed to serialize function argument");
        self.next_label();
    }
    pub fn arg_squash<T: Squashable>(&mut self, t: T) {
        let label = self.next_label();
        self.item
            .squashable
            .push((Box::new(t) as PackedValue, label));
    }
    pub fn build(self) -> SquashBufferItem {
        self.item
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

// dispatch for different squash type of different squash operation
trait SquashDispatch<Op: SquashOperation> {
    fn dispatch_for_squashable(typeid: TypeId, t: &mut Op::DataType);
}

fn squash_all<T>(buf: &mut SquashBuffer)
where
    T: SquashDispatch<SquashOneType>,
{
    let typeids: Vec<_> = buf.squashable_map.keys().cloned().collect();
    for typeid in typeids {
        T::dispatch_for_squashable(typeid, buf);
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

fn extract_all<T>(buf: &mut SquashBuffer)
where
    T: SquashDispatch<ExtractOneType>,
{
    let typeids: Vec<_> = buf.squashed_map.keys().cloned().collect();
    for typeid in typeids {
        T::dispatch_for_squashable(typeid, buf);
    }
    buf.ordered_squashable.sort_unstable_by_key(|x| (*x).1);
}

fn extract_one_type<T: Squashable>(buf: &mut SquashBuffer) {
    let typeid = TypeId::of::<T>();
    let (packed, labels) = buf.squashed_map.remove(&typeid).unwrap();
    let packed = packed.downcast::<T::Squashed>().unwrap();
    extract_into::<T>((packed, labels), &mut buf.ordered_squashable);
}

fn extract_into<T: Squashable>(
    to_inflate: (Box<T::Squashed>, Vec<OrderLabel>),
    to: &mut Vec<(PackedValue, OrderLabel)>,
) {
    let (mut squashed, labels) = to_inflate;
    let mut count = 0;
    let borrow: &mut T::Squashed = &mut *squashed;
    while let Some(s) = T::extract(borrow) {
        to.push((Box::new(s) as PackedValue, labels[labels.len() - count - 1]));
        count += 1;
    }
    debug_assert_eq!(count, labels.len());
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::prelude::*;
    use rand::seq::SliceRandom;
    use std::convert::TryInto;

    fn _clone<T: Send + Clone + 'static>(this: &SquashBufferItem) -> SquashBufferItem {
        let cbox = |v: &(PackedValue, OrderLabel)| {
            let (a, b) = v;
            (
                Box::new(a.downcast_ref::<T>().unwrap().clone()) as PackedValue,
                *b,
            )
        };
        SquashBufferItem {
            fn_id: this.fn_id,
            args: this.args.clone(),
            squashable: this.squashable.iter().map(cbox).collect(),
        }
    }
    fn _eq<T: Send + PartialEq + 'static>(this: &SquashBufferItem, i: &SquashBufferItem) -> bool {
        let mut ret = this.fn_id == i.fn_id
            && this.args == i.args
            && this.squashable.len() == i.squashable.len();
        for index in 0..this.squashable.len() {
            ret = ret && this.squashable[index].1 == i.squashable[index].1;
            ret = ret
                && this.squashable[index].0.downcast_ref::<T>().unwrap()
                    == i.squashable[index].0.downcast_ref::<T>().unwrap();
            if !ret {
                return false;
            }
        }
        ret
    }

    mod usizepacked {
        use super::*;
        impl Clone for SquashBufferItem {
            fn clone(&self) -> Self {
                _clone::<usize>(self)
            }
        }
        impl Eq for SquashBufferItem {}
        impl PartialEq for SquashBufferItem {
            fn eq(&self, i: &Self) -> bool {
                _eq::<usize>(self, i)
            }
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
            assert_eq!(*i, buf.pop().unwrap());
        }
    }

    struct ConcreteDispatch {}
    impl<Op> SquashDispatch<Op> for ConcreteDispatch
    where
        Op: SquashOperation,
    {
        fn dispatch_for_squashable(typeid: TypeId, t: &mut Op::DataType) {
            if typeid == TypeId::of::<A>() {
                Op::call::<A>(t);
            } else if typeid == TypeId::of::<B>() {
                Op::call::<B>(t);
            } else {
                panic!();
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
    struct A {
        value: usize,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Default)]
    struct AOut {
        last: usize,
        diffs: Vec<u8>,
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
    struct B {
        value: u8,
    }
    #[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

    #[test]
    pub fn test_squash_extract() {
        let mut rng = thread_rng();

        let mut a_list: Vec<_> = (1..65).map(|value| Box::new(A { value })).collect();
        let a_len = a_list.len();
        a_list.shuffle(&mut rng);
        let mut a_list: Vec<_> = a_list
            .into_iter()
            .zip(0..a_len)
            .map(|(a, b)| (a, b as OrderLabel))
            .collect(); // assign order
        Squashable::arrange(&mut a_list[..]);
        let squashed = squash::<A>(a_list.clone());
        let mut extracted = vec![];
        extract_into::<A>(squashed, &mut extracted);
        let mut extracted: Vec<_> = extracted
            .into_iter()
            .map(|(a, b)| (a.downcast::<A>().unwrap(), b))
            .collect();
        // there is no order in extract into, so sort here for compare
        extracted.sort_by_key(|(_, b)| *b);
        a_list.sort_by_key(|(_, b)| *b);
        assert_eq!(a_list, extracted);

        let b_list: Vec<_> = (0..64)
            .map(|value| (Box::new(B { value }), value as OrderLabel))
            .collect();
        let mut extracted = vec![];
        extract_into::<B>(squash(b_list.clone()), &mut extracted);
        let mut extracted: Vec<_> = extracted
            .into_iter()
            .map(|(b, l)| (b.downcast::<B>().unwrap(), l))
            .collect();
        extracted.sort_by_key(|(_, b)| *b);
        assert_eq!(b_list, extracted);
    }

    fn crate_squash_item(a: A, b: B, fn_id: FunctionLabel) -> SquashBufferItem {
        let mut item = SquashBufferItem::default();
        item.fn_id = fn_id;
        item.squashable
            .push((Box::new(a.clone()) as PackedValue, 1));
        item.squashable
            .push((Box::new(b.clone()) as PackedValue, 2));
        item.squashable.push((Box::new(a) as PackedValue, 3));
        item.squashable.push((Box::new(b) as PackedValue, 4));
        item
    }
    fn extract_squash_item(item: SquashBufferItem) -> (A, B, FunctionLabel) {
        assert_eq!(item.squashable.len(), 4);
        let SquashBufferItem {
            fn_id,
            args,
            mut squashable,
        } = item;
        drop(args);
        let (_, v) = squashable.pop().unwrap();
        assert_eq!(v, 4);
        let (_, v) = squashable.pop().unwrap();
        assert_eq!(v, 3);
        let (b, v) = squashable.pop().unwrap();
        assert_eq!(v, 2);
        let (a, v) = squashable.pop().unwrap();
        assert_eq!(v, 1);
        (
            *a.downcast::<A>().unwrap(),
            *b.downcast::<B>().unwrap(),
            fn_id,
        )
    }

    #[test]
    pub fn test_squash_extract_all() {
        let mut rng = thread_rng();
        // prepare a
        let mut a_list: Vec<_> = (1..65).map(|value| A { value }).collect();
        a_list.shuffle(&mut rng);
        // prepare b
        let mut b_list: Vec<_> = (0..64).map(|value| B { value }).collect();
        b_list.shuffle(&mut rng);
        let calls: Vec<_> = a_list
            .into_iter()
            .zip(b_list.into_iter())
            .map(|(a, b)| (a, b, rng.gen()))
            .collect();
        let items: Vec<_> = calls
            .iter()
            .cloned()
            .map(|(a, b, f)| crate_squash_item(a, b, f))
            .collect();
        let mut buf = SquashBuffer::default();
        for i in items {
            buf.push(i)
        }
        squash_all::<ConcreteDispatch>(&mut buf);
        extract_all::<ConcreteDispatch>(&mut buf);

        let mut out = vec![];
        while let Some(i) = buf.pop() {
            out.push(i);
        }
        let get_calls: Vec<_> = out
            .into_iter()
            .rev()
            .map(|i| extract_squash_item(i))
            .collect();
        assert_eq!(calls, get_calls);
    }

    #[test]
    pub fn test_item_build_extract(){

        let fn_id: FunctionLabel = 567; // feed by macro
        let activity_id = 123; //
        let a = 333i32;
        let b = A{value:12355};
        let c = 444i32;
        let d = B{value:56};
        let mut builder = SquashBufferItemBuilder::new(fn_id, activity_id);
        builder.arg(a);
        builder.arg_squash(b.clone());
        builder.arg(c);
        builder.arg_squash(d.clone());
        let mut item = builder.build();
        assert_eq!(item.fn_id, fn_id);
        // assert_eq!(activity, fn_id); TODO: activity id
        assert_eq!(item.squashable[0].1, 1);
        assert_eq!(item.squashable[1].1, 3);
        let mut ex = SquashBufferItemExtracter::new(item);
        assert_eq!(a, ex.extract_arg());
        assert_eq!(b, ex.extract_arg_squash());
        assert_eq!(c, ex.extract_arg());
        assert_eq!(d, ex.extract_arg_squash());
    }

    fn real_fn(a: i32, b: i32, c: i32) -> usize {
        0
    }

    // fn desugered_async(ctx: &Context, place: place, a: i32, b: A, c: i32) {
    //     let fn_id: FunctionLabel = 0; // feed by macro
    //     let activity_id = 123; //
    //                            // ctx.gen_activity_id();
    //
    //     let mut builder = SquashBufferItemBuilder::new(fn_id, activity_id);
    //     builder.arg(a);
    //     builder.arg_squash(b);
    //     builder.arg(c);
    //     let item = builder.build();
    //     ctx.send(item);
    // }
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
