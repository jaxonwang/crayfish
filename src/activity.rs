pub use crate::global_id::ActivityId;
use crate::place::Place;
use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde::de::MapAccess;
use serde::de::Visitor;
use serde::ser::SerializeMap;
use serde::ser::SerializeTuple;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::any::Any;
use std::any::TypeId;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;

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

pub fn copy_panic_payload(
    payload: &Box<dyn Any + Send + 'static>,
) -> Box<dyn Any + Send + 'static> {
    let id = (**payload).type_id();
    if id == TypeId::of::<String>() {
        Box::new(payload.downcast_ref::<String>().unwrap().clone())
    } else if id == TypeId::of::<&str>() {
        Box::new(String::from(*payload.downcast_ref::<&str>().unwrap()))
    } else {
        Box::new(String::from("Unsupport payload type"))
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
pub type SquashedMapValue = (PackedValue, Vec<OrderLabel>);
type SquashedMap = FxHashMap<TypeId, SquashedMapValue>;
type OrderedSquashable = Vec<(PackedValue, OrderLabel)>;

// I write this wrapper to implement the serialization for squashbuffer
// Certainly that this design is crap. But I can't find a better one
#[derive(Debug)]
struct SquashedMapWrapper<D> {
    _mark: PhantomData<D>,
    m: SquashedMap,
}

impl<D> Default for SquashedMapWrapper<D> {
    fn default() -> Self {
        SquashedMapWrapper {
            _mark: PhantomData::<D>::default(),
            m: FxHashMap::default(),
        }
    }
}

impl<D> Deref for SquashedMapWrapper<D> {
    type Target = SquashedMap;
    fn deref(&self) -> &Self::Target {
        &self.m
    }
}

impl<D> DerefMut for SquashedMapWrapper<D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.m
    }
}

// stupid traits
pub trait ProperDispatcher:
    for<'a> SquashDispatch<SquashOneType<'a>>
    + for<'a> SquashDispatch<ExtractOneType<'a>>
    + Send
    + 'static
{
}

impl<D> Serialize for SquashedMapWrapper<D>
where
    D: ProperDispatcher,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.m.len()))?;
        for (id, v) in self.m.iter() {
            map.serialize_key(&type_id_to_u64(*id)).unwrap();
            <D as SquashDispatch<SquashOneType>>::dispatch_serialize_entry(*id, v, &mut map)
                .unwrap();
        }
        map.end()
    }
}

impl<'de, DpRoot> Deserialize<'de> for SquashedMapWrapper<DpRoot>
where
    DpRoot: ProperDispatcher,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueMapVisitor<Dp> {
            _mark: PhantomData<Dp>,
        }

        impl<'de, Dp> Visitor<'de> for ValueMapVisitor<Dp>
        where
            Dp: ProperDispatcher + ProperDispatcher,
        {
            type Value = SquashedMap;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("packed value map")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut map = FxHashMap::default();

                while let Some(type_id) = access.next_key::<u64>()? {
                    let type_id = u64_to_type_id(type_id);
                    let value: SquashedMapValue =
                        <Dp as SquashDispatch<SquashOneType>>::dispatch_deserialize_entry(
                            type_id,
                            &mut access,
                        )?;
                    map.insert(type_id, value);
                }

                Ok(map)
            }
        }

        let m = deserializer.deserialize_map(ValueMapVisitor::<DpRoot> {
            _mark: PhantomData::default(),
        })?;

        Ok(SquashedMapWrapper::<DpRoot> {
            m,
            ..Default::default()
        })
    }
}

pub trait AbstractSquashBuffer: Send {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn push(&mut self, item: TaskItem);
    fn pop(&mut self) -> Option<TaskItem>;
    fn squash_all(&mut self);
    fn extract_all(&mut self);
    fn clear(&mut self);
    fn serialize_and_clear(&mut self) -> Vec<u8>;
}

pub trait AbstractSquashBufferFactory: Send {
    fn new_buffer(&self) -> Box<dyn AbstractSquashBuffer>;
    fn deserialize_from(&self, bytes: &[u8]) -> Box<dyn AbstractSquashBuffer>;
}

