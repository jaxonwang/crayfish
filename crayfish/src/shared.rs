use crate::args::RemoteSend;
use once_cell::sync::Lazy;
use parking_lot::const_rwlock;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde::Serialize;
use std::any::Any;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

type HandleID = usize;

#[derive(Default)]
struct ClipBoard {
    next_id: HandleID,
    records: FxHashMap<HandleID, Box<dyn Any + Send + Sync>>,
}

static CLIP_BOARD: Lazy<RwLock<ClipBoard>> = Lazy::new(|| const_rwlock(ClipBoard::default()));

// it is fine to clone. The first to drop will clean the record in clipboard
#[derive(Clone)]
pub struct PlaceLocal<T: ?Sized> {
    id: HandleID,
    value: Arc<T>,
}

// it is reasonable to be 'static since T might live arbitrarily long.
// T must be Sync and Send since accessed in different threads.
impl<T> PlaceLocal<T>
where
    T: 'static + Sync + Send,
{
    pub fn new(val: T) -> Self {
        let mut h = CLIP_BOARD.write();
        let id = h.next_id;
        let pl = PlaceLocal {
            id,
            value: Arc::new(val),
        };
        let record = Box::new(pl.value.clone());
        h.records.insert(pl.id, record);
        h.next_id += 1;
        pl
    }

    pub fn downgrade(&self) -> PlaceLocalWeak<T> {
        PlaceLocalWeak::<T>::new(self.id)
    }
}

impl<T: ?Sized> Deref for PlaceLocal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> Drop for PlaceLocal<T>
where
    T: ?Sized,
{
    fn drop(&mut self) {
        let mut h = CLIP_BOARD.write();
        h.records.remove(&self.id);
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct PlaceLocalWeak<T: ?Sized> {
    id: HandleID,
    _mark: PhantomData<T>,
}

impl<T> Clone for PlaceLocalWeak<T>
where
    T: ?Sized,
{
    fn clone(&self) -> Self {
        PlaceLocalWeak {
            id: self.id,
            _mark: PhantomData,
        }
    }
}

// TODO remove that, when https://github.com/jaxonwang/rust-apgas/issues/18 solved
impl<T: ?Sized + Send + 'static> RemoteSend for PlaceLocalWeak<T> {
    crate::impl_body! {}
}

impl<T: 'static> PlaceLocalWeak<T> {
    fn new(id: HandleID) -> Self {
        PlaceLocalWeak {
            id,
            _mark: PhantomData,
        }
    }

    pub fn upgrade(&self) -> Option<Arc<T>> {
        let h = CLIP_BOARD.read();
        h.records
            .get(&self.id)
            .map(|b| b.downcast_ref::<Arc<T>>().unwrap().clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    #[test]
    pub fn test_place_local() {
        let ptrs = {
            let value = 12345;
            let pl = PlaceLocal::new(value);
            let weak_ptrs = (0..16).map(|_| pl.downgrade());
            let weak_ptrs_invalid: Vec<_> = (0..16).map(|_| pl.downgrade()).collect();

            let threads: Vec<_> = weak_ptrs
                .map(|ptr| {
                    thread::spawn(move || match ptr.upgrade() {
                        Some(p) => assert_eq!(*p, value),
                        None => (),
                    })
                })
                .collect();

            threads.into_iter().for_each(|t| t.join().unwrap());
            weak_ptrs_invalid
            // now pl is destroyed. return ptrs should be invalid.
        };
        ptrs.into_iter()
            .for_each(|p| assert!(p.upgrade().is_none()));
    }
}
