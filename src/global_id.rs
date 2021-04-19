use lazy_static::lazy_static;
use std::cell::Cell;
use std::sync::Mutex;

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
    fn get_activity_place(&self) -> Place;
    fn get_finish_id(&self) -> FinishId;
    fn update_finish_id(&self, fid: FinishId) -> ActivityId;
    fn get_lower(&self) -> ActivityIdLower;
}

impl ActivityIdMethods for ActivityId {
    fn get_activity_place(&self) -> Place {
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

lazy_static! {
    static ref HERE_STATIC: Mutex<Option<Place>> = Mutex::new(None);
    static ref NEXT_WORKER_ID: Mutex<WorkerId> = Mutex::new(0);
}

thread_local! {
    pub static HERE_LOCAL: Cell<Option<Place>> = Cell::new(None);
    static WORKER_ID: Cell<Option<WorkerId>> = Cell::new(None);
    static NEXT_FINISH_LOCAL_ID: Cell<FinishLocalId> = Cell::new(0); // start from 1
    static NEXT_ACTIVITY_LOCAL_ID: Cell<ActivityLocalId> = Cell::new(0); // start from 1
}

pub fn here() -> Place {
    HERE_LOCAL.with(|here| match here.get() {
        Some(p) => p,
        None => {
            let p = HERE_STATIC.lock().unwrap();
            let p = p.as_ref().expect("here place id is not initialized");
            here.set(Some(*p));
            *p
        }
    })
}

pub fn my_worker_id() -> WorkerId {
    WORKER_ID.with(|wid| {
        match wid.get() {
            Some(w) => w,
            None => {
                let mut w = NEXT_WORKER_ID.lock().unwrap();
                wid.set(Some(*w));
                let myid = *w;
                *w = w.checked_add(1).unwrap(); // force overflow panic
                myid
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

pub fn new_global_finish_id() -> FinishId {
    let prefix = ((here() as FinishId) << 16 | my_worker_id() as FinishId) << 32;
    prefix | next_finish_local_id() as FinishId
}

pub fn new_global_activity_id(fid: FinishId, place: Place) -> ActivityId {
    let suffix = ((place as ActivityId) << 16 | my_worker_id() as ActivityId) << 32;
    let suffix = suffix | next_activity_local_id() as ActivityId;
    (fid as ActivityId) << 64 | suffix
}

#[cfg(test)]
mod test {

    use std::collections::HashSet;
    use std::sync::mpsc;
    use std::sync::MutexGuard;
    use std::thread;

    const TEST_HERE: Place = 7;
    lazy_static! {
        static ref TEST_LOCK: Mutex<bool> = Mutex::new(false);
    }
    use super::*;
    struct SetHere<'a> {
        _guard: MutexGuard<'a, bool>,
    }

    impl<'a> SetHere<'a> {
        fn new() -> Self {
            let ret = SetHere {
                _guard: TEST_LOCK.lock().unwrap(),
            };
            let mut p = HERE_STATIC.lock().unwrap();
            assert!(p.is_none());
            *p = Some(TEST_HERE);
            ret
        }
    }
    impl<'a> Drop for SetHere<'a> {
        fn drop(&mut self) {
            let mut p = HERE_STATIC.lock().unwrap();
            *p = None;
        }
    }

    #[test]
    fn test_here() {
        let _a = SetHere::new();
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
        let _a = SetHere::new();
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
        let _a = SetHere::new();
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
        let _a = SetHere::new();

        // id get works well
        let fid = new_global_finish_id();
        assert_eq!(fid.get_place(), TEST_HERE);
        let aid = new_global_activity_id(fid, 233 as Place);
        assert_eq!(aid.get_finish_id(), fid);
        assert_eq!(aid.get_activity_place(), 233 as Place);
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
                    atx.send(new_global_activity_id(fid, 123)).unwrap();
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
