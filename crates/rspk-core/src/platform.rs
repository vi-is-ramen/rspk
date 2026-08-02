//! Platform definitions for package manager compatibility.

use serde::{Deserialize, Serialize};

/// Operating system platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform
{
    /// Linux operating system.
    Linux,
    /// macOS operating system.
    MacOS,
    /// Windows operating system.
    Windows,
    /// FreeBSD operating system.
    FreeBSD,
    /// OpenBSD operating system.
    OpenBSD,
    /// NetBSD operating system.
    NetBSD,
    /// DragonFly BSD operating system.
    DragonFly,
    /// Android operating system (including Termux environment).
    Android,
}

impl Platform
{
    /// Returns the platform corresponding to the current system.
    ///
    /// # Errors
    ///
    /// Returns an error if the current platform is not supported.
    pub fn current() -> Result<Self, crate::Error>
    {
        let os = std::env::consts::OS;
        match os
        {
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::MacOS),
            "windows" => Ok(Self::Windows),
            "freebsd" => Ok(Self::FreeBSD),
            "openbsd" => Ok(Self::OpenBSD),
            "netbsd" => Ok(Self::NetBSD),
            "dragonfly" => Ok(Self::DragonFly),
            "android" => Ok(Self::Android),
            _ => Err(crate::Error::UnsupportedPlatform(os.to_string())),
        }
    }
}

impl std::fmt::Display for Platform
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            Self::Linux => write!(f, "Linux"),
            Self::MacOS => write!(f, "macOS"),
            Self::Windows => write!(f, "Windows"),
            Self::FreeBSD => write!(f, "FreeBSD"),
            Self::OpenBSD => write!(f, "OpenBSD"),
            Self::NetBSD => write!(f, "NetBSD"),
            Self::DragonFly => write!(f, "DragonFly BSD"),
            Self::Android => write!(f, "Android"),
        }
    }
}
