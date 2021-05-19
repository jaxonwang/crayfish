use crayfish::collective;
use crayfish::global_id;
use crayfish::logging::*;
use crayfish::place::Place;
use std::collections::HashMap;
use std::iter::FromIterator;
use std::time;

#[crayfish::main]
async fn main() {
    info!("before barrier");
    collective::barrier();
    info!("after barrier");
    info!("before barrier notify");
    collective::barrier_notify();
    collective::barrier_wait();

    collective::barrier_notify();
    while !collective::barrier_try() {
        info!("try barrier until done.");
        std::thread::sleep(time::Duration::from_millis(10));
    }

    let broadcast_value: usize = 12345;
    let mut received: usize = 0;
    let root: Place = 0;
    if global_id::here() == root {
        received = broadcast_value;
    }
    collective::broadcast_copy(root, &mut received);
    assert_eq!(broadcast_value, received);
    info!("broadcast value {}", broadcast_value);

    let mut m = HashMap::new();
    let map_to_broadcast: HashMap<usize, usize> = HashMap::from_iter((0..10).map(|a| (a, a + 10)));

    if global_id::here() == root {
        m = map_to_broadcast.clone();
    }
    collective::broadcast(root, &mut m);
    assert_eq!(m, map_to_broadcast);
}
