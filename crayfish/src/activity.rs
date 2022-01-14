use crate::args::RemoteSend;
pub use crate::global_id::ActivityId;
use crate::place::Place;
use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
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
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Mutex;

extern crate serde;

pub type PanicPayload = String;
pub type FunctionLabel = u64; // function label is line
pub type ActivityResult = std::result::Result<(), PanicPayload>;

fn cast_panic_payload(payload: Box<dyn Any + Send + 'static>) -> PanicPayload {
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

pub(crate) trait SquashableObject: Any + Send + 'static {
    // for downcast
    fn fold(&self, v: &mut SBox);
    fn default_squashed(&self) -> SBox;
}

impl<T> SquashableObject for T
where
    T: RemoteSend,
{
    fn fold(&self, v: &mut SBox) {
        self.fold(&mut v.downcast_mut::<SquashedHolder<T>>().unwrap().squashed);
    }
    fn default_squashed(&self) -> SBox {
        Box::new(SquashedHolder {
            squashed: T::Output::default(),
            _mark: PhantomData::<T>::default(),
        })
    }
}

impl dyn SquashableObject + Send {
    fn downcast_ref<T: Any>(&self) -> Option<&T> {
        // TODO remove that if if we are sure code we generated is correct
        if (*self).type_id() == TypeId::of::<T>() {
            unsafe { Some(&*(self as *const dyn SquashableObject as *const T)) }
        } else {
            None
        }
    }
}

pub(crate) fn downcast_squashable<T: Any>(b: SoBox) -> Result<Box<T>, SoBox> {
    if (*b).type_id() == TypeId::of::<T>() {
        unsafe {
            let raw: *mut dyn SquashableObject = Box::into_raw(b);
            Ok(Box::from_raw(raw as *mut T))
        }
    } else {
        Err(b)
    }
}

impl fmt::Debug for dyn SquashableObject + Send {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("RemoteSend")
    }
}

struct SquashedHolder<T: RemoteSend> {
    squashed: T::Output,
    _mark: PhantomData<T>,
}

impl<T> Deref for SquashedHolder<T>
where
    T: RemoteSend,
{
    type Target = T::Output;
    fn deref(&self) -> &Self::Target {
        &self.squashed
    }
}

impl<T> DerefMut for SquashedHolder<T>
where
    T: RemoteSend,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.squashed
    }
}

impl<T> SquashedHolder<T>
where
    T: RemoteSend,
{
    fn new(squashed: T::Output) -> Self {
        SquashedHolder {
            squashed,
            _mark: PhantomData::<T>::default(),
        }
    }
    fn new_boxed(squashed: T::Output) -> Box<Self> {
        Box::new(Self::new(squashed))
    }
}

// a magic to implment dyn T::Output where T is not allowed to be trait object
pub(crate) trait SquashedObject: Send + Any {
    fn extract(&mut self) -> Option<SoBox>;
}

impl<T> SquashedObject for SquashedHolder<T>
where
    T: RemoteSend,
{
    fn extract(&mut self) -> Option<SoBox> {
        T::extract(self).map(|a| Box::new(a) as SoBox)
    }
}

impl dyn SquashedObject + Send {
    fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        // TODO remove that if if we are sure code we generated is correct
        if (*self).type_id() == TypeId::of::<T>() {
            unsafe { Some(&mut *(self as *mut dyn SquashedObject as *mut T)) }
        } else {
            None
        }
    }

    fn downcast_ref<T: Any>(&self) -> Option<&T> {
        // TODO remove that if if we are sure code we generated is correct
        if (*self).type_id() == TypeId::of::<T>() {
            unsafe { Some(&*(self as *const dyn SquashedObject as *const T)) }
        } else {
            None
        }
    }
}

impl fmt::Debug for dyn SquashedObject + Send {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Output")
    }
}

// TODO: Auto impl Squash macro by derive, squash
//
/// Lower 8 bits stands for the type of argument
type OrderLabel = u32; // lower 8 bit is for the position in side a function
const ARGUMENT_ORDER_BITS: u32 = 8;

