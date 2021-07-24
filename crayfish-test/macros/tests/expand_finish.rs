use crayfish_macros::*;

#[activity]
#[allow(unused_variables)]
async fn baz() -> i32 {
    123
}

#[activity]
#[allow(unused_variables)]
async fn bar(a: i32, b: i64, c: Vec<usize>, d: String) -> i32 {
    finish!{
        ff!(crayfish::place::here(), baz());
    };
    let a = finish!{
        ff!(crayfish::place::here(), baz());
        let ret = at!(crayfish::place::here(), baz());
        ret.await
    };

    finish!(
        ff!(crayfish::place::here(), baz());
        let ret = at!(crayfish::place::here(), baz());
        ret.await
    ) + a
}
