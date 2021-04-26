pub use crate::global_id::ActivityId;
use crate::place::Place;
use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde::de::MapAccess;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::any::Any;
use std::any::TypeId;
use std::fmt;
use std::marker::PhantomData;

extern crate serde;

pub trait RemoteSend: Serialize + DeserializeOwned + Send + 'static {}
impl<T> RemoteSend for T where T: Serialize + DeserializeOwned + Send + 'static {}

pub type PackedValue = Box<dyn Any + Send + 'static>;
pub type PanicPayload = String;
pub type FunctionLabel = u32; // function label is line
pub type ActivityResult = std::result::Result<(), PanicPayload>;

pub fn cast_panic_payload(payload: Box<dyn Any + Send + 'static>) -> PanicPayload {
    let id = (*payload).type_id();
    if id == TypeId::of::<String>() {
        *payload.downcast::<String>().unwrap()
    } else if id == TypeId::of::<&str>() {
        String::from(*payload.downcast::<&str>().unwrap())
    } else {
        String::from("Unsupport payload type")
    }
}

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
// TODO: arrange squash

type SquashableMap = FxHashMap<TypeId, Vec<(PackedValue, OrderLabel)>>;
type SquashedMap = FxHashMap<TypeId, (PackedValue, Vec<OrderLabel>)>;
type OrderedSquashable = Vec<(PackedValue, OrderLabel)>;

#[derive(Debug)]
pub struct SquashBuffer<D> {
    // TODO boxex
    items: Vec<StrippedTaskItem>,
    squashable_map: SquashableMap, // type id of squashable, must not contain empty vector
    squashed_map: SquashedMap,     // type id of squashed, ditto
    ordered_squashable: OrderedSquashable, // still preserve label, for I don't want extra squash item struct
    _marker: PhantomData<D>,
}

