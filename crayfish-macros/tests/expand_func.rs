use crayfish_macros::*;

use futures::FutureExt;
use futures::future::BoxFuture;

#[activity]
#[allow(unused_variables)]
async fn foo(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
    123
}

mod inside{
    extern crate crayfish as duang;
    #[crayfish_macros::activity(crate="duang")]
    #[allow(unused_variables)]
    async fn foo(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
        123
    }
}
//
// #[activity]
// #[allow(unused_variables)]
// fn bar(ctx: &mut impl ApgasContext, a: i32, b: i64, c: Vec<usize>, d: String) -> BoxFuture<'static, usize>{
//     async move{
//         123
//     }.boxed()
// }
