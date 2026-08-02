//! XBPS package manager implementation for Void Linux.
//!
//! XBPS (X Binary Package System) is the native package manager for
//! Void Linux. It's known for being fast and lightweight, with a
//! focus on simplicity and reliability.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The XBPS package manager for Void Linux.
#[derive(Debug)]
pub struct Xbps
{
    cli_path:   String,
    query_path: String,
    repology:   RepologyClient,
}

impl Xbps
{
    /// Creates a new XBPS manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("xbps-install")
            .map_err(|_| Error::BinaryNotFound("xbps-install".to_string()))?
            .to_string_lossy()
            .to_string();

        let query_path = which::which("xbps-query")
            .map_err(|_| Error::BinaryNotFound("xbps-query".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            query_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `xbps-query -l`.
    ///
    /// Format: `ii package_name version description`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 3
                {
                    let id = parts[1].to_string();
                    let version = parts[2];
                    let description = parts.get(3).map(ToString::to_string);

                    Some(Package {
                        id,
                        name: None,
                        description,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "xbps".to_string(),
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

    /// Parses the output of `xbps-install -Sun`.
    ///
    /// Format: `package_name installed_version available_version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3
                {
                    let id = parts[0].to_string();
                    let installed = parts[1];
                    let latest = parts[2];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "xbps".to_string(),
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

    /// Parses the output of `xbps-query -Rs`.
    ///
    /// Format: `[-] package_name version description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 3
                {
                    // Skip the status marker like "[*]" or "[-]"
                    let id = parts[1].to_string();
                    let version = parts[2];
                    let description = parts.get(3).map(ToString::to_string);

                    Some(Package {
                        id,
                        name: None,
                        description,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "xbps".to_string(),
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
        if let Ok(Some(name)) =
            self.repology.resolve_for_repo(abstract_name, "void").await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.query_path)
            .arg("-Rs")
            .arg(format!("^{abstract_name}$"))
            .run()
            .await;

        if let Ok(output) = result
        {
            if !output.stdout.trim().is_empty()
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Xbps
{
    fn id(&self) -> &'static str
    {
        "xbps"
    }
    fn name(&self) -> &'static str
    {
        "XBPS"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["xbps-install", "xbps-query", "xbps-remove"]
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
            .map_err(|_| Error::ManagerUnavailable("xbps".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.query_path)
            .arg("-l")
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
            .arg("-Sun")
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
                 xbps-query"
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
        let output = CommandBuilder::new(&self.query_path)
            .arg("-Rs")
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
            .arg("-y")
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
                .arg("-y")
                .arg("-u")
                .arg(id)
                .with_sudo()
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new(&self.cli_path)
                .arg("-y")
                .arg("-Su")
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
        let output = CommandBuilder::new("xbps-remove")
            .arg("-y")
            .arg("-R")
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
            .arg("-S")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Remove orphaned packages
        CommandBuilder::new("xbps-remove")
            .arg("-y")
            .arg("-Oo")
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
            "ii curl 7.81.0 command line tool\nii wget 1.21.2 file retriever\n";
        let packages = Xbps::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl 7.81.0 7.82.0\n";
        let packages = Xbps::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "[*] curl 7.81.0 command line tool\n[-] wget 1.21.2 file \
                      retriever\n";
        let packages = Xbps::parse_search_output(output);
        assert_eq!(packages.len(), 2);
    }
}
