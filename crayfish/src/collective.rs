use crate::args::RemoteSend;
use crate::global_id;
use crate::global_id::Place;
use crate::network;
use crate::network::CollectiveOperator;
use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use parking_lot::const_mutex;
use parking_lot::Mutex;
use std::cell::Cell;

static COLLECTIVE_OPERATOR: Mutex<Option<Box<dyn CollectiveOperator>>> = const_mutex(None);

pub(crate) fn set_coll(coll: Box<dyn CollectiveOperator>) {
    let mut h = COLLECTIVE_OPERATOR.lock();
    debug_assert!(h.is_none());
    *h = Some(coll);
}

pub(crate) fn take_coll() -> Box<dyn CollectiveOperator> {
    COLLECTIVE_OPERATOR.lock().take().unwrap()
}

// const COLL_ERR_MSG: &str = "collective operation is only allowed in main thread";
const COLL_ERR_MSG_FINISH: &str = "collective operation is not allowed in any finish block";

pub struct FinishCounterGuard {}

impl FinishCounterGuard {
    pub fn new() -> Self {
        FINISH_COUNTER.with(|c| c.set(c.get() + 1));
        FinishCounterGuard {}
    }
}
impl Drop for FinishCounterGuard {
    fn drop(&mut self) {
        FINISH_COUNTER.with(|c| c.set(c.get() - 1));
    }
}

thread_local! {
    static FINISH_COUNTER:Cell<usize> = Cell::new(0);
}

fn perform_collective<F, T>(mut f: F) -> T
where
    F: FnMut(&mut dyn CollectiveOperator) -> T,
{
    FINISH_COUNTER.with(|c| assert_eq!(0, c.get(), "{}", COLL_ERR_MSG_FINISH));
    let mut handle = COLLECTIVE_OPERATOR.lock();
    let coll = handle.as_mut().unwrap().as_mut();
    f(coll)
}

pub fn broadcast_copy<T: Copy + 'static + Send>(root: Place, value: &mut T) {
    let type_size = std::mem::size_of::<T>();
    perform_collective(|coll| {
        coll.broadcast(
            network::Rank::from_place(root),
            value as *mut T as *mut u8,
            type_size,
        )
    })
}

pub fn broadcast<T: RemoteSend>(root: Place, value: &mut T) {
    // broadcast size first. Then perform value broadcast
    let mut bytes: Vec<u8> = vec![];
    let mut serialized_len = 0;
    let here = global_id::here();
    if here == root {
        serialize_into(&mut bytes, value).unwrap();
        serialized_len = bytes.len();
    }
    broadcast_copy(root, &mut serialized_len);
    // now serialized len is the same
    if here != root {
        bytes = vec![0u8; serialized_len];
    }

    perform_collective(|coll| {
        coll.broadcast(
            network::Rank::from_place(root),
            bytes.as_mut_ptr(),
            serialized_len,
        )
    });

    if here != root {
        *value = deserialize_from(&bytes[..]).unwrap();
    }
}

pub fn barrier() {
    perform_collective(|c| c.barrier())
}

pub fn barrier_notify() {
    perform_collective(|c| c.barrier_notify())
}

pub fn barrier_wait() {
    perform_collective(|c| c.barrier_wait())
}

pub fn barrier_try() -> bool {
    perform_collective(|c| c.barrier_try())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::network::Rank;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex; // use for a non poison mutex
    use parking_lot::MutexGuard;
    use std::thread;

    static TEST_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
    struct TestGuardForStatic<'a> {
        _guard: MutexGuard<'a, bool>,
    }

    fn init_coll() {
        *COLLECTIVE_OPERATOR.lock() = Some(Box::new(MockCollOp {}));
    }

    impl<'a> TestGuardForStatic<'a> {
        pub fn new() -> Self {
            let ret = TestGuardForStatic {
                // must get test lock first
                _guard: TEST_LOCK.lock(),
            };
            init_coll();
            ret
        }
    }

    struct MockCollOp {}

    impl CollectiveOperator for MockCollOp {
        fn barrier(&self) {}
        fn barrier_notify(&mut self) {}
        fn barrier_wait(&mut self) {}
        fn barrier_try(&mut self) -> bool {
            true
        }
        fn broadcast(&self, _: Rank, _: *mut u8, _: usize) {}
    }

    fn do_coll() {
        barrier();
        barrier_notify();
        barrier_wait();
        barrier_try();
    }

    #[test]
    pub fn test_collective() {
        let _t = TestGuardForStatic::new();
        // put in thread, return coll before test lock released.
        thread::spawn(|| {
            let _g = CollGuard::new();
            do_coll();
        })
        .join()
        .unwrap();
    }

    #[test]
    #[should_panic]
    pub fn test_collective_dobule_set() {
        let _t = TestGuardForStatic::new();
        set_coll(Box::new(MockCollOp {}));
    }

    #[test]
    pub fn test_collective_main_thread_exit_return_back() {
        let _t = TestGuardForStatic::new();
        let (tx1, rx1) = std::sync::mpsc::channel();
        let (tx2, rx2) = std::sync::mpsc::channel();
        let t = thread::spawn(move || {
            let _g = CollGuard::new();
            do_coll();
            tx1.send(()).unwrap();
            rx2.recv().unwrap();
            drop(_g);
        });
        rx1.recv().unwrap();
        assert!(COLLECTIVE_OPERATOR.lock().is_none());
        tx2.send(()).unwrap();
        t.join().unwrap();
        assert!(COLLECTIVE_OPERATOR.lock().is_some());
    }

    #[test]
    pub fn test_more_than_1_thread() {
        let _t = TestGuardForStatic::new();
        let thread_num = 10;
        let _g = CollGuard::new();
        do_coll();
        let threads: Vec<_> = (0..thread_num)
            .map(|_| {
                thread::spawn(|| {
                    do_coll();
                })
            })
            .collect();
        let paniced: usize = threads
            .into_iter()
            .map(|t| match t.join() {
                Ok(_) => 0,
                Err(_) => 1,
            })
            .sum();
        assert_eq!(paniced, thread_num);
    }

    #[test]
    pub fn test_with_counter() {
        let _t = TestGuardForStatic::new();
        let _g = CollGuard::new();
        FinishCounterGuard::new();
        do_coll();
    }

    #[test]
    #[should_panic]
    pub fn test_with_counter_panic() {
        let _t = TestGuardForStatic::new();
        let _g = CollGuard::new();
        let _fg = FinishCounterGuard::new();
        do_coll()
    }
}
