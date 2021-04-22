extern crate num_cpus;
extern crate once_cell;
use once_cell::sync::Lazy;

pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub static NUM_CPUS: Lazy<usize> = Lazy::new(||num_cpus::get());
