//! FreeBSD `pkg` package manager implementation.
//!
//! This module implements support for the modern FreeBSD package
//! manager (pkgng), which is also used by Dragon Fly BSD as a fork.
//! Note: this is NOT the same as the `pkg` command used in Termux,
//! which is an apt wrapper and is handled separately in `termux_pkg.rs`.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The FreeBSD/Dragon Fly `pkg` package manager (pkgng).
///
/// Modern binary package manager for FreeBSD and Dragon Fly BSD.
/// Most operations require root privileges (via sudo).
#[derive(Debug)]
pub struct FreeBsdPkg
{
    cli_path:  String,
    repology:  RepologyClient,
    /// Whether this instance runs on Dragon Fly BSD instead of FreeBSD.
    dragonfly: bool,
}

impl FreeBsdPkg
{
    /// Creates a new FreeBSD/Dragon Fly pkg manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `pkg` binary cannot be found or if
    /// running on a non-BSD system (where `pkg` may refer to a
    /// different program, e.g. Termux's apt wrapper).
    pub async fn new() -> Result<Self, Error>
    {
        let platform = Platform::current()?;

        match platform
        {
            Platform::FreeBSD | Platform::DragonFly =>
            {},
            _ =>
            {
                return Err(Error::UnsupportedPlatform(format!(
                    "FreeBSD pkg is not available on {platform}"
                )));
            },
        }

        let cli_path = which::which("pkg")
            .map_err(|_| Error::BinaryNotFound("pkg".to_string()))?
            .to_string_lossy()
            .to_string();

        // Sanity check: FreeBSD/DragonFly pkg reports a version string
        // like "1.19.1" on `pkg -v`. Other `pkg` binaries (e.g. Termux)
        // behave differently.
        let probe = CommandBuilder::new(&cli_path)
            .arg("-v")
            .run()
            .await
            .map_err(|_| Error::ManagerUnavailable("pkg".to_string()))?;

        let version_line = probe.stdout.lines().next().unwrap_or("");
        // FreeBSD pkg version is a simple dotted numeric string.
        let looks_like_freebsd_pkg = version_line
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-');

        if !looks_like_freebsd_pkg
        {
            return Err(Error::ManagerUnavailable(format!(
                "pkg at {cli_path} does not look like FreeBSD pkg (got \
                 '{version_line}')"
            )));
        }

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
            dragonfly: matches!(platform, Platform::DragonFly),
        })
    }

    /// Parses the output of `pkg info -a -q`.
    ///
    /// Each line has the format `name-version` (the package name is
    /// separated from its version by the last dash).
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
                let (id, version) = split_pkg_nv(trimmed);
                Some(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.and_then(|v| v.parse().ok()),
                    latest_version: None,
                    manager_id: "freebsd-pkg".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    /// Parses the output of `pkg version -vRL=` (outdated packages).
    ///
    /// Format: `name-version    <    new-version`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                {
                    return None;
                }
                // Expected format: "name-version    <    remote-version"
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 && parts[1] == "<"
                {
                    let (id, installed) = split_pkg_nv(parts[0]);
                    let latest = parts[2];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed
                            .and_then(|v| v.parse().ok()),
                        latest_version: latest.parse().ok(),
                        manager_id: "freebsd-pkg".to_string(),
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

    /// Parses the output of `pkg search -q`.
    ///
    /// Format: `name-version` (one per line).
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty()
                {
                    return None;
                }
                let (id, version) = split_pkg_nv(trimmed);
                Some(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: None,
                    latest_version: version.and_then(|v| v.parse().ok()),
                    manager_id: "freebsd-pkg".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        let repo = if self.dragonfly
        {
            "dragonfly"
        }
        else
        {
            "freebsd"
        };
        if let Ok(Some(name)) =
            self.repology.resolve_for_repo(abstract_name, repo).await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("-q")
            .arg("-e")
            .arg(abstract_name)
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

/// Splits a FreeBSD-style `name-version` string into its components.
///
/// The version is separated from the name by the last dash. For
/// example: `curl-7.81.0` becomes `("curl", Some("7.81.0"))`.
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
impl PackageManager for FreeBsdPkg
{
    fn id(&self) -> &'static str
    {
        "freebsd-pkg"
    }
    fn name(&self) -> &'static str
    {
        if self.dragonfly
        {
            "DragonFly pkg"
        }
        else
        {
            "FreeBSD pkg"
        }
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["pkg"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::FreeBSD, Platform::DragonFly]
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
        Ok(true) // already verified in `new()`
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("info")
            .arg("-a")
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
        // `pkg version -vRL=` lists local packages older than remote.
        // Requires an up-to-date repository catalog.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("version")
            .arg("-v")
            .arg("-R")
            .arg("-L=")
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
                "[dry-run] resolving '{abstract_name}' via Repology and pkg \
                 search"
            );
            return Ok(Some(abstract_name.to_string()));
        }
        self.resolve_internal(abstract_name).await
    }

    async fn search(
        &self,
        query: &str,
        _extended: bool,
        exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("-q")
            .dry_run(ctx.dry_run);

        if exact
        {
            builder = builder.arg("-e");
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
        // FreeBSD pkg does not support installing specific versions
        // from the binary repository.
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
            .arg("upgrade")
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
            .arg("delete")
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
        // Remove orphaned dependencies.
        CommandBuilder::new(&self.cli_path)
            .arg("autoremove")
            .arg("-y")
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Clean cached package files.
        CommandBuilder::new(&self.cli_path)
            .arg("clean")
            .arg("-a")
            .arg("-y")
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
    fn test_split_pkg_nv()
    {
        assert_eq!(
            split_pkg_nv("curl-7.81.0"),
            ("curl".to_string(), Some("7.81.0"))
        );
        assert_eq!(
            split_pkg_nv("py39-numpy-1.24.0,1"),
            ("py39-numpy".to_string(), Some("1.24.0,1"))
        );
        assert_eq!(split_pkg_nv("bash"), ("bash".to_string(), None));
    }

    #[test]
    fn test_parse_installed_output()
    {
        let output = "curl-7.81.0\nwget-1.21.2\nbash-5.2.15\n";
        let packages = FreeBsdPkg::parse_installed_output(output);
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.81.0"
        );
        assert_eq!(packages[1].id, "wget");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl-7.81.0                     <   7.82.0\n\
                      wget-1.21.2                     <   1.21.3\n";
        let packages = FreeBsdPkg::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "curl-7.81.0\nwget-1.21.2\n";
        let packages = FreeBsdPkg::parse_search_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
    }
}
