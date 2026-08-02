//! Scoop package manager implementation for Windows.
//!
//! Scoop is a command-line installer for Windows inspired by
//! Homebrew. It installs programs to user-writable directories
//! and does not require administrator privileges.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;

/// The Scoop package manager for Windows.
///
/// Installs applications to user-writable locations and manages
/// PATH automatically. Does not require administrator privileges.
#[derive(Debug)]
pub struct Scoop
{
    cli_path: String,
}

impl Scoop
{
    /// Creates a new Scoop manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("scoop")
            .map_err(|_| Error::BinaryNotFound("scoop".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self { cli_path })
    }

    /// Parses the output of `scoop list`.
    ///
    /// Format: `Name    Version    Source    Updated    Info`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with("Installed apps")
                    || trimmed.starts_with("---")
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
                        manager_id: "scoop".to_string(),
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

    /// Parses the output of `scoop status`.
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let mut packages = Vec::new();
        let mut in_updates = false;

        for line in output.lines()
        {
            let trimmed = line.trim();

            if trimmed.starts_with("Updates are available for:")
            {
                in_updates = true;
                continue;
            }

            if in_updates
            {
                if trimmed.is_empty() || trimmed.starts_with("---")
                {
                    in_updates = false;
                    continue;
                }

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3
                {
                    let id = parts[0].to_string();
                    let installed = parts[1];
                    let latest = parts[2];

                    packages.push(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "scoop".to_string(),
                        arch: None,
                    });
                }
            }
        }

        packages
    }

    /// Parses the output of `scoop search`.
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        let mut packages = Vec::new();
        let re = regex::Regex::new(r"^'([^']+)' \(([^)]+)\) includes:").ok();

        for line in output.lines()
        {
            if let Some(re) = &re
            {
                if let Some(caps) = re.captures(line)
                {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let version = caps.get(2).unwrap().as_str();

                    packages.push(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "scoop".to_string(),
                        arch: None,
                    });
                }
            }
        }

        packages
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
            if output.exit_code == 0
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Scoop
{
    fn id(&self) -> &'static str
    {
        "scoop"
    }
    fn name(&self) -> &'static str
    {
        "Scoop"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["scoop"]
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
            sync:            true,
            cleanup:         true,
            resolve:         true,
        }
    }

    fn priority(&self) -> Priority
    {
        Priority::Universal
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("--version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("scoop".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
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
            .arg("status")
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
            eprintln!("[dry-run] resolving '{abstract_name}' via scoop info",);
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
        _version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg(package_id)
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
        let output = if let Some(id) = package_id
        {
            CommandBuilder::new(&self.cli_path)
                .arg("update")
                .arg(id)
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new(&self.cli_path)
                .arg("update")
                .arg("*")
                .dry_run(ctx.dry_run)
                .run()
                .await?
        };

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
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("update")
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("cleanup")
            .arg("*")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        CommandBuilder::new(&self.cli_path)
            .arg("cache")
            .arg("rm")
            .arg("*")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

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
        let output = "Installed apps matching:\n\n  git        2.42.0  main  \
                      2024-01-01\n";
        let packages = Scoop::parse_installed_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "git");
    }
}
