//! Chocolatey package manager implementation for Windows.
//!
//! Chocolatey is one of the oldest and most popular package managers
//! for Windows. It requires administrator privileges for most operations.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;

/// The Chocolatey package manager for Windows.
///
/// Manages Windows packages with automatic installers. Requires
/// administrator privileges for installation and removal.
#[derive(Debug)]
pub struct Choco
{
    cli_path: String,
}

impl Choco
{
    /// Creates a new Chocolatey manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("choco")
            .map_err(|_| Error::BinaryNotFound("choco".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self { cli_path })
    }

    /// Parses the output of `choco list --local-only`.
    ///
    /// Format: `package version`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("Chocolatey v")
                    || trimmed.ends_with("packages installed.")
                {
                    return None;
                }

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2
                {
                    let id = parts[0].to_string();
                    let version = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "choco".to_string(),
                        arch: None,
                    })
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `choco outdated`.
    ///
    /// Format: `package|current|available|pinned?`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3
                {
                    let id = parts[0].trim().to_string();
                    let installed = parts[1].trim();
                    let latest = parts[2].trim();

                    if !id.is_empty() && !id.starts_with("Outdated")
                    {
                        Some(Package {
                            id,
                            name: None,
                            description: None,
                            installed_version: installed.parse().ok(),
                            latest_version: latest.parse().ok(),
                            manager_id: "choco".to_string(),
                            arch: None,
                        })
                    }
                    else
                    {
                        None
                    }
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `choco search`.
    ///
    /// Format: `package version [approved]`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("Chocolatey v")
                    || trimmed.ends_with("packages found.")
                    || trimmed.starts_with("Validation")
                {
                    return None;
                }

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2
                {
                    let id = parts[0].to_string();
                    let version = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "choco".to_string(),
                        arch: None,
                    })
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        let result = CommandBuilder::new(&self.cli_path)
            .arg("info")
            .arg(abstract_name)
            .run()
            .await;

        if let Ok(output) = result
        {
            if output.exit_code == 0 && output.stdout.contains(abstract_name)
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Choco
{
    fn id(&self) -> &'static str
    {
        "choco"
    }
    fn name(&self) -> &'static str
    {
        "Chocolatey"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["choco"]
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
            .map_err(|_| Error::ManagerUnavailable("choco".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--local-only")
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
            .arg("outdated")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        Ok(Self::parse_outdated_output(&output.stdout))
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        if ctx.dry_run
        {
            eprintln!("[dry-run] resolving '{abstract_name}' via choco info");
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
            .arg("-y")
            .arg(package_id)
            .with_sudo()
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
            .arg("-y")
            .with_sudo()
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            builder = builder.arg(id);
        }
        else
        {
            builder = builder.arg("all");
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
            .arg("-y")
            .arg(package_id)
            .with_sudo()
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

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_parse_installed_output()
    {
        let output = "Chocolatey v2.2.0\nchocolatey 2.2.0\ngit 2.42.0\n2 \
                      packages installed.\n";
        let packages = Choco::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
    }
}
