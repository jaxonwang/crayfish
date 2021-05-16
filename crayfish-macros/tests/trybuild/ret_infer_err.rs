use crayfish_macros::*;

#[activity]
#[allow(unused_variables)]
fn bar_impl(a: i32) -> impl Fut<Output=usize>{
    async {
        123
    }
}

pub fn main() {}
