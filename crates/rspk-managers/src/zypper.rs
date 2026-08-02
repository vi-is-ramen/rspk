//! Zypper package manager implementation for openSUSE.
//!
//! Zypper is the command-line package manager for openSUSE and SUSE
//! Linux Enterprise. It uses libzypp as its backend and supports
//! both RPM and pattern-based package management.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The Zypper package manager for openSUSE and SUSE Linux Enterprise.
#[derive(Debug)]
pub struct Zypper
{
    cli_path: String,
    repology: RepologyClient,
}

impl Zypper
{
    /// Creates a new Zypper manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("zypper")
            .map_err(|_| Error::BinaryNotFound("zypper".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `zypper search --installed-only --match-exact`.
    ///
    /// Format (XML): `<solvable name="pkg" edition="version" .../>`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r#"name="([^"]+)"[^>]*edition="([^"]+)""#)
            .unwrap();

        re.captures_iter(output)
            .map(|caps| {
                let id = caps.get(1).unwrap().as_str().to_string();
                let version = caps.get(2).unwrap().as_str();

                Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.parse().ok(),
                    latest_version: None,
                    manager_id: "zypper".to_string(),
                    arch: None,
                }
            })
            .collect()
    }

    /// Parses the output of `zypper list-updates`.
    ///
    /// Format: `S | Name | Type | Available Version | Arch | Repository`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                // Skip header rows
                if line.starts_with("S |")
                    || line.starts_with("---")
                    || line.trim().is_empty()
                {
                    return None;
                }

                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4
                {
                    let id = parts[1].trim().to_string();
                    let version = parts[3].trim();
                    let arch = parts.get(4).map(|a| a.trim().to_string());

                    if id.is_empty()
                    {
                        None
                    }
                    else
                    {
                        Some(Package {
                            id,
                            name: None,
                            description: None,
                            installed_version: None,
                            latest_version: version.parse().ok(),
                            manager_id: "zypper".to_string(),
                            arch,
                        })
                    }
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    /// Parses the output of `zypper search`.
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                if line.starts_with("S |")
                    || line.starts_with("---")
                    || line.trim().is_empty()
                {
                    return None;
                }

                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 5
                {
                    let id = parts[1].trim().to_string();
                    let description =
                        parts.get(4).map(|d| d.trim().to_string());

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
                            manager_id: "zypper".to_string(),
                            arch: None,
                        })
                    }
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
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "opensuse")
            .await
        {
            return Ok(Some(name));
        }

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
impl PackageManager for Zypper
{
    fn id(&self) -> &'static str
    {
        "zypper"
    }
    fn name(&self) -> &'static str
    {
        "Zypper"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["zypper"]
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
            .map_err(|_| Error::ManagerUnavailable("zypper".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("--installed-only")
            .arg("--type=package")
            .arg("--xmlout")
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
            .arg("list-updates")
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
                "[dry-run] resolving '{abstract_name}' via Repology and \
                 zypper info"
            );
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
            .arg("--non-interactive")
            .arg("install")
            .arg("--auto-agree-with-licenses")
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
        let output = if let Some(id) = package_id
        {
            CommandBuilder::new(&self.cli_path)
                .arg("--non-interactive")
                .arg("update")
                .arg(id)
                .with_sudo()
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new(&self.cli_path)
                .arg("--non-interactive")
                .arg("update")
                .with_sudo()
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
            .arg("--non-interactive")
            .arg("remove")
            .arg("--clean-deps")
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
            .arg("--non-interactive")
            .arg("refresh")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
            .arg("--all")
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
        let output =
            r#"<solvable name="curl" edition="7.81.0" arch="x86_64"/>"#;
        let packages = Zypper::parse_installed_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "S | Name | Type | Available Version | Arch | \
                      Repository\\
                      n--+------+-------+----------------+------+-----------\\
                      nv | curl | package | 7.82.0 | x86_64 | updates\n";
        let packages = Zypper::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }
}
