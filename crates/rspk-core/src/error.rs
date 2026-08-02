//! Error types for the meta package manager.

use thiserror::Error;

/// Top-level error type for the `pk` project.
#[derive(Debug, Error)]
pub enum Error
{
    /// The requested package was not found.
    #[error("package '{0}' not found")]
    NotFound(String),

    /// The package manager is not available on the current system.
    #[error("package manager '{0}' is not available")]
    ManagerUnavailable(String),

    /// The current platform is not supported.
    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),

    /// A command execution failed.
    #[error("command execution failed: {0}")]
    ExecutionError(String),

    /// A command timed out.
    #[error("command timed out")]
    Timeout,

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse command output.
    #[error("failed to parse output: {0}")]
    ParseError(String),

    /// JSON parsing failed.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// The CLI binary was not found.
    #[error("CLI binary not found: {0}")]
    BinaryNotFound(String),

    /// The requested capability is not supported.
    #[error("capability not supported: {0}")]
    UnsupportedCapability(String),
}
