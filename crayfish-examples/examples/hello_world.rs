use crayfish::logging;
use crayfish::logging::*;
use crayfish::network;
use crayfish::network::CollectiveOperator;
use crayfish::network::MessageSender;
use crayfish::network::Rank;

extern crate crayfish;

fn print_hostname() {
    use std::process::Command;
    let mut cmd = Command::new("hostname");
    let name = cmd.output().unwrap().stdout;
    info!("My hostname is {}", String::from_utf8_lossy(&name[..]));
}

const MAX_LINUX_UDP_DATAGRAM: usize = 65535;

pub fn main() {
    let payload_len = MAX_LINUX_UDP_DATAGRAM * 10;
    let payload: Vec<u8> = (0..payload_len).map(|a| (a % 256) as u8).collect();

    let a = "hello world!";
    let callback = |src: network::Rank, buf: &[u8]| {
        assert_eq!(buf, &payload[..]);
        // assert_eq!(buf.len(), payload.len());
        info!("{} {} bytes from:{} ", a, buf.len(), src.as_i32());
    };
    logging::setup_logger().unwrap();
    let context = network::context::CommunicationContext::new(callback);
    let sender = context.single_sender();

    let here = context.here();
    let world = context.world_size();

    print_hostname();
    crossbeam::scope(|scope| {
        let mut context = context;
        let coll = context.collective_operator();
        context.init();
        scope.spawn(|_| {
            let sender = sender;
            let mut coll = coll;
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            info!("before barrier");
            coll.barrier();
            info!("after barrier");
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            info!("before barrier notify");
            coll.barrier_notify();
            coll.barrier_try();
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            coll.barrier_wait();
            info!("after barrier wait");
            for p in 0..world {
                sender.send_msg(network::Rank::new(p as i32), payload.clone());
            }
            log::logger().flush();

            let broadcast_value:usize = 123456;
            let received: usize;
            let root = Rank::new(0);
            if here == root {
                received = coll.broadcast(root, Some(broadcast_value));
            } else {
                received = coll.broadcast(root, None);
            }
            assert_eq!(broadcast_value, received);
            info!("broadcast value {}", broadcast_value);
        });
        context.run();
    })
    .unwrap();

    info!("exit gracefully!");
}