pub trait StaticSquashBufferFactory {
    // only used in network message callback
    fn deserialize_from(bytes: &[u8]) -> Box<dyn AbstractSquashBuffer>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SquashBuffer<D>
where
    D: ProperDispatcher,
{
    // TODO boxex
    items: Vec<StrippedTaskItem>,
    #[serde(skip)]
    squashable_map: SquashableMap, // type id of squashable, must not contain empty vector
    squashed_map: SquashedMapWrapper<D>, // type id of squashed, ditto
    #[serde(skip)]
    ordered_squashable: OrderedSquashable, // still preserve label, for I don't want extra squash item struct
}

impl<D> Default for SquashBuffer<D>
where
    D: ProperDispatcher,
{
    fn default() -> Self {
        SquashBuffer {
            items: vec![],
            squashable_map: SquashableMap::default(),
            squashed_map: SquashedMapWrapper::<D>::default(),
            ordered_squashable: OrderedSquashable::default(),
        }
    }
}

impl<D> SquashBuffer<D>
where
    D: ProperDispatcher,
{
    pub fn new() -> Self {
        Self::default()
    }
}
impl<D> AbstractSquashBuffer for SquashBuffer<D>
where
    D: ProperDispatcher + DeserializeOwned + Serialize,
{
    /// number of calls, used to determined when to send
    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// the label in squashable should be prepared by caller
    fn push(&mut self, item: TaskItem) {
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

    fn pop(&mut self) -> Option<TaskItem> {
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

    fn clear(&mut self) {
        self.items.clear();
        self.squashable_map.clear();
        self.squashed_map.clear();
        self.ordered_squashable.clear();
    }

    fn serialize_and_clear(&mut self) -> Vec<u8> {
        let mut bytes = vec![];
        serialize_into(&mut bytes, self).unwrap();
        self.clear();
        bytes
    }
}

#[derive(Debug, Clone)]
pub struct SquashBufferFactory<D> {
    _mark: PhantomData<D>,
}

impl<D> SquashBufferFactory<D> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<D> Default for SquashBufferFactory<D> {
    fn default() -> Self {
        SquashBufferFactory::<D> {
            _mark: PhantomData::default(),
        }
    }
}

impl<D> AbstractSquashBufferFactory for SquashBufferFactory<D>
where
    D: ProperDispatcher + DeserializeOwned + Serialize + 'static,
{
    fn deserialize_from(&self, bytes: &[u8]) -> Box<dyn AbstractSquashBuffer> {
        Box::new(deserialize_from::<&[u8], SquashBuffer<D>>(bytes).unwrap())
    }
    fn new_buffer(&self) -> Box<dyn AbstractSquashBuffer> {
        Box::new(SquashBuffer::<D>::new())
    }
}
impl<D> StaticSquashBufferFactory for SquashBufferFactory<D>
where
    D: ProperDispatcher + DeserializeOwned + Serialize,
{
    fn deserialize_from(bytes: &[u8]) -> Box<dyn AbstractSquashBuffer> {
        Box::new(deserialize_from::<&[u8], SquashBuffer<D>>(bytes).unwrap())
    }
}

pub fn serialize_packed_to_do_avoid_conflict<T, S>(
    v: &SquashedMapValue,
    ser: &mut S,
) -> Result<(), S::Error>
where
    T: Squashable,
    S: SerializeMap,
{
    struct RefTuple<'a, T>
    where
        T: RemoteSend,
    {
        first: &'a T,
        second: &'a Vec<OrderLabel>,
    }

    impl<'a, T> Serialize for RefTuple<'a, T>
    where
        T: RemoteSend,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_tuple(2)?;
            seq.serialize_element::<T>(self.first)?;
            seq.serialize_element::<Vec<OrderLabel>>(self.second)?;
            seq.end()
        }
    }

    let (p, ord) = v;
    let tmp = RefTuple {
        first: p.downcast_ref::<T::Squashed>().unwrap(),
        second: ord,
    };
    ser.serialize_value::<RefTuple<T::Squashed>>(&tmp)
}

pub fn deserialize_entry_to_do_avoid_conflict<'de, T, A>(
    mut access: A,
) -> Result<SquashedMapValue, A::Error>
where
    T: Squashable,
    A: MapAccess<'de>,
{
    let (squashed, orders) = access.next_value::<(T::Squashed, Vec<OrderLabel>)>()?;
    Ok((Box::new(squashed) as PackedValue, orders))
}

