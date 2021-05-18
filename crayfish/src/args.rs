use serde::de::DeserializeOwned;
use serde::Serialize;
use std::any::Any;

extern crate serde;

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
pub trait RemoteSend: Serialize + DeserializeOwned + Any + Send + 'static {
    // squash type not necessarily to be serde
    type Output: Squashed;
    /// folding is from left to right
    fn fold(&self, acc: &mut Self::Output);
    /// folding is from right to left
    fn extract(out: &mut Self::Output) -> Option<Self>
    where
        Self: Sized;
    fn reorder(&self, other: &Self) -> std::cmp::Ordering;
    fn is_squashable() -> bool {
        true
    }
}

pub trait Squashed: Serialize + DeserializeOwned + Send + Default {}
impl<T> Squashed for T where T: Serialize + DeserializeOwned + Send + Default {}

#[macro_export]
macro_rules! impl_body {
    () => {
        type Output = ();
        fn is_squashable() -> ::std::primitive::bool {
            false
        }
        fn fold(&self, _acc: &mut Self::Output) {
            panic!()
        }
        fn extract(_out: &mut Self::Output) -> ::std::option::Option<Self>
        where
            Self: Sized,
        {
            panic!()
        }
        fn reorder(&self, _other: &Self) -> ::std::cmp::Ordering {
            panic!()
        }
    };
}

macro_rules! impl_type {
    ($type_name:ty) => {
        impl RemoteSend for $type_name {
            impl_body! {}
        }
    };
}

macro_rules! impl_types {
    ($($type_name:ty) * ) => {
        $(impl_type!{$type_name})*
    }
}

// copy from serde
macro_rules! impl_generic_type {
    ($ty:ident < T $(: $tbound1:ident $(+ $tbound2:ident)*)* $(, $typaram:ident : $bound:ident)* >) => {
        impl<T $(, $typaram)*> RemoteSend for $ty<T $(, $typaram)*>
        where
            T: RemoteSend $(+ $tbound1 $(+ $tbound2)*)*,
            $($typaram: $bound + Send + 'static,)*
        {
            impl_body!{}
        }
    }
}

macro_rules! impl_arrays{
    ( $($len:tt)+ ) => {
        $(
            impl <T> RemoteSend for [T; $len]
            where
                T: RemoteSend
            {
                impl_body!{}
            }
        )+
    }
}

macro_rules! impl_tuple{
    ( $($idx:ident),+ ) => {
        impl <$($idx),+> RemoteSend for ( $($idx,)+ )
        where
            $(
                $idx:RemoteSend
            ),+
        {
            impl_body!{}
        }
    }
}

macro_rules! impl_tuples{
    () => {};
    ($first:ident$(,$tn: ident)* ) => {
        impl_tuple!{ $first $(,$tn)* }
        impl_tuples!{ $($tn),* }
    }
}

impl_tuples! { T0,T1,T2,T3,T4,T5,T6,T7,T8,T9,T10,T11,T12,T13,T14,T15 }

use std::collections::*;
use std::ffi::*;
use std::hash::*;
use std::marker::PhantomData;
use std::net::*;
use std::num::*;
use std::ops::*;
use std::path::*;
use std::sync::atomic::*;
use std::time::*;

impl<T: ?Sized + Send + 'static> RemoteSend for PhantomData<T> {
    impl_body! {}
}

impl<T, H> RemoteSend for HashSet<T, H>
where
    T: RemoteSend + Eq + Hash,
    H: BuildHasher + Send + 'static + Default, // TODO: Why compiler ask that to be default?
{
    impl_body! {}
}

impl<K, V, H> RemoteSend for HashMap<K, V, H>
where
    K: RemoteSend + Eq + Hash,
    V: RemoteSend,
    H: BuildHasher + Send + 'static + Default, // TODO: Why compiler ask that to be default?
{
    impl_body! {}
}

impl<K, V> RemoteSend for BTreeMap<K, V>
where
    K: RemoteSend + Ord,
    V: RemoteSend,
{
    impl_body! {}
}

impl<T, E> RemoteSend for Result<T, E>
where
    T: RemoteSend,
    E: RemoteSend,
{
    impl_body! {}
}

// TODO: add test cases
impl_generic_type! {Vec<T>}
impl_generic_type! {Option<T>}
impl_generic_type! {BinaryHeap<T: Ord>}
impl_generic_type! {BTreeSet<T: Ord>}
impl_generic_type! {LinkedList<T>}
impl_generic_type! {VecDeque<T>}
impl_generic_type! {Range<T>}
impl_generic_type! {RangeInclusive<T>}
impl_generic_type! {Bound<T>}
impl_generic_type! {Box<T>}
// NOTE: Currently not support ref count

use std::cell::Cell;
use std::cell::RefCell;
impl_generic_type! {Cell<T:Copy>}
impl_generic_type! {RefCell<T>}

use std::sync::Mutex;
use std::sync::RwLock;
impl_generic_type! {Mutex<T>}
impl_generic_type! {RwLock<T>}

impl_generic_type! {Wrapping<T>}
use std::cmp::Reverse;
impl_generic_type! {Reverse<T>}

impl_arrays! {
    01 02 03 04 05 06 07 08 09 10
    11 12 13 14 15 16 17 18 19 20
    21 22 23 24 25 26 27 28 29 30
    31 32
}

impl_types! {
    () bool
    isize i8 i16 i32 i64 i128
    usize u8 u16 u32 u64 u128
    f32 f64 char
    String
    CString OsString
    NonZeroU8 NonZeroU16 NonZeroU32 NonZeroU64 NonZeroU128 NonZeroUsize
    NonZeroI8 NonZeroI16 NonZeroI32 NonZeroI64 NonZeroI128 NonZeroIsize
    Duration SystemTime
    IpAddr Ipv4Addr Ipv6Addr SocketAddr SocketAddrV4 SocketAddrV6
    PathBuf
    AtomicBool
    AtomicI8 AtomicI16 AtomicI32 AtomicIsize
    AtomicU8 AtomicU16 AtomicU32 AtomicUsize
}
