use rust_apgas::logging;
use rust_apgas::logging::*;
use rust_apgas::network;

use std::thread;
use std::time;

extern crate rust_apgas;

pub fn main() {
    let a = "hello world!";
    let mut callback = |src: network::Rank, buf: &[u8]| {
        info!("{} from:{} {}", src.as_i32(), String::from_utf8_lossy(buf), a);
    };
    logging::setup_logger().unwrap();
    let mut context = network::CommunicationContext::new(&mut callback);
    context.run();
    let sender = context.single_sender();
    let context = context;

    let here = context.here();
    let my_rank = here.as_i32();
    let world = context.world_size();

    thread::spawn(move || {
        for p in 0..world {
            sender.send(
                network::Rank::new(p as i32),
                format!("I am {}", my_rank).as_bytes(),
            );
        }
    });

    thread::sleep(time::Duration::from_secs(1));
}
