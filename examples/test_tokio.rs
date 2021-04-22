use tokio::runtime::Runtime;


pub fn main() -> Result<(), Box<dyn std::error::Error>>{
    let _rt = Runtime::new()?; 

    Ok(())

}
