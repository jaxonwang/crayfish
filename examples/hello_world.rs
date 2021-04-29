use rust_apgas::logging;
use rust_apgas::logging::*;
use rust_apgas::network;
use rust_apgas::network::MessageSender;

extern crate rust_apgas;

fn print_hostname(){
    use std::process::Command;
    let mut cmd = Command::new("hostname");
    let name = cmd.output().unwrap().stdout;
    info!("My hostname is {}", String::from_utf8_lossy(&name[..]));
}

pub fn main() {

    let payload_len = 90020usize;
    let payload: Vec<u8> = (0..payload_len).map(|a| (a % 256) as u8).collect();


    let a = "hello world!";
    let callback = |src: network::Rank, buf: &[u8]| {
        assert_eq!(buf, &payload[..]);
        // assert_eq!(buf.len(), payload.len());
        info!("{} {} bytes from:{} ", a, buf.len(), src.as_i32());
    };
    logging::setup_logger().unwrap();
    let mut context = network::context::CommunicationContext::new(callback);
    let sender = context.single_sender();
    let context = context;

    let world = context.world_size();

    print_hostname();
    crossbeam::scope(|scope| {
        let mut context = context;
        context.init();
        scope.spawn(|_| {
            let sender = sender;
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            log::logger().flush();
        });
        context.run();

    })
    .unwrap();

    info!("exit gracefully!");
}
