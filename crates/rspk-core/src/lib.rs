//! Core traits and types for the `pk` meta package manager.
//!
//! This crate provides the foundational abstractions used throughout
//! the project, including the [`PackageManager`] trait and [`Package`]
//! model.
//!
//! Needsfile parsing has been moved to the `rspk-needsfile` crate.
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod context;
pub mod error;
pub mod manager;
pub mod package;
pub mod platform;
pub mod progress;

pub use context::ExecutionContext;
pub use error::Error;
pub use manager::{Capabilities, PackageManager, Priority};
pub use package::Package;
pub use platform::Platform;
pub use progress::{NullProgressReporter, ProgressReporter, ProgressTask};
