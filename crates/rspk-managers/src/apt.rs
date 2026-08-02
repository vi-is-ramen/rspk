//! apt package manager implementation for Debian-based Linux distributions.
//!
//! This module provides support for managing packages via the Advanced
//! Package Tool (apt). Note that most operations require sudo privileges.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;

/// The apt package manager.
///
/// Supports package management on Debian-based Linux distributions.
/// Most operations require sudo privileges and will automatically
/// elevate permissions when needed.
#[derive(Debug)]
pub struct Apt
{
    cli_path:       String,
    cache_cli_path: String,
    dpkg_path:      String,
    repology:       rspk_regs::RepologyClient,
}

impl Apt
{
    /// Creates a new apt manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the required binaries (`apt`, `apt-cache`,
    /// `dpkg-query`) cannot be found on the system.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("apt")
            .map_err(|_| Error::BinaryNotFound("apt".to_string()))?
            .to_string_lossy()
            .to_string();

        let cache_cli_path = which::which("apt-cache")
            .map_err(|_| Error::BinaryNotFound("apt-cache".to_string()))?
            .to_string_lossy()
            .to_string();

        let dpkg_path = which::which("dpkg-query")
            .map_err(|_| Error::BinaryNotFound("dpkg-query".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            cache_cli_path,
            dpkg_path,
            repology: rspk_regs::RepologyClient::new(),
        })
    }

    /// Parses the output of `apt list --upgradable`.
    ///
    /// Format: `package/suite version arch [upgradable from: old_version]`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(
            r"^(\S+)/\S+\s+(\S+)\s+(\S+)\s+\[upgradable from:\s+(\S+)\]",
        )
        .unwrap();

        output
            .lines()
            .filter_map(|line| {
                re.captures(line).map(|caps| {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let latest = caps.get(2).unwrap().as_str();
                    let installed = caps.get(4).unwrap().as_str();
                    let arch = caps.get(3).unwrap().as_str().to_string();

                    Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "apt".to_string(),
                        arch: Some(arch),
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `dpkg-query` for installed packages.
    ///
    /// Format: `package\tversion\tstatus`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3
                {
                    let id = parts[0].to_string();
                    let version = parts[1];
                    let status = parts[2];

                    // Only include packages that are actually installed
                    if status.contains("installed")
                    {
                        Some(Package {
                            id,
                            name: None,
                            description: None,
                            installed_version: version.parse().ok(),
                            latest_version: None,
                            manager_id: "apt".to_string(),
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

    /// Parses the output of `apt-cache search`.
    ///
    /// Format: `package - description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                if let Some((name, description)) = line.split_once(" - ")
                {
                    Some(Package {
                        id:                name.trim().to_string(),
                        name:              None,
                        description:       Some(description.trim().to_string()),
                        installed_version: None,
                        latest_version:    None,
                        manager_id:        "apt".to_string(),
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
}

#[async_trait]
impl PackageManager for Apt
{
    fn id(&self) -> &'static str
    {
        "apt"
    }

    fn name(&self) -> &'static str
    {
        "APT"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["apt", "apt-cache", "dpkg-query"]
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
            .map_err(|_| Error::ManagerUnavailable("apt".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        // Use dpkg-query for reliable machine-readable output
        let output = CommandBuilder::new(&self.dpkg_path)
            .arg("-W")
            .arg("-f=${Package}\t${Version}\t${Status}\n")
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
            .arg("list")
            .arg("--upgradable")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(Self::parse_outdated_output(&output.stdout))
    }

    async fn search(
        &self,
        query: &str,
        extended: bool,
        exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let mut builder = CommandBuilder::new(&self.cache_cli_path)
            .arg("search")
            .dry_run(ctx.dry_run);

        if extended
        {
            builder = builder.arg("--full");
        }

        if exact
        {
            builder = builder.arg("--names-only");
        }

        builder = builder.arg(query);

        let output = builder.run().await?;
        Ok(Self::parse_search_output(&output.stdout))
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
            format!("{package_id}={v}")
        }
        else
        {
            package_id.to_string()
        };

        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("-y")
            .arg(&pkg_spec)
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
            .arg("-y")
            .dry_run(ctx.dry_run);

        if let Some(id) = package_id
        {
            // apt install upgrades a specific package
            builder = builder.arg("install").arg(id);
        }
        else
        {
            // apt upgrade updates all packages
            builder = builder.arg("upgrade");
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
            .arg("remove")
            .arg("-y")
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
            .arg("update")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Clean package cache
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Remove automatically installed packages no longer needed
        CommandBuilder::new(&self.cli_path)
            .arg("autoremove")
            .arg("-y")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(())
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        // Try to resolve via Repology for Debian/Ubuntu
        match self.repology.resolve_debian(abstract_name).await
        {
            Ok(Some(name)) => Ok(Some(name)),
            Ok(None) =>
            {
                // Fallback: try direct apt-cache search
                let output = CommandBuilder::new(&self.cache_cli_path)
                    .arg("search")
                    .arg("--names-only")
                    .arg(format!("^{abstract_name}$"))
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
            },
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"Listing... Done
curl/jammy-updates 7.81.0-1ubuntu1.4 amd64 [upgradable from: 7.81.0-1ubuntu1.3]
wget/jammy-updates 1.21.2-2ubuntu1 amd64 [upgradable from: 1.21.2-2ubuntu1]
"#;

        let packages = Apt::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(packages[0].arch, Some("amd64".to_string()));

        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_installed_output()
    {
        let output = "curl\t7.81.0-1ubuntu1.3\tinstall ok \
                      installed\nwget\t1.21.2-2ubuntu1\tinstall ok installed\n";

        let packages = Apt::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "curl - command line tool for transferring data with URL \
                      syntax\nwget - retrieves files from the web\n";

        let packages = Apt::parse_search_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some(
                "command line tool for transferring data with URL syntax"
                    .to_string()
            )
        );
    }
}