// TODO: should impl serialize
// TODO: arrange squash

type SoBox = Box<dyn SquashableObject + Send>;
type SBox = Box<dyn SquashedObject + Send>;
type SquashableMap = FxHashMap<TypeId, Vec<(SoBox, OrderLabel)>>;
type SquashedMapValue = (SBox, Vec<OrderLabel>);
type SquashedMap = FxHashMap<TypeId, SquashedMapValue>;

// to allow implement serialization for squashedmap
#[derive(Default)]
struct SquashedMapWrapper {
    m: SquashedMap,
}

impl Deref for SquashedMapWrapper {
    type Target = SquashedMap;
    fn deref(&self) -> &Self::Target {
        &self.m
    }
}

impl DerefMut for SquashedMapWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.m
    }
}

type OrderedSquashable = Vec<(SoBox, OrderLabel)>;

pub(crate) struct HelperByType<T: RemoteSend> {
    _mark: PhantomData<T>,
}

impl<T> Default for HelperByType<T>
where
    T: RemoteSend,
{
    fn default() -> Self {
        HelperByType {
            _mark: PhantomData::<T>::default(),
        }
    }
}

impl<T> Clone for HelperByType<T>
where
    T: RemoteSend,
{
    fn clone(&self) -> Self {
        Default::default()
    }
}

impl<T> SquashTypeHelper for HelperByType<T>
where
    T: RemoteSend,
{
    fn serialize(&self, obj: &SBox) -> Vec<u8> {
        let mut ret = vec![];
        serialize_into(
            &mut ret,
            &obj.downcast_ref::<SquashedHolder<T>>().unwrap().squashed,
        )
        .unwrap();
        ret
    }
    fn deserialize(&self, bytes: Vec<u8>) -> SBox {
        // TODO use inplace deserialize to avoid copy
        SquashedHolder::<T>::new_boxed(deserialize_from::<&[u8], T::Output>(&bytes[..]).unwrap())
    }

    fn sort_by(&self, to_arrange: &mut Vec<(SoBox, OrderLabel)>) {
        to_arrange.sort_unstable_by(|a, b| {
            a.0.downcast_ref::<T>()
                .unwrap()
                .reorder(b.0.downcast_ref::<T>().unwrap())
        });
    }
    // TODO: I don't know why remove this send wound not compile, SquashTypeHeper is Send!
    fn clone_boxed(&self) -> Box<dyn SquashTypeHelper + Send> {
        Box::new(self.clone())
    }
}

pub(crate) trait SquashTypeHelper: Send {
    fn serialize(&self, obj: &SBox) -> Vec<u8>;
    fn deserialize(&self, bytes: Vec<u8>) -> SBox;
    fn sort_by(&self, arrange: &mut Vec<(SoBox, OrderLabel)>);
    fn clone_boxed(&self) -> Box<dyn SquashTypeHelper + Send>;
}

impl Clone for Box<dyn SquashTypeHelper + Send> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

impl fmt::Debug for Box<dyn SquashTypeHelper + Send> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("type helper")
    }
}

impl fmt::Debug for dyn SquashTypeHelper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("SquashTypeHelper")
    }
}

pub(crate) type HelperMap = FxHashMap<TypeId, Box<dyn SquashTypeHelper + Send>>;

static SQUASH_HELPERS: Lazy<Mutex<HelperMap>> = Lazy::new(|| Mutex::new(HelperMap::default()));

pub(crate) fn set_helpers(helpers: HelperMap) {
    let mut h = SQUASH_HELPERS.lock().unwrap();
    *h = helpers
}

thread_local! {
    static LOCAL_SQUASH_HELPERS: Cell<Option<HelperMap>> = Cell::new(None);
}

fn get_squash_helper(typeid: TypeId) -> &'static dyn SquashTypeHelper {
    LOCAL_SQUASH_HELPERS.with(|s| loop {
        let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };
        if let Some(s_ref) = maybe_ref.as_ref() {
            break s_ref.get(&typeid).unwrap().as_ref();
        } else {
            let helpers = SQUASH_HELPERS.lock().unwrap().clone();
            s.set(Some(helpers));
        }
    })
}

