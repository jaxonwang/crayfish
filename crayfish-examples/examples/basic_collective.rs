use crayfish::collective;
use crayfish::global_id;
use crayfish::logging::*;
use crayfish::place::Place;
use std::collections::HashMap;
use std::iter::FromIterator;

#[crayfish::main]
async fn main() {
    info!("before barrier");
    let f = collective::barrier();
    info!("after barrier");
    info!("before barrier notify");
    f.await;

    let broadcast_value: usize = 12345;
    let mut received: usize = 0;
    let root: Place = 0;
    if global_id::here() == root {
        received = broadcast_value;
    }
    collective::broadcast_copy(root, &mut received).await;
    assert_eq!(broadcast_value, received);
    info!("broadcast value {}", broadcast_value);

    let mut m = HashMap::new();
    let map_to_broadcast: HashMap<usize, usize> = HashMap::from_iter((0..10).map(|a| (a, a + 10)));

    if global_id::here() == root {
        m = map_to_broadcast.clone();
    }
    collective::broadcast(root, &mut m).await;
    assert_eq!(m, map_to_broadcast);

    let value = global_id::here() as usize;
    let values = collective::all_gather(value).await;
    info!("all gather ranks {:?}", values);
}
