//! pacman package manager implementation for Arch Linux and derivatives.
//!
//! This module provides support for managing packages via pacman,
//! the default package manager for Arch Linux, Manjaro, Endeavour OS,
//! and other Arch-based distributions.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;

/// The pacman package manager.
///
/// Supports package management on Arch Linux and derivative distributions.
/// Most operations require sudo privileges and will automatically
/// elevate permissions when needed.
#[derive(Debug)]
pub struct Pacman
{
    cli_path: String,
    repology: rspk_regs::RepologyClient,
}

impl Pacman
{
    /// Creates a new pacman manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `pacman` binary cannot be found on the system.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("pacman")
            .map_err(|_| Error::BinaryNotFound("pacman".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: rspk_regs::RepologyClient::new(),
        })
    }

    /// Parses the output of `pacman -Q` for installed packages.
    ///
    /// Format: `package_name version`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                {
                    Some(Package {
                        id:                parts[0].to_string(),
                        name:              None,
                        description:       None,
                        installed_version: parts[1].parse().ok(),
                        latest_version:    None,
                        manager_id:        "pacman".to_string(),
                        arch:              None,
                    })
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `pacman -Qu` for outdated packages.
    ///
    /// Format: `package_name old_version -> new_version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"^(\S+)\s+(\S+)\s+->\s+(\S+)$").unwrap();

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
                        manager_id: "pacman".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `pacman -Ss` for search results.
    ///
    /// Format:
    /// ```text
    /// repo/package_name version [installed]
    ///     description
    /// ```
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        let mut packages = Vec::new();
        let mut lines = output.lines().peekable();

        // Match lines like: "core/curl 7.81.0-1 [installed]"
        let re = regex::Regex::new(r"^\S+/(\S+)\s+(\S+)").unwrap();

        while let Some(line) = lines.next()
        {
            if let Some(caps) = re.captures(line)
            {
                let id = caps.get(1).unwrap().as_str().to_string();
                let version = caps.get(2).unwrap().as_str();

                // Next line should be the description
                let description =
                    lines.peek().map(|desc_line| desc_line.trim().to_string());

                if description.is_some()
                {
                    lines.next(); // consume the description line
                }

                packages.push(Package {
                    id,
                    name: None,
                    description,
                    installed_version: None,
                    latest_version: version.parse().ok(),
                    manager_id: "pacman".to_string(),
                    arch: None,
                });
            }
        }

        packages
    }
}

#[async_trait]
impl PackageManager for Pacman
{
    fn id(&self) -> &'static str
    {
        "pacman"
    }

    fn name(&self) -> &'static str
    {
        "pacman"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["pacman"]
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
            sync:            true,
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
            .map_err(|_| Error::ManagerUnavailable("pacman".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Q")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(Self::parse_installed_output(&output.stdout))
    }

    async fn outdated(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Qu")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(Self::parse_outdated_output(&output.stdout))
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
            .arg("-Ss")
            .arg(query)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(Self::parse_search_output(&output.stdout))
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        match self.repology.resolve_arch(abstract_name).await
        {
            Ok(Some(name)) => Ok(Some(name)),
            Ok(None) =>
            {
                // Fallback: try direct pacman search
                let output = CommandBuilder::new(&self.cli_path)
                    .arg("-Ss")
                    .arg(format!("^{abstract_name}$"))
                    .dry_run(ctx.dry_run)
                    .run()
                    .await;

                if let Ok(out) = output
                {
                    if out.stdout.contains(&format!("core/{abstract_name}"))
                        || out
                            .stdout
                            .contains(&format!("extra/{abstract_name}"))
                        || out
                            .stdout
                            .contains(&format!("community/{abstract_name}"))
                    {
                        return Ok(Some(abstract_name.to_string()));
                    }
                }
                Ok(None)
            },
            Err(_) => Ok(None),
        }
    }

    async fn install(
        &self,
        package_id: &str,
        _version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        // pacman doesn't support installing specific versions via CLI
        // without using downgrade tools
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-S")
            .arg("--noconfirm")
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
            .arg("-S")
            .arg("--noconfirm")
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            builder = builder.arg(id);
        }
        else
        {
            // -Syu syncs and upgrades all
            builder = CommandBuilder::new(&self.cli_path)
                .arg("-Syu")
                .arg("--noconfirm");
        }

        let output = builder.with_sudo().run().await?;
        Ok(output.stdout)
    }

    async fn remove(
        &self,
        package_id: &str,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-R")
            .arg("--noconfirm")
            .arg(package_id)
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("-Sy")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Clean package cache (keep last 3 versions)
        CommandBuilder::new(&self.cli_path)
            .arg("-Sc")
            .arg("--noconfirm")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Remove orphaned packages
        let result = CommandBuilder::new("pacman")
            .arg("-Qdtq")
            .dry_run(ctx.dry_run)
            .run()
            .await;

        if let Ok(output) = result
        {
            if !output.stdout.trim().is_empty()
            {
                // There are orphaned packages, remove them
                let orphans: Vec<&str> = output.stdout.lines().collect();
                if !orphans.is_empty()
                {
                    let mut builder = CommandBuilder::new(&self.cli_path)
                        .arg("-R")
                        .arg("--noconfirm")
                        .dry_run(ctx.dry_run);

                    for orphan in orphans
                    {
                        builder = builder.arg(orphan);
                    }

                    builder.with_sudo().run().await?;
                }
            }
        }

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
        let output = "curl 7.81.0-1\nwget 1.21.2-1\nfirefox 98.0-1\n";

        let packages = Pacman::parse_installed_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.81.0-1"
        );

        assert_eq!(packages[1].id, "wget");
        assert_eq!(packages[2].id, "firefox");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"curl 7.81.0-1 -> 7.82.0-1
wget 1.21.2-1 -> 1.21.3-1
"#;

        let packages = Pacman::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.81.0-1"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "7.82.0-1"
        );

        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = r#"core/curl 7.81.0-1 [installed]
    command line tool for transferring data with URL syntax
extra/wget 1.21.2-1
    A network utility to retrieve files from the Web
"#;

        let packages = Pacman::parse_search_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some(
                "command line tool for transferring data with URL syntax"
                    .to_string()
            )
        );

        assert_eq!(packages[1].id, "wget");
        assert_eq!(
            packages[1].description,
            Some(
                "A network utility to retrieve files from the Web".to_string()
            )
        );
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = Pacman::parse_installed_output("");
        assert_eq!(packages.len(), 0);

        let packages = Pacman::parse_outdated_output("");
        assert_eq!(packages.len(), 0);

        let packages = Pacman::parse_search_output("");
        assert_eq!(packages.len(), 0);
    }
}
