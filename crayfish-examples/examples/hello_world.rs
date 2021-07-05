use crayfish::place;

#[crayfish::main]
async fn main() {
    println!("Hello world! My place id is {}", place::here());
}
