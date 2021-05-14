use crayfish_macros::*;

#[arg]
struct Quz<T>{
    p: usize,
    t: T
}

#[arg_squashed]
struct Quzz<T> where T: Copy{
    p: usize,
    t: T
}

#[arg]
struct Quzzz<T, U> where T: Copy + Send, U: Send + Sync{
    p: usize,
    t: T,
    u: U
}
