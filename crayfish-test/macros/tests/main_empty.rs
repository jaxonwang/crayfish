use std::io::Result;

#[crayfish_macros::main]
pub async fn foo() -> Result<()>{
    println!("hello foo");
    Ok(())
} 
