use crate::place::here;
use crate::place::Place;
use once_cell::sync::Lazy;
use std::cell::Cell;
use std::sync::Mutex;
use serde::Serialize;
use serde::Deserialize;

extern crate once_cell;

pub type WorkerId = u32;
type LocalId = u64;

/// composedID = PlaceId + WorkerId + LocalId
pub trait ComposedId {
    fn place(&self) -> Place;
    fn worker_id(&self) -> WorkerId;
    fn local_id(&self) -> LocalId;
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FinishId {
    place: Place,
    worker_id: WorkerId,
    local_id: LocalId,
}

impl FinishId {
    pub fn get_place(&self) -> Place {
        self.place
    }
}

impl FinishId {
    fn new(place: Place, worker_id: WorkerId, local_id: LocalId) -> Self {
        FinishId {
            place,
            worker_id,
            local_id,
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ActivityIdLower {
    place: Place,
    worker_id: WorkerId,
    local_id: LocalId,
}

impl ActivityIdLower {
    fn new(place: Place, worker_id: WorkerId, local_id: LocalId) -> Self {
        ActivityIdLower {
            place,
            worker_id,
            local_id,
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ActivityId {
    finish_id: FinishId,
    lower: ActivityIdLower,
}

impl std::fmt::Display for ActivityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}{:?}", self.finish_id, self.lower)
    }
}

impl Default for ActivityId{
    fn default() -> Self{
        ActivityId::zero()
    }
}

impl ActivityId {
    /// zero value used as calling tree root
    pub fn zero() -> Self {
        ActivityId {
            finish_id: FinishId::new(0, 0, 0),
            lower: ActivityIdLower::new(0, 0, 0),
        }
    }
    pub fn get_spawned_place(&self) -> Place {
        self.lower.place
    }
    pub fn get_finish_id(&self) -> FinishId {
        self.finish_id
    }
    pub fn update_finish_id(&mut self, fid: FinishId) {
        self.finish_id = fid;
    }
    pub fn get_lower(&self) -> ActivityIdLower {
        self.lower
    }
}


// two level mutex here, but I think the code is clear
static NEXT_WORKER_ID: Lazy<Mutex<WorkerId>> = Lazy::new(|| Mutex::new(0));

thread_local! {
    static WORKER_ID: Cell<Option<WorkerId>> = Cell::new(None);
    static NEXT_FINISH_LOCAL_ID: Cell<LocalId> = Cell::new(0); // start from 1
    static NEXT_ACTIVITY_LOCAL_ID: Cell<LocalId> = Cell::new(0); // start from 1
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

fn next_finish_local_id() -> LocalId {
    NEXT_FINISH_LOCAL_ID.with(|fid| {
        let old = fid.get();
        let new = old.checked_add(1).expect("Finish id overflows");
        fid.set(new);
        new
    })
}

fn next_activity_local_id() -> LocalId {
    NEXT_ACTIVITY_LOCAL_ID.with(|aid| {
        let old = aid.get();
        let new = old.checked_add(1).expect("Activity id overflows");
        aid.set(new);
        new
    })
}

pub(crate) fn new_global_finish_id() -> FinishId {
    FinishId::new(here(), my_worker_id(), next_finish_local_id())
}

// place part of lower activity id is the place it spwan
pub(crate) fn new_global_activity_id(fid: FinishId) -> ActivityId {
    ActivityId {
        finish_id: fid,
        lower: ActivityIdLower::new(here(), my_worker_id(), next_activity_local_id()),
    }
}

#[cfg(test)]
pub(crate) mod test {

    use super::*;
    use std::collections::HashSet;
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::sync::MutexGuard;
    use std::thread;

    pub const TEST_HERE: Place = 7;
    static TEST_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

    fn global_id_reset_everything() {
        use crate::place::HERE_LOCAL;
        use crate::place::HERE_STATIC;

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

    impl From<usize> for ActivityId{
        fn from(n: usize) -> Self{
            ActivityId{
                finish_id: FinishId::new(n as Place, n as WorkerId, n as LocalId),
                lower: ActivityIdLower::new(n as Place, n as WorkerId, n as LocalId),
            }
        }
    }

    impl From<usize> for FinishId{
        fn from(n: usize) -> Self{
            FinishId::new(n as Place, n as WorkerId, n as LocalId)
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
                    assert_eq!(next_finish_local_id(), j as LocalId);
                    assert_eq!(next_activity_local_id(), j as LocalId);
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
        assert_eq!(fid.place, TEST_HERE);
        let aid = new_global_activity_id(fid);
        assert_eq!(aid.get_finish_id(), fid);
        assert_eq!(aid.get_spawned_place(), here());
        let new_fid = FinishId::from(12346);
        let mut new_aid = aid.clone();
        new_aid.update_finish_id(new_fid);
        assert_eq!(new_aid.get_finish_id(), new_fid);
        new_aid.update_finish_id(fid); // set back
        assert_eq!(new_aid, aid);
        let new_aid_lower = new_aid.get_lower();
        new_aid.update_finish_id(FinishId::from(0));
        assert_eq!(new_aid_lower, new_aid.get_lower());

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
