use gex_sys;

pub fn main() {
    let (args, cl, ep, tm) = gex_sys::gex_Client_init();
    let my_rank = gex_sys::gex_System_QueryJobRank();
    let size = gex_sys::gex_System_QueryJobSize();
    println!("{:?}", args);
    println!("my rank {:?}, world size {}", my_rank, size);
}
