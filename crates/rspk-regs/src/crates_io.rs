//! crates.io API client for Rust package registry.

use reqwest::Client;
use serde::Deserialize;

/// Crate information from crates.io API.
#[derive(Debug, Deserialize)]
pub struct Crate
{
    /// The crate name.
    pub name:        String,
    /// The latest version.
    #[serde(rename = "max_version")]
    pub max_version: String,
    /// The crate description.
    pub description: Option<String>,
}

/// Response from crates.io API.
#[derive(Debug, Deserialize)]
pub struct CrateResponse
{
    /// The crate information.
    #[serde(rename = "crate")]
    pub crate_info: Crate,
}

/// Client for querying the crates.io API.
#[derive(Debug, Clone)]
pub struct CratesIoClient
{
    client:   Client,
    base_url: String,
}

impl CratesIoClient
{
    /// Creates a new crates.io API client.
    pub fn new() -> Self
    {
        Self {
            client:   Client::new(),
            base_url: "https://crates.io/api/v1".to_string(),
        }
    }

    /// Gets information about a specific crate.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response
    /// cannot be parsed.
    pub async fn get_crate(
        &self,
        crate_name: &str,
    ) -> Result<Option<Crate>, reqwest::Error>
    {
        let url = format!("{}/crates/{}", self.base_url, crate_name);
        let response = self.client.get(&url).send().await?;
        if response.status().is_success()
        {
            let crate_response: CrateResponse = response.json().await?;
            Ok(Some(crate_response.crate_info))
        }
        else
        {
            Ok(None)
        }
    }

    /// Checks if a crate exists on crates.io.
    pub async fn exists(&self, crate_name: &str)
    -> Result<bool, reqwest::Error>
    {
        Ok(self.get_crate(crate_name).await?.is_some())
    }

    /// Gets the latest version of a crate.
    pub async fn latest_version(
        &self,
        crate_name: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        Ok(self.get_crate(crate_name).await?.map(|c| c.max_version))
    }
}

impl Default for CratesIoClient
{
    fn default() -> Self
    {
        Self::new()
    }
}
