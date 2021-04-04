use gex_sys;

pub fn main() {
    let (args, cl, ep, tm) = gex_sys::gex_Client_init();
    println!("{:?}", args);
}
