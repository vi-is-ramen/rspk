//! JSON-RPC 2.0 server for `pk`.
//!
//! Provides a newline-delimited JSON-RPC interface over stdio,
//! allowing scripts and GUI applications to drive `pk` as a
//! long-running subprocess.
//!
//! # Protocol
//!
//! Each message is a single JSON object terminated by `\n`.
//! Implements [JSON-RPC 2.0](https://www.jsonrpc.org/specification)
//! including batch requests (arrays of requests).
//!
//! # Example session
//!
//! ```text
//! >>> {"jsonrpc":"2.0","method":"inventory","id":1}
//! <<< {"jsonrpc":"2.0","result":{"managers":[...]},"id":1}
//!
//! >>> {"jsonrpc":"2.0","method":"install","params":{"package":"ripgrep"},"id":2}
//! <<< {"jsonrpc":"2.0","result":{"installed":true,"output":"..."},"id":2}
//! ```

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

mod methods;
mod protocol;
mod server;

pub use methods::MethodRegistry;
pub use protocol::{ErrorCode, Id, Request, Response, RpcError, Value};
pub use server::Server;
