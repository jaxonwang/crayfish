use crayfish_macros::*;


pub fn main() {

    let _ = async {
        finish!{
            let a = Some(1);
            let b = a.unwrap()?;
            println("{}", b);
        }
    };
}

