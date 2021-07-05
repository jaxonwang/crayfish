use crayfish::collective;
use crayfish::place;
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
    if place::here() == root {
        received = broadcast_value;
    }
    collective::broadcast_copy(root, &mut received).await;
    assert_eq!(broadcast_value, received);
    info!("broadcast value {}", broadcast_value);

    let mut m = HashMap::new();
    let map_to_broadcast: HashMap<usize, usize> = HashMap::from_iter((0..10).map(|a| (a, a + 10)));

    if place::here() == root {
        m = map_to_broadcast.clone();
    }
    collective::broadcast(root, &mut m).await;
    assert_eq!(m, map_to_broadcast);

    let value = place::here().to_string();
    let values = collective::all_gather(value.clone()).await;
    let expected: Vec<String> = (0..place::world_size())
        .map(|i| i.to_string())
        .collect();
    assert_eq!(values, expected);
    info!("all gather ranks {:?}", values);

    // many all gather
    let mut all_gather_futures = vec![];
    for _ in 0..10 {
        let value = (place::here() + 1).to_string();
        all_gather_futures.push(collective::all_gather(value.clone()));
    }
    for f in all_gather_futures {
        let expected: Vec<String> = (0..place::world_size())
            .map(|i| (i + 1).to_string())
            .collect();
        assert_eq!(f.await, expected);
    }
}
