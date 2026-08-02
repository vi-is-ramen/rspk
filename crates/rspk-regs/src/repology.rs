//! Repology API client for resolving package names across distributions.
//!
//! Repology (<https://repology.org>) tracks package versions across many
//! package managers and distributions, providing a unified view of the
//! software ecosystem.

use reqwest::Client;
use serde::Deserialize;

/// A package entry from Repology API.
#[derive(Debug, Deserialize)]
pub struct RepologyPackage
{
    /// The package name in this specific repository.
    pub name:    String,
    /// The repository/distribution identifier.
    pub repo:    String,
    /// The version of the package.
    pub version: String,
    /// The status of the package (e.g., "newest", "outdated").
    pub status:  Option<String>,
}

/// Client for querying the Repology API.
#[derive(Debug, Clone)]
pub struct RepologyClient
{
    client: Client,
}

impl RepologyClient
{
    /// Creates a new Repology API client.
    pub fn new() -> Self
    {
        Self {
            client: Client::new(),
        }
    }

    /// Queries Repology for a package name across all repositories.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response
    /// cannot be parsed.
    pub async fn query(
        &self,
        package_name: &str,
    ) -> Result<Vec<RepologyPackage>, reqwest::Error>
    {
        let url = format!("https://repology.org/api/v1/project/{package_name}");
        let response = self.client.get(&url).send().await?;
        let packages: Vec<RepologyPackage> = response.json().await?;
        Ok(packages)
    }

    /// Resolves a package name for a specific repository type.
    ///
    /// # Arguments
    ///
    /// * `package_name` - The abstract package name to resolve
    /// * `repo_prefix` - Repository prefix to match (e.g., "ubuntu", "debian",
    ///   "arch")
    ///
    /// # Returns
    ///
    /// Returns the package name in the matching repository, or None if not
    /// found.
    pub async fn resolve_for_repo(
        &self,
        package_name: &str,
        repo_prefix: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        let packages = self.query(package_name).await?;
        for pkg in packages
        {
            if pkg.repo.starts_with(repo_prefix)
            {
                return Ok(Some(pkg.name));
            }
        }
        Ok(None)
    }

    /// Resolves a package name for Debian/Ubuntu-based systems.
    pub async fn resolve_debian(
        &self,
        package_name: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        self.resolve_for_repo(package_name, "debian").await
    }

    /// Resolves a package name for Ubuntu-based systems.
    pub async fn resolve_ubuntu(
        &self,
        package_name: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        self.resolve_for_repo(package_name, "ubuntu").await
    }

    /// Resolves a package name for Arch Linux.
    pub async fn resolve_arch(
        &self,
        package_name: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        if let Some(name) = self.resolve_for_repo(package_name, "arch").await?
        {
            return Ok(Some(name));
        }
        self.resolve_for_repo(package_name, "aur").await
    }
}

impl Default for RepologyClient
{
    fn default() -> Self
    {
        Self::new()
    }
}
