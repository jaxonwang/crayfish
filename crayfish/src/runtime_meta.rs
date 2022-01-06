use crate::activity::FunctionLabel;
use crate::activity::TaskItem;
use crate::activity::SquashTypeHelper;
use crate::activity::HelperMap;
use crate::activity::HelperByType;
use crate::args::RemoteSend;
use crate::activity::set_helpers;
use futures::future::BoxFuture;
use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::sync::Mutex;
use std::any::TypeId;

extern crate futures;
pub extern crate inventory;

pub type DispatchFunction = fn(TaskItem) -> BoxFuture<'static, ()>;

#[derive(Clone, Debug)]
pub struct FunctionMetaData {
    fn_id: FunctionLabel,
    fn_ptr: DispatchFunction,
    fn_name: String,
    file: String,
    line: u32,
    mod_path: String,
}
impl FunctionMetaData {
    pub fn new(
        fn_id: FunctionLabel,
        fn_ptr: DispatchFunction,
        fn_name: String,
        file: String,
        line: u32,
        mod_path: String,
    ) -> Self {
        FunctionMetaData {
            fn_id,
            fn_ptr,
            fn_name,
            file,
            line,
            mod_path,
        }
    }

    pub fn call(&self, item: TaskItem) -> BoxFuture<'static, ()> {
        (self.fn_ptr)(item)
    }
}

inventory::collect!(FunctionMetaData);

type FuncMetaTable = FxHashMap<FunctionLabel, FunctionMetaData>;
static STATIC_FUNC_META_TABLE: Lazy<Mutex<FuncMetaTable>> =
    Lazy::new(|| Mutex::new(FuncMetaTable::default()));

pub(crate) fn init_func_table() {
    let mut m = STATIC_FUNC_META_TABLE.lock().unwrap();
    for func_data in inventory::iter::<FunctionMetaData> {
        let exist = m.insert(func_data.fn_id, func_data.clone());
        if let Some(origin) = exist {
            panic!("Found function id conflict!
            The hash of following function metadata are the same: {:?} {:?}", *func_data, origin)
        }
    }
}

pub(crate) fn get_func_table() -> &'static FuncMetaTable {
    thread_local! {
        static FUNC_META_TABLE: Cell<Option<FuncMetaTable>> = Cell::new(None);
    }
    FUNC_META_TABLE.with(|s| {
        let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };
        if let Some(s_ref) = maybe_ref.as_ref() {
            s_ref
        } else {
            let cloned = STATIC_FUNC_META_TABLE.lock().unwrap().clone();
            s.set(Some(cloned));
            get_func_table()
        }
    })
}

#[derive(Clone)]
pub struct SquashHelperMeta{
    type_id: TypeId,
    helper: Box<dyn SquashTypeHelper + Send>,
}

impl SquashHelperMeta{
    pub fn new<T:RemoteSend>() -> Self {
        SquashHelperMeta{
            type_id: TypeId::of::<T>(),
            helper: Box::new(HelperByType::<T>::default())
        }
    }
}

inventory::collect!(SquashHelperMeta);

pub(crate) fn init_helpers(){
    let mut helpers = HelperMap::default();
    for h in inventory::iter::<SquashHelperMeta> {
        helpers.insert(h.type_id, h.helper.clone());
    }
    set_helpers(helpers);
}

