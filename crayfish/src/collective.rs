use crate::args::RemoteSend;
use crate::place::here;
use crate::place::Place;
use crate::network;
use crate::network::CollectiveOperator;
use crate::serialization::deserialize_from;
use crate::serialization::serialize_into;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use futures::Future;
use parking_lot::const_mutex;
use parking_lot::Mutex;
use std::cell::Cell;

static COLLECTIVE_OPERATOR: Mutex<Option<Box<dyn CollectiveOperator>>> = const_mutex(None);

pub(crate) fn set_coll(coll: Box<dyn CollectiveOperator>) {
    let mut h = COLLECTIVE_OPERATOR.lock();
    if cfg!(test) {
        assert!(h.is_none());
    }
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

fn perform_collective<F, T>(f: F) -> T
where
    F: FnOnce(&mut dyn CollectiveOperator) -> T,
{
    FINISH_COUNTER.with(|c| assert_eq!(0, c.get(), "{}", COLL_ERR_MSG_FINISH));
    let mut handle = COLLECTIVE_OPERATOR.lock();
    let coll = handle.as_mut().unwrap().as_mut();
    f(coll)
}

pub fn broadcast_copy<T: Copy + 'static + Send>(
    root: Place,
    value: &mut T,
) -> impl Future<Output = ()> {
    let type_size = std::mem::size_of::<T>();
    perform_collective(|coll| {
        coll.broadcast(
            network::Rank::from_place(root),
            value as *mut T as *mut u8,
            type_size,
        )
    })
    .map(|a| a.unwrap())
}

pub fn broadcast<'a, T: RemoteSend>(root: Place, value: &'a mut T) -> BoxFuture<'a, ()> {
    // broadcast size first. Then perform value broadcast
    let mut bytes: Vec<u8> = vec![];
    let mut serialized_len = 0;
    let here = here();
    if here == root {
        serialize_into(&mut bytes, value).unwrap();
        serialized_len = bytes.len();
    }
    // TODO: use another thread for non blocking first phase broadcast?
    futures::executor::block_on(broadcast_copy(root, &mut serialized_len));
    // now serialized len is the same
    if here != root {
        bytes = vec![0u8; serialized_len];
    }

    let f = perform_collective(|coll| {
        coll.broadcast(
            network::Rank::from_place(root),
            bytes.as_mut_ptr(),
            serialized_len,
        )
    });

    async move {
        f.await.unwrap();
        if here != root {
            *value = deserialize_from(&bytes[..]).unwrap();
        }
    }
    .boxed()
}

pub fn barrier() -> impl Future<Output = ()> {
    let f = perform_collective(|c| c.barrier());
    f.map(|r| {
        r.unwrap();
        perform_collective(|c| c.barrier_done());
    })
}

pub fn all_gather<T: RemoteSend>(input: T) -> impl Future<Output = Vec<T>> {
    let mut bytes = vec![];
    serialize_into(&mut bytes, &input).unwrap();
    let f = perform_collective(move |c| c.all_gather(bytes));
    f.map(|r| {
        let mut ret: Vec<T> = vec![];
        for item in r.unwrap().into_iter() {
            ret.push(deserialize_from(&item[..]).unwrap());
        }
        ret
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::network::Rank;
    use once_cell::sync::Lazy;
    use parking_lot::Mutex; // use for a non poison mutex
    use parking_lot::MutexGuard;

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
    use futures::channel::oneshot;

    impl CollectiveOperator for MockCollOp {
        fn barrier(&mut self) -> oneshot::Receiver<()> {
            let (tx, rx) = oneshot::channel();
            tx.send(()).unwrap();
            rx
        }
        fn barrier_done(&mut self) {}

        fn broadcast(&self, _root: Rank, _bytes: *mut u8, _size: usize) -> oneshot::Receiver<()> {
            let (tx, rx) = oneshot::channel();
            tx.send(()).unwrap();
            rx
        }
        fn all_gather(&self, _bytes: Vec<u8>) -> oneshot::Receiver<Vec<Vec<u8>>> {
            let (tx, rx) = oneshot::channel();
            tx.send(vec![]).unwrap();
            rx
        }
    }

    fn do_coll() {
        futures::executor::block_on(barrier());
    }

    #[test]
    #[should_panic]
    pub fn test_collective_dobule_set() {
        let _t = TestGuardForStatic::new();
        set_coll(Box::new(MockCollOp {}));
    }

    #[test]
    pub fn test_with_counter() {
        let _t = TestGuardForStatic::new();
        FinishCounterGuard::new();
        do_coll();
    }

    #[test]
    #[should_panic]
    pub fn test_with_counter_panic() {
        let _t = TestGuardForStatic::new();
        let _fg = FinishCounterGuard::new();
        do_coll()
    }
}
