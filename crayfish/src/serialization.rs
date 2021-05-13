extern crate bincode;

pub use bincode::serialize_into;
pub use bincode::deserialize_from;

pub extern crate serde;
pub use serde::Deserialize;
pub use serde::Serialize;
