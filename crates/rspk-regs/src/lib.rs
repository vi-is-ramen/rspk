//! API clients for package registries.
//!
//! This crate provides HTTP clients for querying external package
//! registries (Repology, crates.io, AUR, RubyGems). These clients
//! are consumed by package manager implementations in `rspk-managers`
//! to resolve abstract package names to manager-specific identifiers.
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod aur;
pub mod crates_io;
pub mod repology;
pub mod rubygems;

pub use aur::AurClient;
pub use crates_io::CratesIoClient;
pub use repology::RepologyClient;
pub use rubygems::RubyGemsClient;
