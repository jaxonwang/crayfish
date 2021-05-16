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
        ff!(crayfish::global_id::here(), baz());
    };
    let a = finish!{
        ff!(crayfish::global_id::here(), baz());
        let ret = at!(crayfish::global_id::here(), baz());
        ret.await
    };

    finish!(
        ff!(crayfish::global_id::here(), baz());
        let ret = at!(crayfish::global_id::here(), baz());
        ret.await
    ) + a
}
