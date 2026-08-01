use thiserror::Error;

/// Represents librspk error.
#[derive(Error, Debug)]
pub enum Error
{
    /// No package manager found on this system.
    #[error("No package manager found on this system")]
    NoManagerFound,

    /// Preferred manager is not available.
    #[error("Preferred manager {0} is not available")]
    ManagerNotAvailable(String),

    /// Failed to read OS release file.
    #[error("Failed to read OS release file: {0}")]
    OsReleaseRead(std::io::Error),

    /// Failed to parse OS release file.
    #[error("Failed to parse OS release file")]
    OsReleaseParse,

    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Command execution failed.
    #[error("Command execution failed: {0}")]
    Command(String),

    /// Unsupported package manager.
    #[error("Unsupported package manager")]
    UnsupportedManager,

    /// Cache error.
    #[error("Cache error: {0}")]
    Cache(String),
}

/// Represents result of librspk's action.
pub type Result<T> = std::result::Result<T, Error>;