fn type_id_to_u64(type_id: TypeId) -> u64 {
    unsafe { std::mem::transmute::<TypeId, u64>(type_id) }
}

fn u64_to_type_id(data: u64) -> TypeId {
    unsafe { std::mem::transmute::<u64, TypeId>(data) }
}

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
    // TODO avoid conflict of S, ACC
    fn dispatch_serialize_entry<S>(
        typeid: TypeId,
        v: &SquashedMapValue,
        ser: &mut S,
    ) -> Result<(), S::Error>
    where
        S: SerializeMap;
    fn dispatch_deserialize_entry<'de, A>(
        typeid: TypeId,
        access: A,
    ) -> Result<SquashedMapValue, A::Error>
    where
        A: MapAccess<'de>;
}

// I have to make these public, since SquashBuffer leaks them
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

    pub fn _clone<T: Send + Clone + 'static>(this: &TaskItem) -> TaskItem {
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
    pub fn _eq<T: Send + PartialEq + 'static>(t: &TaskItem, i: &TaskItem) -> bool {
        let mut ret = t.inner == i.inner && t.squashable.len() == i.squashable.len();
        for index in 0..t.squashable.len() {
            ret = ret && t.squashable[index].1 == i.squashable[index].1;
            ret = ret
                && t.squashable[index].0.downcast_ref::<T>().unwrap()
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
    pub fn test_squash_extract_serialize_desrialize_all() {
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
        // serialize
        let mut bytes = vec![0u8; 0];
        serialize_into(&mut bytes, &buf).unwrap();
        let mut buf: SquashBuffer<ConcreteDispatch> = deserialize_from(&bytes[..]).unwrap();
        // after deserialize
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

    #[test]
    pub fn test_serialzie_squashed_map() {
        let mut map = SquashedMapWrapper::<ConcreteDispatch>::default();

        // aout squash
        let aout = AOut {
            last: 12345,
            diffs: (0..199).collect(),
        };
        let packed_a = Box::new(aout.clone()) as PackedValue;
        let ord_a = (0..100).collect::<Vec<OrderLabel>>();
        let a_type_id = TypeId::of::<A>();
        map.insert(a_type_id, (packed_a, ord_a.clone()));

        // bout squash
        let bout = BOut {
            list: (0..199u8).rev().collect(),
        };
        let packed_b = Box::new(bout.clone()) as PackedValue;
        let ord_b = (0..100).rev().collect::<Vec<OrderLabel>>();
        let b_type_id = TypeId::of::<B>();
        map.insert(b_type_id, (packed_b, ord_b.clone()));

        // serial & deserial
        let mut bytes = vec![0u8; 0];
        serialize_into(&mut bytes, &map).unwrap();
        let mut map1: SquashedMapWrapper<ConcreteDispatch> = deserialize_from(&bytes[..]).unwrap();

        // check aout
        let (packed_a1, ord_a1) = map1.remove(&a_type_id).unwrap();
        assert_eq!(
            *packed_a1
                .downcast_ref::<<A as Squashable>::Squashed>()
                .unwrap(),
            aout
        );
        assert_eq!(ord_a1, ord_a);

        // check bout
        let (packed_b1, ord_b1) = map1.remove(&b_type_id).unwrap();
        assert_eq!(
            *packed_b1
                .downcast_ref::<<B as Squashable>::Squashed>()
                .unwrap(),
            bout
        );
        assert_eq!(ord_b1, ord_b);
    }

    #[test]
    pub fn test_buffer_factory() {
        let f = Box::new(SquashBufferFactory::<ConcreteDispatch>::new());
        let f = f as Box<dyn AbstractSquashBufferFactory>;
        let buffer = f.new_buffer();
        assert_eq!(buffer.len(), 0);
    }
}
