//! Mac Ports package manager implementation.
//!
//! This module provides support for managing packages via Mac Ports,
//! a package manager for macOS that provides a command-line interface
//! for installing open-source software. Unlike Homebrew, Mac Ports
//! requires sudo for most operations and installs to `/opt/local`.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The Mac Ports package manager.
///
/// Manages packages installed via Mac Ports on macOS. Most operations
/// require sudo privileges and will automatically elevate permissions
/// when needed.
#[derive(Debug)]
pub struct MacPorts
{
    cli_path: String,
    repology: RepologyClient,
}

impl MacPorts
{
    /// Creates a new Mac Ports manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `port` binary cannot be found on the system.
    pub async fn new() -> Result<Self, Error>
    {
        // MacPorts installs to /opt/local/bin by default
        let cli_path = which::which("port")
            .map_err(|_| Error::BinaryNotFound("port".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `port installed`.
    ///
    /// Format:
    /// ```text
    /// The following ports are currently installed:
    ///   curl @8.4.0_0 (active)
    ///   wget @1.21.4_0 (active)
    /// ```
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        let re =
            regex::Regex::new(r"^\s+(\S+)\s+@([^_]+)(?:_\S+)?\s+\(active\)")
                .unwrap();

        output
            .lines()
            .filter_map(|line| {
                re.captures(line).map(|caps| {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let version = caps.get(2).unwrap().as_str();

                    Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "macports".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `port outdated`.
    ///
    /// Format:
    /// ```text
    /// The following ports have newer versions available:
    ///   curl                           8.3.0 < 8.4.0
    ///   wget                           1.21.3 < 1.21.4
    /// ```
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"^\s+(\S+)\s+(\S+)\s+<\s+(\S+)").unwrap();

        output
            .lines()
            .filter_map(|line| {
                re.captures(line).map(|caps| {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let installed = caps.get(2).unwrap().as_str();
                    let latest = caps.get(3).unwrap().as_str();

                    Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "macports".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `port search`.
    ///
    /// Format: `name    description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("Found")
                {
                    return None;
                }

                let mut parts = trimmed.splitn(2, char::is_whitespace);
                let id = parts.next()?.trim().to_string();
                let description = parts.next().map(|d| d.trim().to_string());

                if id.is_empty()
                {
                    None
                }
                else
                {
                    Some(Package {
                        id,
                        name: None,
                        description,
                        installed_version: None,
                        latest_version: None,
                        manager_id: "macports".to_string(),
                        arch: None,
                    })
                }
            })
            .collect()
    }

    /// Internal resolution logic without dry-run handling.
    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        // Try Repology first
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "macports")
            .await
        {
            return Ok(Some(name));
        }

        // Fallback: try port info directly
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
impl PackageManager for MacPorts
{
    fn id(&self) -> &'static str
    {
        "macports"
    }

    fn name(&self) -> &'static str
    {
        "MacPorts"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["port"]
    }

    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::MacOS]
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

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("port".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("installed")
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
            eprintln!(
                "[dry-run] resolving '{abstract_name}' via Repology and port \
                 info"
            );
            return Ok(Some(abstract_name.to_string()));
        }

        self.resolve_internal(abstract_name).await
    }

    async fn search(
        &self,
        query: &str,
        extended: bool,
        _exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .dry_run(ctx.dry_run);

        if extended
        {
            // --line gives more detailed output
            builder = builder.arg("--line");
        }

        builder = builder.arg(query);

        let output = builder.run().await?;

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
        // MacPorts does not support installing specific versions via CLI.
        // MacPorts requires sudo for installation.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg(package_id)
            .with_sudo()
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
            .arg("upgrade")
            .with_sudo()
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            builder = builder.arg(id);
        }
        else
        {
            // upgrade all outdated
            builder = builder.arg("outdated");
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
        // --follow-dependencies also removes unneeded dependencies
        let output = CommandBuilder::new(&self.cli_path)
            .arg("uninstall")
            .arg("--follow-dependencies")
            .arg(package_id)
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // selfupdate updates both MacPorts itself and the ports tree
        CommandBuilder::new(&self.cli_path)
            .arg("selfupdate")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Remove inactive ports (old versions)
        CommandBuilder::new(&self.cli_path)
            .arg("uninstall")
            .arg("inactive")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Clean downloaded distfiles and build artifacts
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
            .arg("--all")
            .arg("installed")
            .with_sudo()
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
        let output = r#"The following ports are currently installed:
  curl @8.4.0_0 (active)
  curl @8.3.0_0
  wget @1.21.4_0 (active)
  ripgrep @14.1.0_0 (active)
"#;

        let packages = MacPorts::parse_installed_output(output);
        // Only active ones should be included
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "8.4.0"
        );

        assert_eq!(packages[1].id, "wget");
        assert_eq!(packages[2].id, "ripgrep");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"The following ports have newer versions available:
  curl                           8.3.0 < 8.4.0
  wget                           1.21.3 < 1.21.4
"#;

        let packages = MacPorts::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "8.3.0"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "8.4.0"
        );

        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = r#"Found 3 ports:
curl    Tool for transferring files with URL syntax
wget    Internet file retriever
ripgrep    Recursively search directories for a regex pattern
"#;

        let packages = MacPorts::parse_search_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some("Tool for transferring files with URL syntax".to_string())
        );
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = MacPorts::parse_installed_output("");
        assert_eq!(packages.len(), 0);

        let packages = MacPorts::parse_outdated_output("");
        assert_eq!(packages.len(), 0);
    }
}
