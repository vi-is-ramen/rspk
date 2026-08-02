//! OpenBSD package tools implementation (`pkg_add` / `pkg_delete` /
//! `pkg_info`).
//!
//! OpenBSD uses a classic BSD-style package toolset consisting of three
//! separate binaries: `pkg_add` (install/upgrade), `pkg_delete`
//! (remove), and `pkg_info` (query). Unlike modern Linux managers,
//! the OpenBSD toolset does not have a unified CLI.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The OpenBSD package tools (`pkg_add` / `pkg_delete` / `pkg_info`).
///
/// Classic BSD-style package manager. Most operations require root
/// privileges via sudo.
#[derive(Debug)]
pub struct OpenBsdPkg
{
    pkg_add:    String,
    pkg_delete: String,
    pkg_info:   String,
    repology:   RepologyClient,
}

impl OpenBsdPkg
{
    /// Creates a new OpenBSD package tools instance.
    ///
    /// # Errors
    ///
    /// Returns an error if not running on OpenBSD or any of the
    /// required binaries (`pkg_add`, `pkg_delete`, `pkg_info`)
    /// cannot be found.
    pub async fn new() -> Result<Self, Error>
    {
        if Platform::current()? != Platform::OpenBSD
        {
            return Err(Error::UnsupportedPlatform(
                "OpenBSD pkg tools are only available on OpenBSD".to_string(),
            ));
        }

        let pkg_add = which::which("pkg_add")
            .map_err(|_| Error::BinaryNotFound("pkg_add".to_string()))?
            .to_string_lossy()
            .to_string();

        let pkg_delete = which::which("pkg_delete")
            .map_err(|_| Error::BinaryNotFound("pkg_delete".to_string()))?
            .to_string_lossy()
            .to_string();

        let pkg_info = which::which("pkg_info")
            .map_err(|_| Error::BinaryNotFound("pkg_info".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            pkg_add,
            pkg_delete,
            pkg_info,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `pkg_info -a`.
    ///
    /// Format: `name-version : description`
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

                let (nv, description) = match trimmed.split_once(" : ")
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
                    manager_id: "openbsd-pkg".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    /// Parses the output of `pkg_add -uvn` (update dry-run verbose).
    ///
    /// Lines containing `->` indicate an available upgrade:
    /// `name-version -> new-version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"(\S+)-(\S+)\s+->\s+(\S+)").unwrap();

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
                        manager_id: "openbsd-pkg".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `pkg_info -Q query`.
    ///
    /// Format: `name-version : description`
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
            .resolve_for_repo(abstract_name, "openbsd")
            .await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.pkg_info)
            .arg("-E")
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

/// Splits a BSD-style `name-version` string at the last dash.
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
impl PackageManager for OpenBsdPkg
{
    fn id(&self) -> &'static str
    {
        "openbsd-pkg"
    }
    fn name(&self) -> &'static str
    {
        "OpenBSD pkg"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["pkg_add", "pkg_delete", "pkg_info"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::OpenBSD]
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
        let output = CommandBuilder::new(&self.pkg_info)
            .arg("-a")
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
        // `pkg_add -u -v -n` does a verbose dry-run of upgrade-all,
        // listing which packages would be updated.
        let output = CommandBuilder::new(&self.pkg_add)
            .arg("-u")
            .arg("-v")
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
                "[dry-run] resolving '{abstract_name}' via Repology and \
                 pkg_info -E"
            );
            return Ok(Some(abstract_name.to_string()));
        }
        self.resolve_internal(abstract_name).await
    }

    async fn search(
        &self,
        query: &str,
        extended: bool,
        exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let mut builder =
            CommandBuilder::new(&self.pkg_info).dry_run(ctx.dry_run);

        if exact
        {
            builder = builder.arg("-E"); // exact name match
        }
        else if extended
        {
            builder = builder.arg("-Q"); // search descriptions
        }
        else
        {
            builder = builder.arg("-Q");
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
        // `pkg_add` installs and automatically fetches dependencies.
        let output = CommandBuilder::new(&self.pkg_add)
            .arg("-I") // non-interactive
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
        // `pkg_add -u` upgrades all installed packages to latest.
        // `pkg_add -u name` upgrades a specific one.
        let mut builder = CommandBuilder::new(&self.pkg_add)
            .arg("-u")
            .arg("-I")
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
        // `pkg_delete -a` removes unneeded dependencies too.
        let output = CommandBuilder::new(&self.pkg_delete)
            .arg("-a")
            .arg(package_id)
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, _ctx: &ExecutionContext) -> Result<(), Error>
    {
        // OpenBSD pkg tools use a static snapshot; no explicit sync
        // is required. `pkg_add -u` fetches the latest snapshot
        // automatically when updating.
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // `pkg_delete -a` (with no package arg) removes orphaned
        // dependencies.
        CommandBuilder::new(&self.pkg_delete)
            .arg("-a")
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
        let output = "curl-7.81.0 : command line tool for transferring \
                      data\nwget-1.21.2 : retrieve files from the web\n";
        let packages = OpenBsdPkg::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some("command line tool for transferring data".to_string())
        );
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl-7.81.0 -> curl-7.82.0\nwget-1.21.2 -> wget-1.21.3\n";
        let packages = OpenBsdPkg::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }
}
