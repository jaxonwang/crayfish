use crayfish_macros::*;
use crayfish::args::RemoteSend;

#[arg]
#[derive(Debug)]
struct Foo{
    p: usize,
}

// #[arg]
// struct Quz<'a, T> where T:RemoteSend + crayfish::Deserialize<'a>{
//     p: usize,
//     t: T
// }

#[arg_squashed]
#[derive(Default)]
struct Baz{
    p: usize,
    d: i32,
}

#[arg_squashable]
enum Bar{
    Ha(usize),
    Ho(usize),
}

impl RemoteSend for Bar{
            type Output = Baz;
            fn is_squashable() -> ::std::primitive::bool {
                false
            }
            fn fold(&self, _acc: &mut Self::Output){
                panic!()
            }
            fn extract(_out: &mut Self::Output) -> ::std::option::Option<Self>
            where
                Self: Sized
            {
                panic!()
            }
            fn reorder(&self, _other: &Self) -> ::std::cmp::Ordering{
                panic!()
            }
}

pub fn main() {
}
