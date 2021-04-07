use rust_apgas::network;
use rust_apgas::logging;
use rust_apgas::logging::*;

use std::thread;
use std::time;

extern crate rust_apgas;

pub fn main() {
    let a = "hello world!";
    let mut callback = |src:network::Rank, buf:&[u8]|{
        info!("{} from:{} {}", src.int(), String::from_utf8_lossy(buf), a);
    };
    logging::setup_logger();
    let mut context = network::CommunicationContext::new(&mut callback);
    context.run();
    let context = context;
    let here = context.here();
    let my_rank = here.int();
    let world = context.world_size();

    println!("{:?}", context.cmd_args());
    println!("my rank {:?}, world size {}", my_rank, world);
    for p in 0..world{
        context.send(network::Rank::new(p as i32), format!("I am {}", my_rank).as_bytes());
    }

    thread::sleep(time::Duration::from_secs(1));

}
