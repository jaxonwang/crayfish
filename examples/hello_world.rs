use rust_apgas::logging;
use rust_apgas::logging::*;
use rust_apgas::network;
use signal_hook::consts::signal;
use std::sync::atomic;
use std::sync::Arc;

extern crate rust_apgas;
extern crate signal_hook;

fn print_hostname(){
    use std::process::Command;
    let mut cmd = Command::new("hostname");
    let name = cmd.output().unwrap().stdout;
    info!("My hostname is {}", String::from_utf8_lossy(&name[..]));
}

pub fn main() {
    // signal handling
    let got = Arc::new(atomic::AtomicBool::new(false));
    signal_hook::flag::register(signal::SIGQUIT, Arc::clone(&got)).unwrap();

    let payload_len = 90020usize;
    let payload: Vec<u8> = (0..payload_len).map(|a| (a % 256) as u8).collect();


    let a = "hello world!";
    let mut callback = |src: network::Rank, buf: &[u8]| {
        assert_eq!(buf, &payload[..]);
        // assert_eq!(buf.len(), payload.len());
        info!("{} {} bytes from:{} ", a, buf.len(), src.as_i32());
    };
    logging::setup_logger().unwrap();
    let mut context = network::CommunicationContext::new(&mut callback);
    let sender = context.single_sender();
    let context = context;

    let here = context.here();
    let world = context.world_size();

    print_hostname();
    crossbeam::scope(|scope| {
        let mut context = context;
        context.init();
        scope.spawn(|_| {
            let sender = sender;
            for p in 0..world {
                if p != here.as_usize(){
                    sender.send(network::Rank::new(p as i32), payload.clone());
                }
            }
            
            // std::thread::sleep(std::time::Duration::from_millis(1000));
            // for p in 0..world {
            //     if p != here.as_usize(){
            //         sender.send(network::Rank::new(p as i32), payload.clone());
            //     }
            // }
            log::logger().flush();
        });
        context.run();

    })
    .unwrap();

    info!("exit gracefully!");
}
