use crayfish::collective;
use crayfish::ff;
use crayfish::finish;
use crayfish::global_id::here;
use crayfish::global_id::world_size;
use crayfish::logging::*;
use crayfish::place::Place;
use crayfish::shared;
use std::sync::Mutex;

extern crate crayfish;
extern crate rand;

#[crayfish::activity]
async fn change_local(local_num: shared::PlaceLocalWeak<Mutex<usize>>) {
    let strong_ref = local_num.upgrade().unwrap();
    let mut h = strong_ref.lock().unwrap();
    *h += 10086;
}

#[crayfish::main]
async fn main() {
    let local_num = shared::PlaceLocal::new(Mutex::new(0usize));
    collective::barrier().await;
    finish! {
        if here() == 0 {
            for i in 0..world_size() {
                ff!(i as Place, change_local(local_num.downgrade()));
            }
        }
    }
    collective::barrier().await;

    info!("now my local value is {}", local_num.lock().unwrap());
}