impl Serialize for SquashedMapWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        struct RefTuple<'a> {
            first: Vec<u8>,
            second: &'a Vec<OrderLabel>,
        }

        impl<'a> Serialize for RefTuple<'a> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element::<Vec<u8>>(&self.first)?;
                seq.serialize_element::<Vec<OrderLabel>>(self.second)?;
                seq.end()
            }
        }
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (id, v) in self.iter() {
            map.serialize_key(&type_id_to_u64(*id)).unwrap();
            let helper = get_squash_helper(*id);
            let (squashed, ord) = v;
            let t = RefTuple {
                first: helper.serialize(squashed),
                second: ord,
            };
            map.serialize_value(&t).unwrap()
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SquashedMapWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueMapVisitor {}

        impl<'de> Visitor<'de> for ValueMapVisitor {
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
                    let (p, ord) = access.next_value::<(Vec<u8>, Vec<OrderLabel>)>()?;
                    let helper = get_squash_helper(type_id);
                    let squashed = helper.deserialize(p);
                    map.insert(type_id, (squashed, ord));
                }

                Ok(map)
            }
        }

        let m = deserializer.deserialize_map(ValueMapVisitor {})?;

        Ok(SquashedMapWrapper { m })
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

#[derive(Serialize, Deserialize, Default)]
struct SquashBuffer {
    // TODO boxex
    items: Vec<StrippedTaskItem>,
    #[serde(skip)]
    squashable_map: SquashableMap, // type id of squashable, must not contain empty vector
    squashed_map: SquashedMapWrapper, // type id of squashed, ditto
    #[serde(skip)]
    ordered_squashable: OrderedSquashable, // still preserve label, for I don't want extra squash item struct
}

