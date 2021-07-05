use crayfish::place::Place;
use crayfish::args::RemoteSend;
use crayfish::place;
use crayfish::logging::*;
use std::cmp::Ordering;
use std::convert::TryInto;

extern crate crayfish;

#[crayfish::arg_squashable]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct A {
    pub value: usize,
}

#[crayfish::arg_squashed]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

#[crayfish::arg_squashable]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct B {
    pub value: u8,
}
#[crayfish::arg_squashed]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

#[crayfish::arg]
#[derive(Debug)]
struct R {
    a: A,
    b: B,
    c: i32,
}

#[crayfish::activity]
async fn real_fn(a: A, b: B, c: i32) -> R {
    // macro
    debug!("execute func with args: {:?}, {:?}, {}", a, b, c);
    if c < 200 {
        let here = place::here();
        let world_size = place::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        crayfish::ff!(dst_place, real_fn(a.clone(), b.clone(), c + 1));
    }
    R { a, b, c: c + 1 }
}

// desugered finish
#[crayfish::main]
async fn finish() {
    crayfish::finish!{
    if place::here() == 0 {
        // ctx contains a new finish id now
        //
        let here = place::here();
        let world_size = place::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        // let f = async_create_for_fn_id_0(&mut ctx, dst_place, A { value: 1 }, B { value: 2 }, 3);
        //
        // debug!("waiting return of the function");
        // let ret = f.await; // if await, remove it from activity list this finish block will wait
        // debug!("got return value {:?}", ret);
        crayfish::ff!(dst_place, real_fn(A { value: 2 }, B { value: 3 }, 1));

        info!("Main finished")
    }
    }
}
