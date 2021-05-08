use crate::network::Rank;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use std::cell::Cell;
use std::convert::TryInto;
use std::sync::Mutex;
extern crate once_cell;

pub type Place = u16;
pub type WorkerId = u16;
pub type FinishId = u64;
pub type ActivityId = u128;
pub type ActivityIdLower = u64;
type FinishLocalId = u32;
type ActivityLocalId = u32;

pub trait FinishIdMethods {
    fn get_place(&self) -> Place;
}

impl FinishIdMethods for FinishId {
    fn get_place(&self) -> Place {
        (*self >> (16 + 32)) as Place
    }
}

pub trait ActivityIdMethods {
    fn get_spawned_place(&self) -> Place;
    fn get_finish_id(&self) -> FinishId;
    fn update_finish_id(&self, fid: FinishId) -> ActivityId;
    fn get_lower(&self) -> ActivityIdLower;
}

impl ActivityIdMethods for ActivityId {
    fn get_spawned_place(&self) -> Place {
        (*self >> (32 + 16) & ((1 << 16) - 1)) as Place
    }
    fn get_finish_id(&self) -> FinishId {
        (*self >> 64) as FinishId
    }
    fn update_finish_id(&self, fid: FinishId) -> ActivityId {
        (fid as ActivityId) << 64 | (self & ((1 << 64) - 1))
    }
    fn get_lower(&self) -> ActivityIdLower {
        (*self & ((1 << 64) - 1)) as ActivityIdLower
    }
}

// TODO move these world info to another mod
static HERE_STATIC: OnceCell<Place> = OnceCell::new();
static WORLD_SIZE_STATIC: OnceCell<usize> = OnceCell::new();
// two level mutex here, but I think the code is clear
static NEXT_WORKER_ID: Lazy<Mutex<WorkerId>> = Lazy::new(|| Mutex::new(0));

thread_local! {
    static HERE_LOCAL: Cell<Option<Place>> = Cell::new(None);
    static WORLD_SIZE: Cell<Option<usize>> = Cell::new(None);
    static WORKER_ID: Cell<Option<WorkerId>> = Cell::new(None);
    static NEXT_FINISH_LOCAL_ID: Cell<FinishLocalId> = Cell::new(0); // start from 1
    static NEXT_ACTIVITY_LOCAL_ID: Cell<ActivityLocalId> = Cell::new(0); // start from 1
}

pub(crate) fn init_here(here: Rank) {
    HERE_STATIC.set(here.as_i32().try_into().unwrap()).unwrap();
}

pub(crate) fn init_world_size(size: usize){
    WORLD_SIZE_STATIC.set(size).unwrap();
}

pub fn here() -> Place {
    HERE_LOCAL.with(|h| match h.get() {
        Some(p) => p,
        None => {
            let p = HERE_STATIC.get().expect("here place id is not initialized");
            h.set(Some(*p));
            here()
        }
    })
}

pub fn world_size() -> usize{
    WORLD_SIZE.with(|h| match h.get() {
        Some(p) => p,
        None => {
            let p = WORLD_SIZE_STATIC.get().expect("world size is not initialized");
            h.set(Some(*p));
            world_size()
        }
    })
}

pub(crate) fn my_worker_id() -> WorkerId {
    WORKER_ID.with(|wid| {
        match wid.get() {
            Some(w_id) => w_id,
            None => {
                let mut w_id_old = NEXT_WORKER_ID.lock().unwrap();
                wid.set(Some(*w_id_old));
                *w_id_old = w_id_old.checked_add(1).unwrap(); // force overflow panic
                my_worker_id()
            }
        }
    })
}

fn next_finish_local_id() -> FinishLocalId {
    NEXT_FINISH_LOCAL_ID.with(|fid| {
        let old = fid.get();
        let new = old.checked_add(1).expect("Finish id overflows");
        fid.set(new);
        new
    })
}

fn next_activity_local_id() -> ActivityLocalId {
    NEXT_ACTIVITY_LOCAL_ID.with(|aid| {
        let old = aid.get();
        let new = old.checked_add(1).expect("Activity id overflows");
        aid.set(new);
        new
    })
}

pub(crate) fn new_global_finish_id() -> FinishId {
    let prefix = ((here() as FinishId) << 16 | my_worker_id() as FinishId) << 32;
    prefix | next_finish_local_id() as FinishId
}

