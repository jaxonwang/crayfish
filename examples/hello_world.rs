use rust_apgas::network;
use rust_apgas::logging;

use std::boxed::Box;

extern crate rust_apgas;

pub fn main() {
    let callback = Box::new(|src:network::Place, buf:&[u8]|{
        println!("{}: {}", src.rank(), String::from_utf8_lossy(buf));
    });
    logging::setup_logger();
    let mut context = network::CommunicationContext::new(callback);
    context.run();
    let here = context.here();
    let my_rank = here.rank();
    let world = context.world();
    let size = world.len();

    println!("{:?}", context.cmd_args());
    println!("my rank {:?}, world size {}", my_rank, size);
}
