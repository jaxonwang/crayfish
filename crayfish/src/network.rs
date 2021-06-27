use std::convert::TryInto;
use crate::global_id::Place;
use std::fmt;
use futures::channel::oneshot;

pub trait MessageHandler: for<'a> FnMut(Rank, &'a [u8]) {}
impl<T> MessageHandler for T where T: for<'a> FnMut(Rank, &'a [u8]) {}

pub trait MessageSender: Send + 'static {
    fn send_msg(&self, dst: Rank, message: Vec<u8>);
}

pub(crate) trait CollectiveOperator : Send + 'static{
    fn barrier(&mut self) -> oneshot::Receiver<()>;
    fn barrier_done(&mut self);
    fn broadcast(&self, root: Rank, bytes:*mut u8, size: usize) -> oneshot::Receiver<()>;
    fn all_gather(&self, bytes:Vec<u8>) -> oneshot::Receiver<Vec<Vec<u8>>>;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Rank {
    rank: i32,
}

impl Rank {
    pub fn new(rank: i32) -> Self {
        Rank { rank }
    }
    pub fn from_place(place: Place) -> Self{
        Self::new(place as i32)
    }
    pub fn from_usize(rank: usize) -> Self {
        Rank { rank: rank as i32 }
    }
    pub fn as_place(&self) -> Place{
        self.rank as Place
    }

    pub fn as_i32(&self) -> i32 {
        self.rank
    }
    pub fn as_usize(&self) -> usize {
        self.rank.try_into().unwrap()
    }
}
impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rank)
    }
}

pub mod context {
    pub use crate::gasnet::*;
}