impl<D> SquashBuffer<D>
where
    D: for<'a> SquashDispatch<SquashOneType<'a>> + for<'a> SquashDispatch<ExtractOneType<'a>>,
{

    pub fn new() -> Self{
        Self::default()
    }

    pub fn default() -> Self{
        // derive default requrie D to be default, so here manually implement
        SquashBuffer::<D>{
            items: vec![],
            squashable_map: SquashableMap::default(),
            squashed_map: SquashedMap::default(),
            ordered_squashable: OrderedSquashable::default(),
            _marker: PhantomData::<D>::default()
        }
    }

    /// number of calls, used to determined when to send
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// the label in squashable should be prepared by caller
    pub fn push(&mut self, item: TaskItem) {
        let mut item = item;
        // the label of a argument is always == fn_ids.len() - 1
        let next_label = (self.items.len() << ARGUMENT_ORDER_BITS) as OrderLabel;
        for (_, l) in item.squashable.iter_mut() {
            *l |= next_label;
        }
        let TaskItem { inner, squashable } = item;
        self.items.push(inner);
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

    pub fn pop(&mut self) -> Option<TaskItem> {
        // the buffer must be in extracted state
        debug_assert!(self.squashable_map.is_empty());
        debug_assert!(self.squashed_map.is_empty());
        let item = self.items.pop()?;
        let inflated = &mut self.ordered_squashable;
        let mut squashable = vec![];
        let current_label = self.items.len();
        while !inflated.is_empty()
            && inflated[inflated.len() - 1].1 >> ARGUMENT_ORDER_BITS == current_label as u32
        {
            let (a, b) = inflated.pop().unwrap();
            let b = b & ((1 << ARGUMENT_ORDER_BITS) - 1); // remove upper bits
            squashable.push((a, b));
        }
        squashable.reverse();
        Some(TaskItem {
            inner: item,
            squashable,
        })
    }

    fn squash_all(&mut self) {
        let typeids: Vec<_> = self.squashable_map.keys().cloned().collect();
        for typeid in typeids {
            <D as SquashDispatch<SquashOneType>>::dispatch_for_squashable(
                typeid,
                (&mut self.squashable_map, &mut self.squashed_map),
            );
        }
    }

    fn extract_all(&mut self) {
        let typeids: Vec<_> = self.squashed_map.keys().cloned().collect();
        for typeid in typeids {
            <D as SquashDispatch<ExtractOneType>>::dispatch_for_squashable(
                typeid,
                (&mut self.squashed_map, &mut self.ordered_squashable),
            );
        }
        self.ordered_squashable.sort_unstable_by_key(|x| (*x).1);
    }
}

/*
impl Serialize for SquashBuffer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        debug_assert!(self.squashable_map.is_empty());
        debug_assert!(self.ordered_squashable.is_empty());
        self.items.serialize(serializer)?;

        let mut map = serializer.serialize_map(Some(self.squashed_map.len()))?;
        for (id, v) in self.squashed_map.len() {
            map.serialize_entry(id, v)?;
        }
        map.end()
    }
}

fn deserialize_pack<T>(type_id: TypeId, aceess: M) -> Result<(PackedValue, Vec<OrderLabel>), D::Error>
where
    M: MapAccess,
    T: Squashable,
{
}

struct DeserializeBuffer<'de, M> where M:MapAccess<'de>{
    phantom: PhantomData<M>,
}

impl<'de, M> SquashOperation for DeserializeBuffer<'de, M> where M:MapAccess<'de>{
    type DataType = M;
    type ReturnType = Result<PackedValue, Vec<OrderLabel>>;
    fn call<T: Squashable>(access: Self::DataType) -> Self::ReturnType {
        let (v, s) = args;
        let (v, labels) = access.next_value::<(T::Squashed, Vec<OrderLabel>)>()?;
        Ok((Box::new(v) as PackedValue, labels))
    }
}

// impl<'de> Deserialize<'de> for SquashBuffer {
fn deserialize<T, D>(deserializer: D) -> Result<SquashBuffer, D::Error>
where
    D: Deserializer<'de>,
    T: SquashDispatch<DeserializeBuffer<>>
{
    let items = Vec::<StrippedTaskItem>::deserialize(deserializer);

    struct MapVisitor;
    impl<'de> Visitor<'de> for MapVisitor {
        type Value = FxHashMap<TypeId, (PackedValue, Vec<OrderLabel>)>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("packed value map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map = FxHashMap::with_capacity(access.size_hint().unwrap_or(0));

            while let Some(type_id) = access.next_key()? {
                let packed = deserialize_pack(type_id, access);
                let packed =
                map.insert(type_id, packed);
            }

            Ok(map)
        }
    }
    let squashed_map = deserializer.deserialize_map(MapVisitor {});
    SquashBuffer {
        items,
        squashable_map: FxHashMap::default(),
        squashed_map,
        ordered_squashable: FxHashMap::default(),
    }
}
// }

impl Serialize for PackedValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_squashed::<Something, S>(&self, serializer);
    }
}

struct SerializeSquash<'a, S: Serializer> {
    phantom: PhantomData<&'a S>,
}

impl<'a, S: Serializer> SquashOperation for SerializeSquash<'a, S> {
    type DataType = (&'a PackedValue, S);
    type ReturnType = Result<S::Ok, S::Error>;
    fn call<T: Squashable>(args: Self::DataType) -> Self::ReturnType {
        let (v, s) = args;
        // current trait method has not ret, so I just unwrap here
        v.downcast_ref::<T::Squashed>().serialize(s)
    }
}

fn serialize_squashed<T, S>(packed: &PackedValue, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: for<'a> SquashDispatch<SerializeSquash<'a, S>>,
{
    T::dispatch_for_squashable(packed.type_id(), (packed, serializer));
}

*/
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ReturnInfo {
    result: ActivityResult,
    sub_activities: Vec<ActivityId>,
}

#[derive(Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct StrippedTaskItem {
    fn_id: FunctionLabel,
    place: Place, // it's dst place, request place or return place
    activity_id: ActivityId,
    ret: Option<ReturnInfo>,
    waited: bool, // indicating this activity is waited on the spawned place
    args: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct TaskItem {
    inner: StrippedTaskItem,
    squashable: Vec<(PackedValue, OrderLabel)>,
}
impl TaskItem {
    pub fn is_ret(&self) -> bool {
        self.inner.ret.is_some()
    }
    pub fn place(&self) -> Place {
        self.inner.place
    }
    pub fn activity_id(&self) -> ActivityId {
        self.inner.activity_id
    }
    pub fn is_waited(&self) -> bool {
        self.inner.waited
    }
}

pub struct TaskItemExtracter {
    position: usize,
    item: TaskItem,
}
impl TaskItemExtracter {
    pub fn new(item: TaskItem) -> Self {
        let mut item = item;
        item.squashable.reverse(); // to have the same order as arg, reverse since pop from behind
        TaskItemExtracter { position: 0, item }
    }
    pub fn arg<T: RemoteSend>(&mut self) -> T {
        let mut read = &self.item.inner.args[self.position..];
        let t = deserialize_from(&mut read).expect("Failed to deserialize function argument");
        self.position =
            unsafe { read.as_ptr().offset_from(self.item.inner.args.as_ptr()) as usize };
        t
    }
    pub fn arg_squash<T: Squashable>(&mut self) -> T {
        *self
            .item
            .squashable
            .pop()
            .unwrap()
            .0
            .downcast::<T>()
            .unwrap()
    }
    pub fn ret<T: RemoteSend>(&mut self) -> Result<T, PanicPayload> {
        let ret_info = self.item.inner.ret.take().unwrap();
        match ret_info.result {
            Ok(()) => Ok(self.arg()),
            Err(e) => Err(e),
        }
    }
    pub fn ret_squash<T: Squashable>(&mut self) -> Result<T, PanicPayload> {
        let ret_info = self.item.inner.ret.take().unwrap();
        match ret_info.result {
            Ok(()) => Ok(self.arg_squash()),
            Err(e) => Err(e),
        }
    }
    /// consume and discard the return value but get panic payload
    pub fn ret_panic(&mut self) -> ActivityResult {
        let ret_info = self.item.inner.ret.take().unwrap();
        ret_info.result
    }
    /// should be called before ret_xxx
    pub fn sub_activities(&mut self) -> Vec<ActivityId> {
        std::mem::replace(
            &mut self.item.inner.ret.as_mut().unwrap().sub_activities,
            vec![],
        )
    }
    pub fn fn_id(&self) -> FunctionLabel {
        self.item.inner.fn_id
    }
    pub fn place(&self) -> Place {
        self.item.inner.place
    }
    pub fn activity_id(&self) -> ActivityId {
        self.item.inner.activity_id
    }
}

#[derive(Default, Debug)]
pub struct TaskItemBuilder {
    next_label: OrderLabel,
    item: TaskItem,
}
impl TaskItemBuilder {
    pub fn new(fn_id: FunctionLabel, place: Place, a_id: ActivityId) -> Self {
        let mut item = TaskItem::default();
        item.inner.fn_id = fn_id;
        item.inner.place = place;
        item.inner.activity_id = a_id;

        TaskItemBuilder {
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

    pub fn waited(&mut self) {
        self.item.inner.waited = true;
    }

    pub fn arg(&mut self, t: impl RemoteSend) {
        serialize_into(&mut self.item.inner.args, &t)
            .expect("Failed to serialize function argument");
        self.next_label();
    }
    pub fn arg_squash(&mut self, t: impl Squashable) {
        let label = self.next_label();
        self.item
            .squashable
            .push((Box::new(t) as PackedValue, label));
    }
    pub fn build(self) -> TaskItem {
        self.item
    }
    pub fn build_box(self) -> Box<TaskItem> {
        Box::new(self.build())
    }
    fn set_result(&mut self, result: ActivityResult) {
        debug_assert!(self.item.inner.ret.is_none());
        self.item.inner.ret = Some(ReturnInfo {
            result,
            sub_activities: vec![],
        });
    }
    pub fn ret(&mut self, result: std::thread::Result<impl RemoteSend>) {
        let result = match result {
            Ok(ret) => {
                self.arg(ret);
                Ok(())
            }
            Err(payload) => Err(cast_panic_payload(payload)),
        };
        self.set_result(result);
    }
    pub fn ret_squash(&mut self, result: std::thread::Result<impl Squashable>) {
        let result = match result {
            Ok(ret) => {
                self.arg_squash(ret);
                Ok(())
            }
            Err(payload) => Err(cast_panic_payload(payload)),
        };
        self.set_result(result);
    }
    pub fn sub_activities(&mut self, a_ids: Vec<ActivityId>) {
        let _ = std::mem::replace(
            &mut self
                .item
                .inner
                .ret
                .as_mut()
                .expect("result must be set before sub_activities")
                .sub_activities,
            a_ids,
        );
    }
}

// dispatch for different squash type of different squash operation
pub trait SquashDispatch<Op: SquashOperation> {
    fn dispatch_for_squashable(typeid: TypeId, t: Op::DataType) -> Op::ReturnType;
}

// I have to make these public, since Squashbuffer leaks them
pub trait SquashOperation {
    type DataType;
    type ReturnType;
    fn call<T: Squashable>(buf: Self::DataType) -> Self::ReturnType;
}

pub struct SquashOneType<'a> {
    phantom: PhantomData<&'a usize>,
}

impl<'a> SquashOperation for SquashOneType<'a> {
    type DataType = (&'a mut SquashableMap, &'a mut SquashedMap);
    type ReturnType = ();
    // here pass by map ref tuple instead of squashmap to avoid recursive type
    fn call<T: Squashable>(args: Self::DataType) -> Self::ReturnType {
        squash_one_type::<T>(args);
    }
}

fn squash_one_type<T: Squashable>(args: (&mut SquashableMap, &mut SquashedMap)) {
    let (squashable_map, squashed_map) = args;
    let typeid = TypeId::of::<T>();
    let to_squash = squashable_map.remove(&typeid).unwrap();
    let to_squash: Vec<_> = to_squash
        .into_iter()
        .map(|(s, a)| (s.downcast::<T>().unwrap(), a))
        .collect();
    let (a, b) = squash::<T>(to_squash);
    let a = a as PackedValue;
    squashed_map.insert(
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

pub struct ExtractOneType<'a> {
    phantom: PhantomData<&'a usize>,
}
impl<'a> SquashOperation for ExtractOneType<'a> {
    type DataType = (&'a mut SquashedMap, &'a mut OrderedSquashable);
    type ReturnType = ();
    fn call<T: Squashable>(args: Self::DataType) -> Self::ReturnType {
        extract_one_type::<T>(args);
    }
}

fn extract_one_type<T: Squashable>(args: (&mut SquashedMap, &mut OrderedSquashable)) {
    let (squashed_map, ordered_squashable) = args;
    let typeid = TypeId::of::<T>();
    let (packed, labels) = squashed_map.remove(&typeid).unwrap();
    let packed = packed.downcast::<T::Squashed>().unwrap();
    extract_into::<T>((packed, labels), ordered_squashable);
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
pub mod test {
    use super::*;
    use rand::prelude::*;
    use rand::seq::SliceRandom;
    use std::convert::TryInto;

    fn _clone<T: Send + Clone + 'static>(this: &TaskItem) -> TaskItem {
        let cbox = |v: &(PackedValue, OrderLabel)| {
            let (a, b) = v;
            (
                Box::new(a.downcast_ref::<T>().unwrap().clone()) as PackedValue,
                *b,
            )
        };
        TaskItem {
            inner: StrippedTaskItem {
                fn_id: this.inner.fn_id,
                place: this.inner.place,
                waited: false,
                activity_id: this.inner.activity_id,
                ret: this.inner.ret.as_ref().map(|retinfo| ReturnInfo {
                    result: retinfo.result.clone(),
                    sub_activities: retinfo.sub_activities.clone(),
                }),
                args: this.inner.args.clone(),
            },
            squashable: this.squashable.iter().map(cbox).collect(),
        }
    }
    fn _eq<T: Send + PartialEq + 'static>(this: &TaskItem, i: &TaskItem) -> bool {
        let mut ret = this.inner == i.inner && this.squashable.len() == i.squashable.len();
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
        impl Clone for TaskItem {
            fn clone(&self) -> Self {
                _clone::<usize>(self)
            }
        }
        impl Eq for TaskItem {}
        impl PartialEq for TaskItem {
            fn eq(&self, i: &Self) -> bool {
                _eq::<usize>(self, i)
            }
        }
    }
    use rand::distributions::Alphanumeric;
    #[test]
    pub fn test_squashbuffer_push_pop() {
        let mut buf = SquashBuffer::<ConcreteDispatch>::new();
        let mut rng = thread_rng();
        let mut items: Vec<TaskItem> = vec![];
        for _ in 0..50 {
            let s: String = (&mut rng)
                .sample_iter(Alphanumeric)
                .take(7)
                .map(char::from)
                .collect();

            let i = TaskItem {
                inner: StrippedTaskItem {
                    fn_id: rng.gen(),
                    place: rng.gen(),
                    activity_id: rng.gen(),
                    waited: false,
                    ret: Some(ReturnInfo {
                        result: Err(s),
                        sub_activities: (0..8).map(|_| rng.gen()).collect(),
                    }),
                    args: (0..64).map(|_| rng.gen()).collect(),
                },
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
        fn dispatch_for_squashable(typeid: TypeId, t: Op::DataType) -> Op::ReturnType {
            if typeid == TypeId::of::<A>() || typeid == TypeId::of::<<A as Squashable>::Squashed>()
            {
                Op::call::<A>(t)
            } else if typeid == TypeId::of::<B>()
                || typeid == TypeId::of::<<B as Squashable>::Squashed>()
            {
                Op::call::<B>(t)
            } else {
                panic!()
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord)]
    pub struct A {
        pub value: usize,
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Default)]
    pub struct AOut {
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

    fn crate_squash_item(a: A, b: B, fn_id: FunctionLabel) -> TaskItem {
        let mut rng = thread_rng();
        let mut builder = TaskItemBuilder::new(fn_id, rng.gen(), rng.gen());
        builder.arg_squash(a.clone());
        builder.arg_squash(b.clone());
        builder.arg_squash(a);
        builder.arg_squash(b);
        builder.build()
    }
    fn extract_squash_item(item: TaskItem) -> (A, B, FunctionLabel) {
        assert_eq!(item.squashable.len(), 4);
        let mut e = TaskItemExtracter::new(item);
        let _: A = e.arg_squash();
        let _: B = e.arg_squash();
        let a: A = e.arg_squash();
        let b: B = e.arg_squash();
        let fn_id = e.fn_id();
        (a, b, fn_id)
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
        let mut buf = SquashBuffer::<ConcreteDispatch>::new();
        for i in items {
            buf.push(i)
        }
        buf.squash_all();
        buf.extract_all();

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
    pub fn test_item_build_extract() {
        let mut rng = thread_rng();
        let fn_id: FunctionLabel = 567; // feed by macro
        let place: Place = 444;
        let activity_id: ActivityId = 123; //
        let a = 333i32;
        let b = A { value: 12355 };
        let c = 444i32;
        let d = B { value: 56 };
        let mut builder = TaskItemBuilder::new(fn_id, place, activity_id);
        builder.arg(a);
        builder.arg_squash(b.clone());
        builder.arg(c);
        builder.arg_squash(d.clone());
        let item = builder.build();
        assert_eq!(item.inner.fn_id, fn_id);
        assert_eq!(item.inner.place, place);
        assert_eq!(item.inner.activity_id, activity_id);
        assert_eq!(item.squashable[0].1, 1);
        assert_eq!(item.squashable[1].1, 3);
        let mut ex = TaskItemExtracter::new(item);
        assert_eq!(a, ex.arg());
        assert_eq!(b, ex.arg_squash());
        assert_eq!(c, ex.arg());
        assert_eq!(d, ex.arg_squash());
        assert_eq!(ex.fn_id(), fn_id);
        assert_eq!(ex.place(), place);
        assert_eq!(ex.activity_id(), activity_id);

        let activities: Vec<ActivityId> = (0..8).map(|_| rng.gen()).collect();
        // ret ok
        let result = Ok(1234usize);
        let mut builder = TaskItemBuilder::new(fn_id, place, activity_id);
        builder.ret(result);
        builder.sub_activities(activities.clone());
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.sub_activities(), activities);
        assert_eq!(ex.ret::<usize>().unwrap(), 1234usize);

        // ret squash ok
        let result = Ok(A { value: 4577 });
        let mut builder = TaskItemBuilder::new(fn_id, place, activity_id);
        builder.ret_squash(result);
        builder.sub_activities(activities.clone());
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.sub_activities(), activities);
        assert_eq!(ex.ret_squash::<A>().unwrap(), A { value: 4577 });

        // ret Err
        let msg = String::from("123125435");
        let result = Box::new(msg.clone()) as Box<dyn Any + 'static + Send>;
        let result: std::thread::Result<usize> = Err(result);
        let mut builder = TaskItemBuilder::new(fn_id, place, activity_id);
        builder.ret(result);
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.ret::<usize>().unwrap_err(), msg);

        // ret Err
        let msg = String::from("123125435");
        let result = Box::new(msg.clone()) as Box<dyn Any + 'static + Send>;
        let result: std::thread::Result<A> = Err(result);
        let mut builder = TaskItemBuilder::new(fn_id, place, activity_id);
        builder.ret_squash(result);
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.ret_squash::<A>().unwrap_err(), msg);
    }

    #[test]
    pub fn test_cast_panic_payload() {
        use std::panic;

        // silent current panic handler
        panic::set_hook(Box::new(|_| {}));
        let result = panic::catch_unwind(|| panic!("12345"));
        let _ = panic::take_hook();
        assert_eq!(cast_panic_payload(result.unwrap_err()), "12345");

        let mut rng = thread_rng();
        let value: usize = rng.gen();
        #[allow(non_fmt_panic)]
        let func = |a: usize| panic!(format!("1{}", a));
        panic::set_hook(Box::new(|_| {}));
        let result = panic::catch_unwind(|| {
            func(value);
        });
        let _ = panic::take_hook();
        assert_eq!(
            cast_panic_payload(result.unwrap_err()),
            format!("1{}", value)
        );
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
