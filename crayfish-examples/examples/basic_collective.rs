use crayfish::collective;
use crayfish::global_id;
use crayfish::logging::*;
use crayfish::place::Place;

#[crayfish::main]
async fn main() {
    info!("before barrier");
    collective::barrier();
    info!("after barrier");
    info!("before barrier notify");
    collective::barrier_notify();
    // collective::barrier_try();
    collective::barrier_wait();

    let broadcast_value: usize = 12345;
    let mut received: usize = 0;
    let root: Place = 0;
    if global_id::here() == root {
        received = broadcast_value;
    }
    collective::broadcast(root, &mut received);
    assert_eq!(broadcast_value, received);
    info!("broadcast value {}", broadcast_value);
}
