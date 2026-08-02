//! YUM (Yellowdog Updater Modified) package manager implementation.
//!
//! YUM is the classic package manager for RHEL, Cent OS 7 and earlier.
//! It has been largely superseded by DNF, but is still in use on
//! legacy systems. The command interface is nearly identical to DNF.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The YUM package manager for legacy RHEL/CentOS systems.
#[derive(Debug)]
pub struct Yum
{
    cli_path: String,
    repology: RepologyClient,
}

impl Yum
{
    /// Creates a new YUM manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("yum")
            .map_err(|_| Error::BinaryNotFound("yum".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                {
                    let id = parts[0].split('.').next()?.to_string();
                    let version = parts[1];
                    let arch =
                        parts[0].split('.').nth(1).map(ToString::to_string);

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "yum".to_string(),
                        arch,
                    })
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                {
                    let id = parts[0].split('.').next()?.to_string();
                    let version = parts[1];
                    let arch =
                        parts[0].split('.').nth(1).map(ToString::to_string);

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "yum".to_string(),
                        arch,
                    })
                }
                else
                {
                    None
                }
            })
            .collect()
    }

    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, " : ");
                let id_part = parts.next()?.trim();
                let description = parts.next().map(|d| d.trim().to_string());

                let id = id_part.split('.').next()?.to_string();

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
                        manager_id: "yum".to_string(),
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
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "centos")
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
impl PackageManager for Yum
{
    fn id(&self) -> &'static str
    {
        "yum"
    }
    fn name(&self) -> &'static str
    {
        "YUM"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["yum"]
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
            .map_err(|_| Error::ManagerUnavailable("yum".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("installed")
            .arg("-q")
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
        let result = CommandBuilder::new(&self.cli_path)
            .arg("check-update")
            .arg("-q")
            .dry_run(ctx.dry_run)
            .run()
            .await;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        let output = match result
        {
            Ok(o) => o,
            Err(rspk_executor::ExecutionError::NonZeroExit {
                exit_code,
                stderr,
            }) =>
            {
                if exit_code == 100
                {
                    rspk_executor::CommandOutput {
                        stdout:    stderr,
                        stderr:    String::new(),
                        exit_code: 100,
                    }
                }
                else
                {
                    return Err(Error::ExecutionError(format!(
                        "Unexpected exit code: {exit_code}"
                    )));
                }
            },
            Err(e) => return Err(Error::ExecutionError(e.to_string())),
        };

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
                "[dry-run] resolving '{abstract_name}' via Repology and yum \
                 info"
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
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("update")
            .arg("-y")
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
            .arg("check-update")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await
            .ok();
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
            .arg("all")
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
        let output = "curl.x86_64   7.81.0-1.el7   @base\n";
        let packages = Yum::parse_installed_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }
}
