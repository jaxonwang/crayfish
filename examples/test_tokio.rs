use tokio::runtime::Runtime;


pub fn main() -> Result<(), Box<dyn std::error::Error>>{
    let rt = Runtime::new()?; 

    Ok(())

}
