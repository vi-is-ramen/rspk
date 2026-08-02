//! npm package manager implementation.
//!
//! This module provides support for managing global npm packages.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use serde::Deserialize;
use std::collections::HashMap;

use rspk_executor::CommandBuilder;

/// The npm package manager.
///
/// Supports global package management operations.
#[derive(Debug)]
pub struct Npm
{
    cli_path: String,
}

impl Npm
{
    /// Creates a new npm manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `npm` binary cannot be found.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("npm")
            .map_err(|_| Error::BinaryNotFound("npm".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self { cli_path })
    }
}

#[async_trait]
impl PackageManager for Npm
{
    fn id(&self) -> &'static str
    {
        "npm"
    }

    fn name(&self) -> &'static str
    {
        "npm"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["npm"]
    }

    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Linux, Platform::MacOS, Platform::Windows]
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
            cleanup:         true,
            resolve:         true,
        }
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("--version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("npm".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        #[derive(Deserialize)]
        struct NpmPackage
        {
            version:     String,
            description: Option<String>,
        }
        #[derive(Deserialize)]
        struct NpmOutput
        {
            dependencies: Option<HashMap<String, NpmPackage>>,
        }
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("-g")
            .arg("--json")
            .arg("--depth=0")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        let data: NpmOutput = serde_json::from_str(&output.stdout)
            .map_err(|e| Error::ParseError(e.to_string()))?;
        let dependencies = data.dependencies.unwrap_or_default();
        Ok(dependencies
            .into_iter()
            .map(|(id, pkg)| Package {
                id,
                name: None,
                description: pkg.description,
                installed_version: pkg.version.parse().ok(),
                latest_version: None,
                manager_id: self.id().to_string(),
                arch: None,
            })
            .collect())
    }

    async fn outdated(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        #[derive(Deserialize)]
        struct OutdatedPkg
        {
            current: String,
            latest:  String,
        }
        let result = CommandBuilder::new(&self.cli_path)
            .arg("outdated")
            .arg("-g")
            .arg("--json")
            .dry_run(ctx.dry_run)
            .run()
            .await;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        let output = match result
        {
            Ok(output) => output,
            Err(rspk_executor::ExecutionError::NonZeroExit {
                stderr, ..
            }) =>
            {
                return Err(Error::ExecutionError(stderr));
            },
            Err(e) => return Err(Error::ExecutionError(e.to_string())),
        };
        let data: HashMap<String, OutdatedPkg> =
            serde_json::from_str(&output.stdout)
                .map_err(|e| Error::ParseError(e.to_string()))?;
        Ok(data
            .into_iter()
            .map(|(id, pkg)| Package {
                id,
                name: None,
                description: None,
                installed_version: pkg.current.parse().ok(),
                latest_version: pkg.latest.parse().ok(),
                manager_id: self.id().to_string(),
                arch: None,
            })
            .collect())
    }

    async fn search(
        &self,
        query: &str,
        _extended: bool,
        _exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        #[derive(Deserialize)]
        struct SearchResult
        {
            name:        String,
            version:     String,
            description: Option<String>,
        }

        let output = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg(query)
            .arg("--json")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        let results: Vec<SearchResult> =
            serde_json::from_str(&output.stdout)
                .map_err(|e| Error::ParseError(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| Package {
                id:                r.name,
                name:              None,
                description:       r.description,
                installed_version: None,
                latest_version:    r.version.parse().ok(),
                manager_id:        self.id().to_string(),
                arch:              None,
            })
            .collect())
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        // Query npm registry to check if package exists
        let output = CommandBuilder::new(&self.cli_path)
            .arg("view")
            .arg(abstract_name)
            .arg("name")
            .dry_run(ctx.dry_run)
            .run()
            .await;

        if let Ok(out) = output
        {
            if !out.stdout.trim().is_empty()
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }

    async fn install(
        &self,
        package_id: &str,
        version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let pkg_spec = if let Some(v) = version
        {
            format!("{package_id}@{v}")
        }
        else
        {
            package_id.to_string()
        };

        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("-g")
            .arg(&pkg_spec)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn upgrade(
        &self,
        package_id: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("update")
            .arg("-g")
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            builder = builder.arg(id);
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
            .arg("-g")
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        let _ = ctx;
        // npm doesn't require explicit sync
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("cache")
            .arg("clean")
            .arg("--force")
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }
}
