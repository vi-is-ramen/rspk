//! AUR (Arch User Repository) API client.
//!
//! The AUR provides community-maintained packages for Arch Linux.

use reqwest::Client;
use serde::Deserialize;

/// AUR package information from the RPC API.
#[derive(Debug, Deserialize)]
pub struct AurPackage
{
    /// The package name.
    #[serde(rename = "Name")]
    pub name:        String,
    /// The package version.
    #[serde(rename = "Version")]
    pub version:     String,
    /// The package description.
    #[serde(rename = "Description")]
    pub description: Option<String>,
    /// The number of votes.
    #[serde(rename = "NumVotes")]
    pub num_votes:   Option<i32>,
    /// The popularity score.
    #[serde(rename = "Popularity")]
    pub popularity:  Option<f64>,
}

/// Response from AUR RPC API.
#[derive(Debug, Deserialize)]
struct AurRpcResponse
{
    #[allow(unused)]
    #[serde(rename = "resultcount")]
    result_count: i32,
    results:      Vec<AurPackage>,
}

/// Client for querying the AUR RPC API.
#[derive(Debug, Clone)]
pub struct AurClient
{
    client:   Client,
    base_url: String,
}

impl AurClient
{
    /// Creates a new AUR API client.
    pub fn new() -> Self
    {
        Self {
            client:   Client::new(),
            base_url: "https://aur.archlinux.org/rpc".to_string(),
        }
    }

    /// Searches for packages in the AUR.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response
    /// cannot be parsed.
    pub async fn search(
        &self,
        query: &str,
    ) -> Result<Vec<AurPackage>, reqwest::Error>
    {
        let url = format!(
            "{}?v=5&type=search&arg={}",
            self.base_url,
            urlencoding::encode(query)
        );
        let response = self.client.get(&url).send().await?;
        let rpc_response: AurRpcResponse = response.json().await?;
        Ok(rpc_response.results)
    }

    /// Gets information about a specific AUR package.
    pub async fn info(
        &self,
        package_name: &str,
    ) -> Result<Option<AurPackage>, reqwest::Error>
    {
        let url = format!(
            "{}?v=5&type=info&arg[]={}",
            self.base_url,
            urlencoding::encode(package_name)
        );
        let response = self.client.get(&url).send().await?;
        let rpc_response: AurRpcResponse = response.json().await?;
        Ok(rpc_response.results.into_iter().next())
    }

    /// Checks if a package exists in the AUR.
    pub async fn exists(
        &self,
        package_name: &str,
    ) -> Result<bool, reqwest::Error>
    {
        Ok(self.info(package_name).await?.is_some())
    }
}

impl Default for AurClient
{
    fn default() -> Self
    {
        Self::new()
    }
}
