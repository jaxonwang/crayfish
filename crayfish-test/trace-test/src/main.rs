use crayfish::args::RemoteSend;
use crayfish::place;
use crayfish::place::Place;
use crayfish::logging::*;
use std::cmp::Ordering;
use std::convert::TryInto;
use std::thread;
use std::time;

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

async fn finish() {
    crayfish::finish! {
    if place::here() == 0 {

        let here = place::here();
        let world_size = place::world_size();
        let dst_place = ((here + 1) as usize % world_size) as Place;
        crayfish::ff!(dst_place, real_fn(A { value: 2 }, B { value: 3 }, 1));

        info!("Main finished")
    }
    }
}

#[crayfish::main]
async fn main() {
    let task = || {
        let duration = time::Duration::from_micros(1);
        for _ in 0..1000 {
            crayfish::profiling_start!("trace-test0");
            thread::sleep(duration);
            crayfish::profiling_stop!();

            crayfish::profiling_start!("trace-test1");
            thread::sleep(duration);
            crayfish::profiling_stop!();
        }
        println!("done")
    };

    let t0 = thread::spawn(task.clone());
    let t1 = thread::spawn(task);
    t0.join().unwrap();
    t1.join().unwrap();

    finish().await;

    crayfish::trace::print_profiling();
}