impl SquashBuffer {
    pub fn new() -> Self {
        Self::default()
    }
}
impl AbstractSquashBuffer for SquashBuffer {
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
            squash_one_type(typeid, &mut self.squashable_map, &mut self.squashed_map);
        }
    }

    fn extract_all(&mut self) {
        let typeids: Vec<_> = self.squashed_map.keys().cloned().collect();
        for typeid in typeids {
            extract_one_type(typeid, &mut self.squashed_map, &mut self.ordered_squashable);
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

// TODO remove such abstraction
#[derive(Debug, Clone, Default)]
pub struct SquashBufferFactory {}

impl SquashBufferFactory {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AbstractSquashBufferFactory for SquashBufferFactory {
    fn deserialize_from(&self, bytes: &[u8]) -> Box<dyn AbstractSquashBuffer> {
        Box::new(deserialize_from::<&[u8], SquashBuffer>(bytes).unwrap())
    }
    fn new_buffer(&self) -> Box<dyn AbstractSquashBuffer> {
        Box::new(SquashBuffer::new())
    }
}
impl StaticSquashBufferFactory for SquashBufferFactory {
    fn deserialize_from(bytes: &[u8]) -> Box<dyn AbstractSquashBuffer> {
        Box::new(deserialize_from::<&[u8], SquashBuffer>(bytes).unwrap())
    }
}

fn type_id_to_u64(type_id: TypeId) -> u64 {
    unsafe { std::mem::transmute::<TypeId, u64>(type_id) }
}

fn u64_to_type_id(data: u64) -> TypeId {
    unsafe { std::mem::transmute::<u64, TypeId>(data) }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ReturnInfo {
    result: ActivityResult,
    sub_activities: Vec<ActivityId>,
}

#[derive(Default, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct StrippedTaskItem {
    fn_id: FunctionLabel,
    place: Place, // it's dst place, request place or return place
    activity_id: ActivityId,
    ret: Option<ReturnInfo>,
    waited: bool, // indicating this activity is waited on the spawned place
    args: Vec<u8>,
}

impl fmt::Debug for StrippedTaskItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stripped")
            .field("fn_id", &self.fn_id)
            .field("place", &self.place)
            .field("activity_id", &self.activity_id)
            .field("ret", &self.ret)
            .field("waited", &self.waited)
            .finish()
    }
}

#[derive(Default)]
pub struct TaskItem {
    inner: StrippedTaskItem,
    squashable: Vec<(SoBox, OrderLabel)>,
}

impl fmt::Debug for TaskItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskItem")
            .field("inner", &self.inner)
            .field("squashable_len", &self.squashable.len())
            .finish()
    }
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
    pub fn function_id(&self) -> FunctionLabel {
        self.inner.fn_id
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
        if T::is_squashable() {
            *downcast_squashable::<T>(self.item.squashable.pop().unwrap().0).unwrap()
        } else {
            let mut read = &self.item.inner.args[self.position..];
            let t = deserialize_from(&mut read).expect("Failed to deserialize function argument");
            self.position =
                unsafe { read.as_ptr().offset_from(self.item.inner.args.as_ptr()) as usize };
            t
        }
    }
    pub fn ret<T: RemoteSend>(&mut self) -> Result<T, PanicPayload> {
        let ret_info = self.item.inner.ret.take().unwrap();
        match ret_info.result {
            Ok(()) => Ok(self.arg::<T>()),
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
        std::mem::take(&mut self.item.inner.ret.as_mut().unwrap().sub_activities)
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

    pub fn arg<T: RemoteSend>(&mut self, t: T) {
        if T::is_squashable() {
            let label = self.next_label();
            self.item.squashable.push((Box::new(t), label));
        } else {
            serialize_into(&mut self.item.inner.args, &t)
                .expect("Failed to serialize function argument");
            self.next_label();
        }
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
    pub fn ret<T: RemoteSend>(&mut self, result: std::thread::Result<T>) {
        let result = match result {
            Ok(ret) => {
                self.arg::<T>(ret);
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

fn squash_one_type(
    typeid: TypeId,
    squashable_map: &mut SquashableMap,
    squashed_map: &mut SquashedMap,
) {
    let to_squash = squashable_map.remove(&typeid).unwrap();
    let squashed_with_order = squash(typeid, to_squash);
    squashed_map.insert(typeid, squashed_with_order);
}

fn squash(typeid: TypeId, to_squash: Vec<(SoBox, OrderLabel)>) -> (SBox, Vec<OrderLabel>) {
    let mut to_squash = to_squash;
    let helper = get_squash_helper(typeid);
    helper.sort_by(&mut to_squash);
    debug_assert!(!to_squash.is_empty());
    let mut squashed = to_squash[0].0.default_squashed();
    let mut labels = Vec::<_>::with_capacity(to_squash.len());
    for (s, order) in to_squash {
        s.fold(&mut squashed);
        labels.push(order);
    }
    (squashed, labels)
}

fn extract_one_type(
    typeid: TypeId,
    squashed_map: &mut SquashedMap,
    ordered_squashable: &mut OrderedSquashable,
) {
    let to_inflate = squashed_map.remove(&typeid).unwrap();
    extract_into(to_inflate, ordered_squashable);
}

fn extract_into(to_inflate: (SBox, Vec<OrderLabel>), to: &mut Vec<(SoBox, OrderLabel)>) {
    let (mut squashed, labels) = to_inflate;
    let mut count = 0;
    while let Some(s) = squashed.extract() {
        to.push((s, labels[labels.len() - count - 1]));
        count += 1;
    }
    debug_assert_eq!(count, labels.len());
}

#[cfg(test)]
pub mod test {
    use super::*;
    use rand::prelude::*;
    use rand::seq::SliceRandom;
    use std::cmp::Ordering;
    use std::convert::TryInto;
    use std::sync::MutexGuard;

    fn clone_squashable_orderlabel_list<T: RemoteSend + Clone>(
        v: &[(SoBox, OrderLabel)],
    ) -> Vec<(SoBox, OrderLabel)> {
        let cbox = |v: &(SoBox, OrderLabel)| {
            let (a, b) = v;
            let a: T = (*a.downcast_ref::<T>().unwrap()).clone();
            (Box::new(a) as SoBox, *b)
        };
        v.iter().map(cbox).collect()
    }

    pub fn _clone<T: RemoteSend + Clone>(this: &TaskItem) -> TaskItem {
        TaskItem {
            inner: this.inner.clone(),
            squashable: clone_squashable_orderlabel_list::<T>(&this.squashable[..]),
        }
    }
    pub fn _eq<T: RemoteSend + PartialEq>(t: &TaskItem, i: &TaskItem) -> bool {
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
                TaskItem {
                    inner: self.inner.clone(),
                    squashable: vec![],
                }
            }
        }
        impl Eq for TaskItem {}
        impl PartialEq for TaskItem {
            fn eq(&self, i: &Self) -> bool {
                self.inner == i.inner
            }
        }
    }
    use rand::distributions::Alphanumeric;
    #[test]
    pub fn test_squashbuffer_push_pop() {
        let mut buf = SquashBuffer::new();
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

    #[derive(Serialize, Deserialize)]
    struct R {
        a: A,
        b: B,
        c: i32,
    }

    static TEST_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

    fn set_helpers_for_a_b() {
        let mut helpers = HelperMap::default();
        helpers.insert(TypeId::of::<B>(), Box::new(HelperByType::<B>::default()));
        helpers.insert(TypeId::of::<A>(), Box::new(HelperByType::<A>::default()));
        set_helpers(helpers);
    }
    pub struct TestGuardForStatic<'a> {
        _guard: MutexGuard<'a, bool>,
    }

    impl<'a> TestGuardForStatic<'a> {
        pub fn new() -> Self {
            let ret = TestGuardForStatic {
                // must get test lock first
                _guard: TEST_LOCK.lock().unwrap(),
            };
            set_helpers_for_a_b();
            ret
        }
    }

    #[test]
    pub fn test_squash_extract() {
        let _ = TestGuardForStatic::new();
        let mut rng = thread_rng();

        let mut a_list: Vec<_> = (1..65).map(|value| Box::new(A { value })).collect();
        let a_len = a_list.len();
        a_list.shuffle(&mut rng);
        let mut a_list: Vec<_> = a_list
            .into_iter()
            .zip(0..a_len)
            .map(|(a, b)| (a, b as OrderLabel))
            .collect(); // assign order

        let a_list_clone = a_list
            .clone()
            .into_iter()
            .map(|(a, b)| (a as SoBox, b))
            .collect();
        let squashed = squash(TypeId::of::<A>(), a_list_clone);
        let mut extracted = vec![];
        extract_into(squashed, &mut extracted);
        let mut extracted: Vec<_> = extracted
            .into_iter()
            .map(|(a, b)| (downcast_squashable::<A>(a).unwrap(), b))
            .collect();
        // there is no order in extract into, so sort here for compare
        extracted.sort_by_key(|(_, b)| *b);
        a_list.sort_by_key(|(_, b)| *b);
        assert_eq!(a_list, extracted);

        let b_list: Vec<_> = (0..64)
            .map(|value| (Box::new(B { value }), value as OrderLabel))
            .collect();
        let mut extracted = vec![];
        let b_list_clone = b_list
            .clone()
            .into_iter()
            .map(|(a, b)| (a as SoBox, b))
            .collect();
        extract_into(squash(TypeId::of::<B>(), b_list_clone), &mut extracted);
        let mut extracted: Vec<_> = extracted
            .into_iter()
            .map(|(b, l)| (downcast_squashable::<B>(b).unwrap(), l))
            .collect();
        extracted.sort_by_key(|(_, b)| *b);
        assert_eq!(b_list, extracted);
    }

    fn crate_squash_item(a: A, b: B, fn_id: FunctionLabel) -> TaskItem {
        let mut rng = thread_rng();
        let mut builder = TaskItemBuilder::new(fn_id, rng.gen(), rng.gen());
        builder.arg(a.clone());
        builder.arg(b.clone());
        builder.arg(a);
        builder.arg(b);
        builder.build()
    }
    fn extract_squash_item(item: TaskItem) -> (A, B, FunctionLabel) {
        assert_eq!(item.squashable.len(), 4);
        let mut e = TaskItemExtracter::new(item);
        let _: A = e.arg();
        let _: B = e.arg();
        let a: A = e.arg();
        let b: B = e.arg();
        let fn_id = e.fn_id();
        (a, b, fn_id)
    }

    #[test]
    pub fn test_squash_extract_all() {
        let _ = TestGuardForStatic::new();
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
        let mut buf = SquashBuffer::new();
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
        let _ = TestGuardForStatic::new();
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
        let mut buf = SquashBuffer::new();
        for i in items {
            buf.push(i)
        }
        buf.squash_all();
        // serialize
        let mut bytes = vec![0u8; 0];
        serialize_into(&mut bytes, &buf).unwrap();
        let mut buf: SquashBuffer = deserialize_from(&bytes[..]).unwrap();
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
        builder.arg(b.clone());
        builder.arg(c);
        builder.arg(d.clone());
        let item = builder.build();
        assert_eq!(item.inner.fn_id, fn_id);
        assert_eq!(item.inner.place, place);
        assert_eq!(item.inner.activity_id, activity_id);
        assert_eq!(item.squashable[0].1, 1);
        assert_eq!(item.squashable[1].1, 3);
        let mut ex = TaskItemExtracter::new(item);
        assert_eq!(a, ex.arg());
        assert_eq!(b, ex.arg());
        assert_eq!(c, ex.arg());
        assert_eq!(d, ex.arg());
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
        builder.ret(result);
        builder.sub_activities(activities.clone());
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.sub_activities(), activities);
        assert_eq!(ex.ret::<A>().unwrap(), A { value: 4577 });

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
        builder.ret(result);
        let mut ex = TaskItemExtracter::new(builder.build());
        assert_eq!(ex.ret::<A>().unwrap_err(), msg);
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
        #[allow(non_fmt_panics)]
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
        let _ = TestGuardForStatic::new();
        let mut map = SquashedMapWrapper::default();

        // aout squash
        let aout = AOut {
            last: 12345,
            diffs: (0..199).collect(),
        };
        let packed_a = SquashedHolder::<A>::new_boxed(aout.clone()) as SBox;
        let ord_a = (0..100).collect::<Vec<OrderLabel>>();
        let a_type_id = TypeId::of::<A>();
        map.insert(a_type_id, (packed_a, ord_a.clone()));

        // bout squash
        let bout = BOut {
            list: (0..199u8).rev().collect(),
        };
        let packed_b = SquashedHolder::<B>::new_boxed(bout.clone()) as SBox;
        let ord_b = (0..100).rev().collect::<Vec<OrderLabel>>();
        let b_type_id = TypeId::of::<B>();
        map.insert(b_type_id, (packed_b, ord_b.clone()));

        // serial & deserial
        let mut bytes = vec![0u8; 0];
        serialize_into(&mut bytes, &map).unwrap();
        let mut map1: SquashedMapWrapper = deserialize_from(&bytes[..]).unwrap();

        // check aout
        let (packed_a1, ord_a1) = map1.remove(&a_type_id).unwrap();
        assert_eq!(
            packed_a1
                .downcast_ref::<SquashedHolder<A>>()
                .unwrap()
                .squashed,
            aout
        );
        assert_eq!(ord_a1, ord_a);

        // check bout
        let (packed_b1, ord_b1) = map1.remove(&b_type_id).unwrap();
        assert_eq!(
            packed_b1
                .downcast_ref::<SquashedHolder<B>>()
                .unwrap()
                .squashed,
            bout
        );
        assert_eq!(ord_b1, ord_b);
    }

    #[test]
    pub fn test_buffer_factory() {
        let f = Box::new(SquashBufferFactory::new());
        let f = f as Box<dyn AbstractSquashBufferFactory>;
        let buffer = f.new_buffer();
        assert_eq!(buffer.len(), 0);
    }
}
