//! Business logic layer for the `pk` meta package manager.
//!
//! This crate sits between the user-facing interfaces (CLI, RPC)
//! and the low-level core/manager/executor crates. It contains
//! cross-cutting business logic that is shared by multiple
//! interfaces:
//!
//! - [`resolver`] — resolving abstract package names to concrete manager +
//!   package-ID pairs across all available managers.
//! - [`satisfy`] — the Needsfile satisfaction pipeline (parse, resolve, install
//!   in parallel).
//!
//! Interfaces call into this crate and then format the results
//! for their specific output channel (terminal, JSON-RPC, etc.).
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::unused_async)]

pub mod resolver;
pub mod satisfy;

pub use resolver::{Candidate, find_candidates, split_package_spec};
pub use satisfy::{
    ManagerSelector, PrioritySelector, SatisfyResult, SatisfySummary, satisfy,
};
