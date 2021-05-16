use crayfish_macros::*;

#[arg]
struct Quz<T> {
    p: usize,
    t: T,
}

#[arg_squashed]
struct Quzz<T>
where
    T: Copy
{
    p: usize,
    t: T,
}

#[arg]
struct Quzzz<T, U>
where
    T: Copy + Send,
    U: Send + Sync,
{
    p: usize,
    t: T,
    u: U,
}

use crayfish::args::RemoteSend;

#[arg]
#[derive(Debug)]
struct Foo {
    p: usize,
}

#[arg_squashed]
#[derive(Default)]
struct Baz {
    p: usize,
    d: i32,
}

#[arg_squashable]
enum Bar {
    Ha(usize),
    Ho(usize),
}

impl RemoteSend for Bar {
    type Output = Baz;
    fn is_squashable() -> ::std::primitive::bool {
        false
    }
    fn fold(&self, _acc: &mut Self::Output) {
        panic!()
    }
    fn extract(_out: &mut Self::Output) -> ::std::option::Option<Self>
    where
        Self: Sized,
    {
        panic!()
    }
    fn reorder(&self, _other: &Self) -> ::std::cmp::Ordering {
        panic!()
    }
}

