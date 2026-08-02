//! Ruby Gems API client for Ruby package registry.

use reqwest::Client;
use serde::Deserialize;

/// Gem information from Ruby Gems API.
#[derive(Debug, Deserialize)]
pub struct Gem
{
    /// The gem name.
    pub name:         String,
    /// The latest version.
    pub version:      String,
    /// The gem description.
    #[serde(default)]
    pub info:         Option<String>,
    /// The gem homepage.
    #[serde(default)]
    pub homepage_uri: Option<String>,
}

/// Client for querying the Ruby Gems API.
#[derive(Debug, Clone)]
pub struct RubyGemsClient
{
    client:   Client,
    base_url: String,
}

impl RubyGemsClient
{
    /// Creates a new Ruby Gems API client.
    pub fn new() -> Self
    {
        Self {
            client:   Client::new(),
            base_url: "https://rubygems.org/api/v1".to_string(),
        }
    }

    /// Gets information about a specific gem.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or the response
    /// cannot be parsed.
    pub async fn get_gem(
        &self,
        gem_name: &str,
    ) -> Result<Option<Gem>, reqwest::Error>
    {
        let url = format!("{}/gems/{}.json", self.base_url, gem_name);
        let response = self.client.get(&url).send().await?;
        if response.status().is_success()
        {
            let gem: Gem = response.json().await?;
            Ok(Some(gem))
        }
        else
        {
            Ok(None)
        }
    }

    /// Checks if a gem exists on Ruby Gems.
    pub async fn exists(&self, gem_name: &str) -> Result<bool, reqwest::Error>
    {
        Ok(self.get_gem(gem_name).await?.is_some())
    }

    /// Gets the latest version of a gem.
    pub async fn latest_version(
        &self,
        gem_name: &str,
    ) -> Result<Option<String>, reqwest::Error>
    {
        Ok(self.get_gem(gem_name).await?.map(|g| g.version))
    }
}

impl Default for RubyGemsClient
{
    fn default() -> Self
    {
        Self::new()
    }
}
