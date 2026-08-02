//! Package model representing an installed or available software package.

use semver::Version;
use serde::{Deserialize, Serialize};

/// A software package managed by a package manager.
///
/// This struct represents either an installed package (with
/// [`installed_version`](Self::installed_version) set) or an available
/// package (with [`latest_version`](Self::latest_version) set).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package
{
    /// Unique identifier of the package within its manager.
    pub id: String,

    /// Human-readable name of the package, if different from the ID.
    pub name: Option<String>,

    /// Description of the package.
    pub description: Option<String>,

    /// Currently installed version, if the package is installed.
    pub installed_version: Option<Version>,

    /// Latest available version from the registry.
    pub latest_version: Option<Version>,

    /// Identifier of the package manager that manages this package.
    pub manager_id: String,

    /// Architecture or target platform, if applicable.
    pub arch: Option<String>,
}

impl Package
{
    /// Creates a new package with the given ID and manager.
    ///
    /// # Examples
    ///
    /// ```
    /// use rspk_core::Package;
    ///
    /// let pkg = Package::new("lodash", "npm");
    /// assert_eq!(pkg.id, "lodash");
    /// assert_eq!(pkg.manager_id, "npm");
    /// ```
    pub fn new(id: impl Into<String>, manager_id: impl Into<String>) -> Self
    {
        Self {
            id:                id.into(),
            name:              None,
            description:       None,
            installed_version: None,
            latest_version:    None,
            manager_id:        manager_id.into(),
            arch:              None,
        }
    }

    /// Returns `true` if the package has an available update.
    ///
    /// A package is considered outdated if both
    /// [`installed_version`](Self::installed_version) and
    /// [`latest_version`](Self::latest_version) are set, and the
    /// installed version is strictly less than the latest.
    pub fn is_outdated(&self) -> bool
    {
        match (&self.installed_version, &self.latest_version)
        {
            (Some(installed), Some(latest)) => installed < latest,
            _ => false,
        }
    }

    /// Returns the display name for the package.
    ///
    /// If [`name`](Self::name) is set, returns it; otherwise returns
    /// the [`id`](Self::id).
    pub fn display_name(&self) -> &str
    {
        self.name.as_deref().unwrap_or(&self.id)
    }
}
