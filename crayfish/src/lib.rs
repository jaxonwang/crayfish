#[macro_use]
mod utils;

pub mod activity; // TODO private
pub mod args;
pub mod collective;
pub mod essence;
mod executor;
mod finish;
mod gasnet;
pub mod global_id; // TODO: private
pub mod logging; // TODO: mark as private
mod meta_data;
pub mod network; // TODO: mark private
pub mod place;
pub mod runtime;
pub mod runtime_meta;
mod serialization;
pub mod shared;
#[cfg(feature = "trace")]
pub mod trace;

mod prelude {
    pub use crate::executor::runtime;
    pub use crate::executor::spawn;
    pub use crate::runtime_meta::inventory;
    pub use crate::serialization::serde;
    pub use crate::serialization::Deserialize;
    pub use crate::serialization::DeserializeOwned;
    pub use crate::serialization::Serialize;
}

pub mod re_export {
    pub use futures;
    pub use once_cell;
}

pub use crate::prelude::*;
pub use crayfish_macros::*;
pub use crayfish_trace_macros::*;
