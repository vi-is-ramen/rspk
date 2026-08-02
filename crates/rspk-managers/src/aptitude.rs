//! Aptitude package manager implementation for Debian-based systems.
//!
//! Aptitude is a high-level interface to the APT package system,
//! providing both command-line and text-based interactive interfaces.
//! Unlike the basic `apt` tool, aptitude offers advanced dependency
//! resolution and conflict management.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The aptitude package manager.
///
/// A high-level interface to APT that provides advanced dependency
/// resolution. Most operations require sudo privileges.
#[derive(Debug)]
pub struct Aptitude
{
    cli_path: String,
    repology: RepologyClient,
}

impl Aptitude
{
    /// Creates a new aptitude manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("aptitude")
            .map_err(|_| Error::BinaryNotFound("aptitude".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `aptitude search -F "%p\t%v\t%s" "~i"`.
    ///
    /// Format: `package\tversion\tstatus`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2
                {
                    let id = parts[0].trim().to_string();
                    let version = parts[1].trim();

                    if !id.is_empty() && !version.is_empty()
                    {
                        Some(Package {
                            id,
                            name: None,
                            description: None,
                            installed_version: version.parse().ok(),
                            latest_version: None,
                            manager_id: "aptitude".to_string(),
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

    /// Parses the output of `aptitude search "~U" -F "%p\t%v\t%V"`.
    ///
    /// Format: `package\tinstalled_version\tavailable_version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 3
                {
                    let id = parts[0].trim().to_string();
                    let installed = parts[1].trim();
                    let latest = parts[2].trim();

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
                            installed_version: installed.parse().ok(),
                            latest_version: latest.parse().ok(),
                            manager_id: "aptitude".to_string(),
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

    /// Parses the output of `aptitude search`.
    ///
    /// Format: `i   package_name    - description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                // Skip state prefixes: i (installed), p (purged), etc.
                let trimmed = line.trim_start();
                if trimmed.len() < 4
                {
                    return None;
                }

                // Skip the state marker (typically 3 chars + space)
                let rest = &trimmed[4..];

                // Split into name and description
                let mut parts = rest.splitn(2, " - ");
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
                        manager_id: "aptitude".to_string(),
                        arch: None,
                    })
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
            self.repology.resolve_debian(abstract_name).await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg(format!("^{abstract_name}$"))
            .arg("-F")
            .arg("%p")
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
impl PackageManager for Aptitude
{
    fn id(&self) -> &'static str
    {
        "aptitude"
    }
    fn name(&self) -> &'static str
    {
        "Aptitude"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["aptitude"]
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
            .map_err(|_| Error::ManagerUnavailable("aptitude".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("~i")
            .arg("-F")
            .arg("%p\t%v")
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
            .arg("search")
            .arg("~U")
            .arg("-F")
            .arg("%p\t%v\t%V")
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
                 aptitude"
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
            .arg("install")
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
                .arg("install")
                .arg("-y")
                .arg(id)
                .with_sudo()
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new(&self.cli_path)
                .arg("safe-upgrade")
                .arg("-y")
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
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
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
        let output = "curl\t7.81.0\nwget\t1.21.2\n";
        let packages = Aptitude::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl\t7.81.0\t7.82.0\n";
        let packages = Aptitude::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "i   curl            - command line tool for \
                      transferring data\np   wget            - retrieves \
                      files from the web\n";
        let packages = Aptitude::parse_search_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert!(packages[0].description.is_some());
    }
}
