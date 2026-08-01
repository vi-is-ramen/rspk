//! Pk core crate named librspk (`rspk-core` formerly).

#![deny(missing_docs, missing_debug_implementations)]

mod detect;
mod err;
mod exec;
mod manager;
mod resolve;
pub mod version;

pub use detect::*;
pub use err::*;
pub use exec::*;
pub use manager::*;
pub use resolve::*;
