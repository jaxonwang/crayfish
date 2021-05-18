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
use std::cell::RefCell;

static COLLECTIVE_OPERATOR: Mutex<Option<Box<dyn CollectiveOperator>>> = const_mutex(None);

pub(crate) fn set_coll(coll: Box<dyn CollectiveOperator>) {
    let mut h = COLLECTIVE_OPERATOR.lock();
    *h = Some(coll);
}

const COLL_ERR_MSG: &str =
    "collective operation is only allowed in main thread and out of any finish block";

pub(crate) fn take_coll() -> Box<dyn CollectiveOperator> {
    COLLECTIVE_OPERATOR.lock().take().expect(COLL_ERR_MSG)
}

thread_local! {
    static COLLECTIVE_OPERATOR_GUARD: RefCell<CollGuard> = RefCell::new(CollGuard{op: None});
    static FINISH_COUNTER: Cell<usize> = Cell::new(0);
}

struct CollGuard {
    op: Option<Box<dyn CollectiveOperator>>,
}

impl Drop for CollGuard { // if something, return to global
    fn drop(&mut self) {
        self.op.take().map(|c|set_coll(c));
    }
}

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

fn get_local_coll<F, T>(mut f: F) -> T
where
    F: FnMut(&mut dyn CollectiveOperator) -> T,
{
    FINISH_COUNTER.with(|c| assert_eq!(0, c.get(), "{}", COLL_ERR_MSG));
    COLLECTIVE_OPERATOR_GUARD.with(|g| loop {
        let mut guard = g.borrow_mut();

        if let Some(s_ref) = guard.op.as_mut() {
            return f(s_ref.as_mut());
        } else {
            let coll = take_coll();
            guard.op = Some(coll);
        }
    })
}

pub fn broadcast_copy<T: Copy + 'static + Send>(root: Place, value: &mut T) {
    let type_size = std::mem::size_of::<T>();
    get_local_coll(|coll| {
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

    get_local_coll(|coll| {
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
    get_local_coll(|c| c.barrier())
}

pub fn barrier_notify() {
    get_local_coll(|c| c.barrier_notify())
}

pub fn barrier_wait() {
    get_local_coll(|c| c.barrier_wait())
}

pub fn barrier_try() -> bool {
    get_local_coll(|c| c.barrier_try())
}

#[cfg(test)]
mod test {
    use super::*;

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

    #[test]
    pub fn test_collective(){
        set_coll(MockCollOp{});

    }
}
