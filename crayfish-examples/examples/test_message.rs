use crayfish::essence;
use crayfish::logging;
use crayfish::logging::*;
use crayfish::network;
use crayfish::network::MessageSender;

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

    let world = context.world_size();

    print_hostname();
    crossbeam::scope(|scope| {
        let mut context = context;
        essence::init_collective_operator(&context);
        context.init();
        scope.spawn(move |_| context.run());
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
            // to stop the context
            crayfish::collective::take_and_release_coll();
        });
    })
    .unwrap();

    info!("exit gracefully!");
}