// place part of lower activity id is the place it spwan
pub(crate) fn new_global_activity_id(fid: FinishId) -> ActivityId {
    let suffix = ((here() as ActivityId) << 16 | my_worker_id() as ActivityId) << 32;
    let suffix = suffix | next_activity_local_id() as ActivityId;
    (fid as ActivityId) << 64 | suffix
}

#[cfg(test)]
pub mod test {

    use super::*;
    use std::collections::HashSet;
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::sync::MutexGuard;
    use std::thread;

    const TEST_HERE: Place = 7;
    static TEST_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

    fn global_id_reset_everything() {
        match HERE_STATIC.set(TEST_HERE) {
            // only set once
            _ => (),
        }
        *NEXT_WORKER_ID.lock().unwrap() = 0;
        HERE_LOCAL.with(|h| h.set(None));
        WORKER_ID.with(|w| w.set(None));
        NEXT_FINISH_LOCAL_ID.with(|f| f.set(0));
        NEXT_ACTIVITY_LOCAL_ID.with(|a| a.set(0));
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
            global_id_reset_everything();
            ret
        }
    }

    #[test]
    fn test_here() {
        let _a = TestGuardForStatic::new();
        let mut threads = vec![];
        for _ in 0..8 {
            threads.push(thread::spawn(|| {
                assert!(HERE_LOCAL.with(|h| h.get().is_none()));
                assert_eq!(here(), TEST_HERE);
                assert!(HERE_LOCAL.with(|h| h.get().is_some()));
                assert_eq!(here(), TEST_HERE);
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    }

    #[test]
    fn test_worker_id() {
        let _a = TestGuardForStatic::new();
        let mut threads = vec![];
        let (tx, rx) = mpsc::sync_channel::<WorkerId>(0);
        let range = 0..8;
        for _ in range.clone() {
            let tx = tx.clone();
            threads.push(thread::spawn(move || {
                assert!(WORKER_ID.with(|h| h.get().is_none()));
                let myid = my_worker_id();
                assert!(WORKER_ID.with(|h| h.get().is_some()));
                assert_eq!(my_worker_id(), myid);
                tx.send(myid).unwrap();
            }));
        }
        let mut ids = vec![];
        for _ in range.clone() {
            ids.push(rx.recv().unwrap());
        }
        for t in threads {
            t.join().unwrap();
        }

        // all workder id should be unique
        let s: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), s.len());
    }

    #[test]
    fn test_local_id() {
        let _a = TestGuardForStatic::new();
        let mut threads = vec![];
        let range = 0..8;
        for _ in range.clone() {
            threads.push(thread::spawn(move || {
                for j in 1..10 {
                    assert_eq!(next_finish_local_id(), j as FinishLocalId);
                    assert_eq!(next_activity_local_id(), j as FinishLocalId);
                }
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    }

    #[test]
    fn test_global_id() {
        let _a = TestGuardForStatic::new();

        // id get works well
        let fid = new_global_finish_id();
        assert_eq!(fid.get_place(), TEST_HERE);
        let aid = new_global_activity_id(fid);
        assert_eq!(aid.get_finish_id(), fid);
        assert_eq!(aid.get_spawned_place(), here());
        let new_fid: FinishId = 12346;
        let new_aid = aid.update_finish_id(new_fid);
        assert_eq!(new_aid.get_finish_id(), new_fid);
        assert_eq!(new_aid.update_finish_id(fid), aid); // set back
        assert_eq!(aid.get_lower(), aid.update_finish_id(0) as ActivityIdLower);

        // globally unique
        let (atx, arx) = mpsc::sync_channel::<ActivityId>(0);
        let (ftx, frx) = mpsc::sync_channel::<FinishId>(0);

        let mut threads = vec![];
        let range = 0..8;
        for _ in range.clone() {
            let atx = atx.clone();
            let ftx = ftx.clone();
            threads.push(thread::spawn(move || {
                for _ in 1..1025 {
                    let fid = new_global_finish_id();
                    ftx.send(fid).unwrap();
                    atx.send(new_global_activity_id(fid)).unwrap();
                }
            }));
        }
        let mut aids = vec![];
        let mut fids = vec![];
        for _ in 0..8 * 1024 {
            fids.push(frx.recv().unwrap());
            aids.push(arx.recv().unwrap());
        }
        for t in threads {
            t.join().unwrap();
        }

        let count_fids: HashSet<_> = fids.iter().cloned().collect();
        let count_aids: HashSet<_> = aids.iter().cloned().collect();
        assert_eq!(aids.len(), count_aids.len());
        assert_eq!(fids.len(), count_fids.len());
    }
}
