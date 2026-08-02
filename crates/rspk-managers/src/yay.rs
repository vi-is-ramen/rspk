//! yay package manager implementation for Arch Linux with AUR support.
//!
//! This module provides support for managing packages via yay, an
//! AUR helper that extends pacman with access to the Arch User
//! Repository (AUR). Yay can manage both official repository packages
//! and AUR packages through a unified interface.
//!
//! Note: Unlike pacman, yay does not require sudo for AUR operations
//! (which build packages as the current user), but still requires
//! sudo for official repository operations.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::AurClient;

/// The yay package manager.
///
/// An AUR helper that extends pacman with access to the Arch User
/// Repository. Supports managing both official repository packages
/// and community-contributed AUR packages.
#[derive(Debug)]
pub struct Yay
{
    cli_path: String,
    aur:      AurClient,
}

impl Yay
{
    /// Creates a new yay manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `yay` binary cannot be found on the system.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("yay")
            .map_err(|_| Error::BinaryNotFound("yay".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            aur: AurClient::new(),
        })
    }

    /// Strips ANSI color codes from a string.
    ///
    /// Yay produces colored output by default, which must be stripped
    /// for reliable parsing.
    fn strip_ansi(input: &str) -> String
    {
        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.replace_all(input, "").to_string()
    }

    /// Parses the output of `yay -Q` for installed packages.
    ///
    /// Format: `package_name version`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        let clean_output = Self::strip_ansi(output);

        clean_output
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
                        manager_id:        "yay".to_string(),
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

    /// Parses the output of `yay -Qu` for outdated packages.
    ///
    /// Format: `package_name old_version -> new_version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let clean_output = Self::strip_ansi(output);
        let re = regex::Regex::new(r"^(\S+)\s+(\S+)\s+->\s+(\S+)$").unwrap();

        clean_output
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
                        manager_id: "yay".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `yay -Ss` for search results.
    ///
    /// Format:
    /// ```text
    /// repo/package_name version [installed]
    ///     description
    /// aur/package_name version (+votes popularity)
    ///     description
    /// ```
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        let clean_output = Self::strip_ansi(output);
        let mut packages = Vec::new();
        let mut lines = clean_output.lines().peekable();

        // Match lines like:
        //   "core/curl 7.81.0-1 [installed]"
        //   "aur/curl-git 7.82.0.r123-1 (+42 0.00)"
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

                if let Some(desc) = description
                {
                    lines.next(); // consume the description line

                    packages.push(Package {
                        id,
                        name: None,
                        description: Some(desc),
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "yay".to_string(),
                        arch: None,
                    });
                }
            }
        }

        packages
    }
}

#[async_trait]
impl PackageManager for Yay
{
    fn id(&self) -> &'static str
    {
        "yay"
    }

    fn name(&self) -> &'static str
    {
        "yay"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["yay"]
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
            .map_err(|_| Error::ManagerUnavailable("yay".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Q")
            .arg("--color=never")
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
        // This returns both repository and AUR outdated packages
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Qu")
            .arg("--color=never")
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
        // Yay searches both official repos and AUR by default
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Ss")
            .arg("--color=never")
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
        // First check AUR directly
        if self
            .aur
            .exists(abstract_name)
            .await
            .is_ok_and(|exists| exists)
        {
            return Ok(Some(abstract_name.to_string()));
        }

        // Fallback: try yay search
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-Ss")
            .arg("--color=never")
            .arg(abstract_name)
            .dry_run(ctx.dry_run)
            .run()
            .await;

        if let Ok(out) = output
        {
            // Check if exact match exists in output
            let clean_output = Self::strip_ansi(&out.stdout);
            let re = regex::Regex::new(&format!(
                r"^\S+/{} ",
                regex::escape(abstract_name)
            ))
            .unwrap();

            for line in clean_output.lines()
            {
                if re.is_match(line)
                {
                    return Ok(Some(abstract_name.to_string()));
                }
            }
        }

        Ok(None)
    }

    async fn install(
        &self,
        package_id: &str,
        _version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        // Yay does not support installing specific versions via CLI.
        // It handles both repository and AUR packages transparently.
        //
        // Note: yay internally uses sudo for repo packages but builds
        // AUR packages as the current user. We don't pass with_sudo()
        // here because yay manages privilege elevation itself.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-S")
            .arg("--noconfirm")
            .arg("--color=never")
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
            // Upgrade a specific package
            CommandBuilder::new(&self.cli_path)
                .arg("-S")
                .arg("--noconfirm")
                .arg("--color=never")
                .arg(id)
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            // Upgrade all packages (both repo and AUR)
            CommandBuilder::new(&self.cli_path)
                .arg("-Syu")
                .arg("--noconfirm")
                .arg("--color=never")
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
            .arg("-R")
            .arg("--noconfirm")
            .arg("--color=never")
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("-Sy")
            .arg("--color=never")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Clean package cache
        CommandBuilder::new(&self.cli_path)
            .arg("-Sc")
            .arg("--noconfirm")
            .arg("--color=never")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Clean AUR build cache and orphaned packages
        // -Yc removes unneeded dependencies and AUR package cache
        CommandBuilder::new(&self.cli_path)
            .arg("-Yc")
            .arg("--noconfirm")
            .arg("--color=never")
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
    fn test_strip_ansi()
    {
        let colored = "\x1b[01;32mcore\x1b[0m/\x1b[01;32mcurl\x1b[0m 7.81.0-1";
        let stripped = Yay::strip_ansi(colored);
        assert_eq!(stripped, "core/curl 7.81.0-1");
    }

    #[test]
    fn test_parse_installed_output()
    {
        let output = "curl 7.81.0-1\nwget 1.21.2-1\nfirefox 98.0-1\n";

        let packages = Yay::parse_installed_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(packages[1].id, "wget");
        assert_eq!(packages[2].id, "firefox");
    }

    #[test]
    fn test_parse_installed_output_with_colors()
    {
        let output = "\x1b[01;32mcurl\x1b[0m 7.81.0-1\n\x1b[01;32mwget\x1b[0m \
                      1.21.2-1\n";

        let packages = Yay::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"curl 7.81.0-1 -> 7.82.0-1
wget 1.21.2-1 -> 1.21.3-1
"#;

        let packages = Yay::parse_outdated_output(output);
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
aur/curl-git 7.82.0.r123-1 (+42 0.00)
    Latest development version of curl
extra/wget 1.21.2-1
    A network utility to retrieve files from the Web
"#;

        let packages = Yay::parse_search_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some(
                "command line tool for transferring data with URL syntax"
                    .to_string()
            )
        );

        assert_eq!(packages[1].id, "curl-git");
        assert!(packages[1].description.is_some());

        assert_eq!(packages[2].id, "wget");
    }

    #[test]
    fn test_parse_search_output_with_colors()
    {
        let output = "\x1b[01;32mcore\x1b[0m/\x1b[01;32mcurl\x1b[0m \
                      7.81.0-1\n    test description\n";

        let packages = Yay::parse_search_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = Yay::parse_installed_output("");
        assert_eq!(packages.len(), 0);

        let packages = Yay::parse_outdated_output("");
        assert_eq!(packages.len(), 0);

        let packages = Yay::parse_search_output("");
        assert_eq!(packages.len(), 0);
    }
}
