use crayfish_macros::*;

use futures::Future;
use futures::FutureExt;

#[activity]
#[allow(unused_variables)]
async fn baz() -> i32 {
    123
}

#[activity]
#[allow(unused_variables)]
async fn bar(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
    let ret = at!(crayfish::place::here(), baz());
    ret.await
}

#[activity]
#[allow(unused_variables)]
async fn foo(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
    let ret = ff!(crayfish::place::here(), bar(123, 43, vec![2,3,4], String::from("adsa")));
    let ret = at!(crayfish::place::here(), bar(123, 43, vec![2,3,4], String::from("adsa")));
    ret.await
}

mod inside{
    extern crate crayfish as duang;
    #[crayfish_macros::activity(crate="duang")]
    #[allow(unused_variables)]
    async fn foo(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
        123
    }
}

#[activity]
#[allow(unused_variables)]
async fn bar_box(a: i32, b: i64, c: Vec<usize>, d: String) -> usize {
    if a == 0 {
        123
    }else{
        at!(crayfish::place::here(), bar_impl(a-1)).await
    }
}

#[activity]
#[allow(unused_variables)]
fn bar_impl(a: i32) -> impl Future<Output=usize>{
    async move{
        123
    }.boxed()
}

use std::future::Future as Fut;
#[activity(ret = "Vec<usize>")]
#[allow(unused_variables)]
fn no_infer_ret(a: i32) -> impl Fut<Output=Vec<usize>>{
    async {
        vec![123]
    }
}
