//! Winget package manager implementation for Windows.
//!
//! Winget is the official Windows Package Manager developed by
//! Microsoft. It provides a command-line tool for installing
//! applications from the Windows Package Manager repository.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use serde::Deserialize;

/// The Winget package manager for Windows.
#[derive(Debug)]
pub struct Winget
{
    cli_path: String,
}

impl Winget
{
    /// Creates a new Winget manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("winget")
            .map_err(|_| Error::BinaryNotFound("winget".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self { cli_path })
    }

    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        #[derive(Deserialize)]
        struct WingetPackage
        {
            #[serde(alias = "Id", alias = "id")]
            id:        Option<String>,
            #[serde(alias = "Name", alias = "name")]
            name:      Option<String>,
            #[serde(alias = "Version", alias = "version")]
            version:   Option<String>,
            #[serde(alias = "Available", alias = "available")]
            available: Option<String>,
        }

        let packages: Vec<WingetPackage> = match serde_json::from_str(output)
        {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        packages
            .into_iter()
            .filter_map(|p| {
                let id = p.id?;
                let name = p.name.unwrap_or_else(|| id.clone());

                Some(Package {
                    id,
                    name: Some(name),
                    description: None,
                    installed_version: p.version.and_then(|v| v.parse().ok()),
                    latest_version: p.available.and_then(|v| v.parse().ok()),
                    manager_id: "winget".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    fn parse_search_output(output: &str) -> Vec<Package>
    {
        #[derive(Deserialize)]
        struct SearchResult
        {
            #[serde(alias = "Id", alias = "id")]
            id:      Option<String>,
            #[serde(alias = "Name", alias = "name")]
            name:    Option<String>,
            #[serde(alias = "Version", alias = "version")]
            version: Option<String>,
        }

        let results: Vec<SearchResult> = match serde_json::from_str(output)
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        results
            .into_iter()
            .filter_map(|r| {
                let id = r.id?;
                Some(Package {
                    name: r.name,
                    id,
                    description: None,
                    installed_version: None,
                    latest_version: r.version.and_then(|v| v.parse().ok()),
                    manager_id: "winget".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        let result = CommandBuilder::new(&self.cli_path)
            .arg("show")
            .arg("--id")
            .arg(abstract_name)
            .arg("--accept-source-agreements")
            .run()
            .await;

        if let Ok(output) = result
        {
            if output.exit_code == 0
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        // Try searching
        let search_result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg(abstract_name)
            .arg("--accept-source-agreements")
            .run()
            .await;

        if let Ok(output) = search_result
        {
            if output.stdout.contains(abstract_name)
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Winget
{
    fn id(&self) -> &'static str
    {
        "winget"
    }
    fn name(&self) -> &'static str
    {
        "Winget"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["winget"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Windows]
    }

    fn capabilities(&self) -> Capabilities
    {
        Capabilities {
            list_installed:  true,
            detect_outdated: true,
            search:          true,
            install:         true,
            upgrade_one:     true,
            upgrade_all:     true,
            remove:          true,
            sync:            false,
            cleanup:         false,
            resolve:         true,
        }
    }

    fn priority(&self) -> Priority
    {
        Priority::System
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("--version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("winget".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--accept-source-agreements")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        Ok(Self::parse_installed_output(&output.stdout))
    }

    async fn outdated(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("upgrade")
            .arg("--accept-source-agreements")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        // winget upgrade shows upgradeable packages in the list
        Ok(Self::parse_installed_output(&output.stdout))
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        if ctx.dry_run
        {
            eprintln!("[dry-run] resolving '{abstract_name}' via winget show",);
            return Ok(Some(abstract_name.to_string()));
        }
        self.resolve_internal(abstract_name).await
    }

    async fn search(
        &self,
        query: &str,
        _extended: bool,
        _exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg(query)
            .arg("--accept-source-agreements")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        Ok(Self::parse_search_output(&output.stdout))
    }

    async fn install(
        &self,
        package_id: &str,
        version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("--id")
            .arg(package_id)
            .arg("--accept-source-agreements")
            .arg("--accept-package-agreements")
            .arg("--silent")
            .dry_run(ctx.dry_run);

        if let Some(v) = version
        {
            builder = builder.arg("--version").arg(v);
        }

        let output = builder.run().await?;
        Ok(output.stdout)
    }

    async fn upgrade(
        &self,
        package_id: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("upgrade")
            .arg("--accept-source-agreements")
            .arg("--accept-package-agreements")
            .arg("--silent")
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            builder = builder.arg("--id").arg(id);
        }
        else
        {
            builder = builder.arg("--all");
        }

        let output = builder.run().await?;
        Ok(output.stdout)
    }

    async fn remove(
        &self,
        package_id: &str,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("uninstall")
            .arg("--id")
            .arg(package_id)
            .arg("--silent")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, _ctx: &ExecutionContext) -> Result<(), Error>
    {
        Ok(())
    }

    async fn cleanup(&self, _ctx: &ExecutionContext) -> Result<(), Error>
    {
        Ok(())
    }
}
