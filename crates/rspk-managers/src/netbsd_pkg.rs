//! pkgin package manager implementation for NetBSD.
//!
//! pkgin is a high-level binary package manager for NetBSD's pkgsrc
//! framework. It provides apt-like commands on top of the lower-level
//! pkg_* tools (`pkg_add`, `pkg_delete`, `pkg_info`).

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The pkgin package manager for NetBSD (pkgsrc binary packages).
///
/// A higher-level interface over the NetBSD pkg_* tools with
/// automatic dependency resolution. Most operations require root.
#[derive(Debug)]
pub struct Pkgin
{
    cli_path: String,
    repology: RepologyClient,
}

impl Pkgin
{
    /// Creates a new pkgin manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if not running on NetBSD or the `pkgin`
    /// binary cannot be found.
    pub async fn new() -> Result<Self, Error>
    {
        if Platform::current()? != Platform::NetBSD
        {
            return Err(Error::UnsupportedPlatform(
                "pkgin is only available on NetBSD".to_string(),
            ));
        }

        let cli_path = which::which("pkgin")
            .map_err(|_| Error::BinaryNotFound("pkgin".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `pkgin list`.
    ///
    /// Format: `name-version = description`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                {
                    return None;
                }

                let (nv, description) = match trimmed.split_once(" = ")
                {
                    Some((a, b)) => (a, Some(b.trim().to_string())),
                    None => (trimmed, None),
                };

                let (id, version) = split_pkg_nv(nv);
                Some(Package {
                    id,
                    name: None,
                    description,
                    installed_version: version.and_then(|v| v.parse().ok()),
                    latest_version: None,
                    manager_id: "pkgin".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    /// Parses the output of `pkgin upgrade -n` (dry-run).
    ///
    /// pkgin prints planned actions such as:
    ///   `upgrade: name-oldversion to name-newversion`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re =
            regex::Regex::new(r"upgrade:\s+(\S+)-(\S+)\s+to\s+(\S+)-(\S+)")
                .unwrap();

        output
            .lines()
            .filter_map(|line| {
                re.captures(line).map(|caps| {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let installed = caps.get(2).unwrap().as_str();
                    let latest = caps.get(4).unwrap().as_str();

                    Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "pkgin".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `pkgin search`.
    ///
    /// Format: `name-version = description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        Self::parse_installed_output(output)
    }

    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "netbsd")
            .await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("show")
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

/// Splits a NetBSD-style `name-version` string at the last dash.
fn split_pkg_nv(s: &str) -> (String, Option<&str>)
{
    if let Some(pos) = s.rfind('-')
    {
        (s[..pos].to_string(), Some(&s[pos + 1..]))
    }
    else
    {
        (s.to_string(), None)
    }
}

#[async_trait]
impl PackageManager for Pkgin
{
    fn id(&self) -> &'static str
    {
        "pkgin"
    }
    fn name(&self) -> &'static str
    {
        "pkgin"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["pkgin"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::NetBSD]
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
        Priority::System
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        Ok(true)
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
        // Dry-run upgrade shows what would be upgraded.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("upgrade")
            .arg("-n")
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
                "[dry-run] resolving '{abstract_name}' via Repology and pkgin \
                 show"
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
        // `-y` for non-interactive confirmation.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-y")
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
            .arg("-y")
            .arg("upgrade")
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
        // `-R` also removes dependencies no longer needed.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("-y")
            .arg("-R")
            .arg("remove")
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
            .arg("-y")
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
            .arg("-y")
            .arg("autoremove")
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
        let output = "curl-7.81.0 = command line tool for transferring \
                      data\nwget-1.21.2 = retrieve files from the web\n";
        let packages = Pkgin::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "upgrade: curl-7.81.0 to curl-7.82.0\n";
        let packages = Pkgin::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }
}
