use rust_apgas::network;

extern crate rust_apgas;

pub fn main() {

    let context = network::CommunicationContext::new();
    let here = context.here();
    let my_rank = here.rank();
    let world = context.world();
    let size = world.len();

    println!("{:?}", context.cmd_args());
    println!("my rank {:?}, world size {}", my_rank, size);
}
