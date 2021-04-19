extern crate num_cpus;
extern crate lazy_static;
use lazy_static::lazy_static;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

lazy_static! {
    pub static ref NUM_CPUS: usize = num_cpus::get();
}
