//! Snap package manager implementation for Ubuntu and derivatives.
//!
//! Snap is Canonical's universal package manager for Linux. Snaps are
//! containerized applications that work across different Linux
//! distributions.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;

/// The Snap package manager.
///
/// Manages containerized applications across Linux distributions.
/// Does not require sudo for user-level operations, but uses sudo
/// for system-level installations.
#[derive(Debug)]
pub struct Snap
{
    cli_path: String,
}

impl Snap
{
    /// Creates a new Snap manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("snap")
            .map_err(|_| Error::BinaryNotFound("snap".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self { cli_path })
    }

    /// Parses the output of `snap list`.
    ///
    /// Format: `Name    Version    Rev    Tracking    Publisher    Notes`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .skip(1) // skip header
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let id = parts[0].to_string();
                    let version = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "snap".to_string(),
                        arch: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `snap refresh --list`.
    ///
    /// Format: `Name    Version    Rev    Size    Publisher    Notes`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .skip(1) // skip header
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let id = parts[0].to_string();
                    let latest = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: None,
                        latest_version: latest.parse().ok(),
                        manager_id: "snap".to_string(),
                        arch: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `snap find`.
    ///
    /// Format: `Name    Version    Publisher    Notes    Summary`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .skip(1) // skip header
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    let id = parts[0].to_string();
                    let version = parts[1];
                    let description = parts[4..].join(" ");

                    Some(Package {
                        id,
                        name: None,
                        description: Some(description),
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "snap".to_string(),
                        arch: None,
                    })
                } else {
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
            if output.exit_code == 0
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Snap
{
    fn id(&self) -> &'static str
    {
        "snap"
    }
    fn name(&self) -> &'static str
    {
        "Snap"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["snap"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Linux]
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
        Priority::Universal
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("--version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("snap".to_string()))
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
            .arg("refresh")
            .arg("--list")
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
            eprintln!("[dry-run] resolving '{abstract_name}' via snap info",);
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
            .arg("find")
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
            .with_sudo()
            .dry_run(ctx.dry_run);

        if let Some(v) = version
        {
            // Use channel for specific version/track
            builder = builder.arg("--channel").arg(v);
        }

        builder = builder.arg(package_id);

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
            .arg("refresh")
            .with_sudo()
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
            .arg("remove")
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
        // Snap does not have a native cleanup command
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
        let output = "Name    Version    Rev    Tracking    Publisher    \
                      Notes\nfirefox 120.0-1    1234   latest/stable    \
                      mozilla✓    -\n";
        let packages = Snap::parse_installed_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "firefox");
    }
}
