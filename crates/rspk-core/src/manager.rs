//! Core traits defining package manager behavior.

use crate::context::ExecutionContext;
use crate::error::Error;
use crate::package::Package;
use crate::platform::Platform;
use serde::{Deserialize, Serialize};

/// Priority level for automatic manager selection.
///
/// Lower values indicate higher priority. When `--quiet` is enabled
/// and a package is available in multiple managers, the manager with
/// the lowest priority value is selected automatically.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deserialize,
    Serialize,
)]
pub enum Priority
{
    /// Highest priority: system-native package managers.
    /// Examples: apt, dnf, pacman, zypper, xbps, brew, macports.
    System    = 0,

    /// Medium priority: universal Linux managers.
    /// Examples: flatpak, snap.
    Universal = 1,

    /// Lower priority: language-specific managers.
    /// Examples: cargo, npm, gems, pip.
    #[default]
    Language  = 2,

    /// Lowest priority: AUR helpers and other niche managers.
    /// Examples: yay, paru.
    Auxiliary = 3,
}

/// Describes the capabilities supported by a package manager.
///
/// This struct is used to advertise which operations a manager
/// supports, allowing the CLI to disable unavailable commands.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
pub struct Capabilities
{
    /// Whether the manager can list installed packages.
    pub list_installed: bool,

    /// Whether the manager can detect outdated packages.
    pub detect_outdated: bool,

    /// Whether the manager supports searching packages.
    pub search: bool,

    /// Whether the manager supports installing packages.
    pub install: bool,

    /// Whether the manager supports upgrading individual packages.
    pub upgrade_one: bool,

    /// Whether the manager supports upgrading all packages at once.
    pub upgrade_all: bool,

    /// Whether the manager supports removing packages.
    pub remove: bool,

    /// Whether the manager supports synchronizing repositories.
    pub sync: bool,

    /// Whether the manager supports cleaning up caches.
    pub cleanup: bool,

    /// Whether the manager supports resolving abstract package names.
    pub resolve: bool,
}

/// Core trait that every package manager must implement.
///
/// This trait defines the common interface for interacting with
/// different package managers in a unified way.
#[async_trait::async_trait]
pub trait PackageManager: Send + Sync + std::fmt::Debug
{
    /// Returns the unique identifier of this manager.
    ///
    /// The ID is used for routing and must be stable across versions.
    fn id(&self) -> &'static str;

    /// Returns the human-readable name of this manager.
    fn name(&self) -> &'static str;

    /// Returns the CLI binary names used by this manager.
    ///
    /// Used for auto-detection on the system.
    fn cli_names(&self) -> &'static [&'static str];

    /// Returns the platforms supported by this manager.
    fn platforms(&self) -> &'static [Platform];

    /// Returns the capabilities supported by this manager.
    fn capabilities(&self) -> Capabilities;

    /// Returns the priority of this manager for automatic selection.
    ///
    /// Lower values indicate higher priority. Defaults to
    /// [`Priority::Language`] if not overridden.
    fn priority(&self) -> Priority
    {
        Priority::Language
    }

    /// Checks whether the manager is available on the current system.
    ///
    /// # Errors
    ///
    /// Returns an error if the check fails for reasons other than
    /// the manager simply not being installed.
    async fn is_available(&self) -> Result<bool, Error>;

    /// Lists all currently installed packages.
    ///
    /// # Errors
    ///
    /// Returns an error if the manager cannot list packages.
    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>;

    /// Detects packages that have newer versions available.
    ///
    /// # Errors
    ///
    /// Returns an error if the manager cannot detect outdated packages.
    async fn outdated(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>;

    /// Searches for packages matching the given query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string.
    /// * `extended` - Whether to search descriptions too.
    /// * `exact` - Whether to match the query exactly.
    ///
    /// # Errors
    ///
    /// Returns an error if the manager cannot perform the search.
    async fn search(
        &self,
        query: &str,
        extended: bool,
        exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>;

    /// Installs a package by ID, optionally at a specific version.
    ///
    /// # Errors
    ///
    /// Returns an error if installation fails.
    /// Resolves an abstract package name to a manager-specific package ID.
    ///
    /// This method attempts to find the corresponding package in this
    /// manager's ecosystem for a given abstract name. For example:
    /// - System managers (apt, pacman) use Repology API
    /// - Language managers (cargo, npm) query their respective registries
    /// - AUR helpers (yay) query the AUR directly
    ///
    /// # Arguments
    ///
    /// * `abstract_name` - The abstract package name to resolve
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(package_id))` if a match is found,
    /// `Ok(None)` if no match exists, or an error if resolution fails.
    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>;

    /// Installs a package by ID, optionally at a specific version.
    async fn install(
        &self,
        package_id: &str,
        version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>;

    /// Upgrades a specific package, or all packages if `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the upgrade fails.
    async fn upgrade(
        &self,
        package_id: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>;

    /// Removes a package by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if removal fails.
    async fn remove(
        &self,
        package_id: &str,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>;

    /// Synchronizes package repositories.
    ///
    /// # Errors
    ///
    /// Returns an error if synchronization fails.
    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>;

    /// Cleans up caches and temporary files.
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails.
    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>;
}