// This mod is trying to do something like "check types at compiling time"
// Dark Magic: Just register all type of squashable, and manually create dyn trait at runtime
// If rust support specialization someday, will remove this.
//
// #[repr(C)]
// #[derive(Debug, Clone)]
// struct TypeMetaInfo {
//     type_id: TypeId,   // type id of squashable
//     vtable: *const (), // vtable of suqashble as squashobject
// }
//
// unsafe impl Send for TypeMetaInfo {}
//
// #[repr(C)]
// #[derive(Debug)]
// pub struct TraitObject {
//     pub data: *mut (),
//     pub vtable: *const (),
// }
//
// #[repr(C)]
// union PtrRepr<T: ?Sized> {
//     const_ptr: *const T,
//     mut_ptr: *mut T,
//     trait_obj: TraitObject,
// }
//
// impl Copy for TraitObject {}
//
// // Manual impl needed to avoid `T: Clone` bound.
// impl Clone for TraitObject {
//     fn clone(&self) -> Self {
//         *self
//     }
// }
//
// // type table for squashable types
// type TypeMetaInfoTable = FxHashMap<TypeId, TypeMetaInfo>;
// static STATIC_META_TABLE: Lazy<Mutex<TypeMetaInfoTable>> =
//     Lazy::new(|| Mutex::new(TypeMetaInfoTable::default()));
//
// fn get_meta_table() -> &'static TypeMetaInfoTable {
//     thread_local! {
//         static META_TABLE: Cell<Option<TypeMetaInfoTable>> = Cell::new(None);
//     }
//     META_TABLE.with(|s| {
//         let maybe_ref: &Option<_> = unsafe { &*s.as_ptr() };
//         if let Some(s_ref) = maybe_ref.as_ref() {
//             s_ref
//         } else {
//             let cloned = STATIC_META_TABLE.lock().unwrap().clone();
//             s.set(Some(cloned));
//             get_meta_table()
//         }
//     })
// }
//
// impl TypeMetaInfo {
//     fn new<T>() -> Self
//     where
//         T: RemoteSend + Any + 'static,
//     {
//         let fake_value = mem::MaybeUninit::<T>::uninit();
//         let fake_value = unsafe { fake_value.assume_init() };
//         let fake_ref = &fake_value as &dyn SquashableObject;
//         TypeMetaInfo {
//             type_id: TypeId::of::<T>(),
//             vtable: Self::trait_object::<dyn SquashableObject>(fake_ref).vtable,
//         }
//     }
//     fn trait_object<T: ?Sized>(ptr: &T) -> TraitObject {
//         unsafe {
//             PtrRepr {
//                 const_ptr: ptr as *const T,
//             }
//             .trait_obj
//         }
//     }
// }
//
// pub fn register_squashable<T>()
// where
//     T: RemoteSend + Any + 'static,
// {
//     let meta_info = TypeMetaInfo::new::<T>();
//     STATIC_META_TABLE
//         .lock()
//         .unwrap()
//         .insert(TypeId::of::<T>(), meta_info);
// }
//
// pub fn try_cast_squashable_object<T>(t: T) -> Result<Box<dyn SquashableObject>, T>
// where
//     T: Any + 'static,
// {
//     if let Some(meta_info) = get_meta_table().get(&TypeId::of::<T>()) {
//         unsafe {
//             let vtable = meta_info.vtable;
//             let b = Box::new(t);
//             let data = Box::into_raw(b) as *mut ();
//             let trait_ptr = PtrRepr::<dyn SquashableObject> {
//                 trait_obj: TraitObject { data, vtable },
//             }
//             .mut_ptr;
//             Ok(Box::from_raw(trait_ptr))
//         }
//     } else {
//         Err(t)
//     }
// }
//
// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::activity::test::A;
//     use crate::activity::test::B;
//
//     // WARN: only one test case will touch the static data
//     fn clean_and_set_meta_info() {
//         *STATIC_META_TABLE.lock().unwrap() = TypeMetaInfoTable::default();
//         register_squashable::<A>();
//         register_squashable::<B>();
//     }
//
//     use crate::activity::downcast_squashable;
//
//     fn test_cast_of<T: RemoteSend + std::fmt::Debug + Clone>(list: Vec<Box<T>>) {
//         let mut a_obj_list = vec![];
//         for a in list.iter().cloned() {
//             let result = try_cast_squashable_object(*a);
//             assert!(result.is_ok());
//             a_obj_list.push(result.unwrap());
//         }
//
//         // squash
//         let mut default_aout = a_obj_list[0].default_squashed();
//         for a in a_obj_list.iter() {
//             a.fold(&mut default_aout)
//         }
//         // inflate
//         let mut inflated = vec![];
//         while let Some(a) = default_aout.extract() {
//             inflated.push(a)
//         }
//         let inflated: Vec<_> = inflated
//             .into_iter()
//             .rev()
//             .map(|a| downcast_squashable::<T>(a).unwrap())
//             .collect();
//         assert_eq!(inflated, list);
//     }
//
//     #[test]
//     pub fn test_cast() {
//         clean_and_set_meta_info();
//
//         let num1 = 1usize;
//         assert_eq!(try_cast_squashable_object(num1).err().unwrap(), num1);
//
//         let a_list: Vec<_> = (0..128).map(|i| Box::new(A { value: i })).collect();
//         test_cast_of(a_list);
//
//         let b_list: Vec<_> = (0..128).map(|i| Box::new(B { value: i })).collect();
//         test_cast_of(b_list);
//     }
// }
