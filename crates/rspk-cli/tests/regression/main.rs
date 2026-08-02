//! Regression test suite for `pk`.
//!
//! These tests are **not** part of the default `cargo test` run.
//! They are gated behind the `regression` feature and compiled as a
//! dedicated test target (`--test regression`).
//!
//! Run with:
//!
//! ```shell
//! cargo test -p rspk-cli --features regression --test regression
//! ```
//!
//! Each module targets a specific subsystem whose behaviour must
//! remain stable across releases.

mod cli_smoke;
mod needsfile;
mod package_spec;
mod rpc_protocol;
