//! Termux `pkg` package manager implementation.
//!
//! Termux is a terminal emulator for Android that provides a Linux-like
//! environment. Its `pkg` command is a wrapper around `apt` customized
//! for Termux's package repositories. Note: this is NOT the same as
//! the FreeBSD `pkg` (pkgng) — see `freebsd_pkg.rs` for that one.
//!
//! Termux does NOT require root/sudo; it operates entirely within the
//! user's home directory under `$PREFIX` (typically
//! `/data/data/com.termux/files/usr`).

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The Termux `pkg` package manager (apt wrapper for Android).
///
/// Manages packages in the Termux Linux-like environment on Android.
/// Operates without root privileges.
#[derive(Debug)]
pub struct TermuxPkg
{
    cli_path: String,
    repology: RepologyClient,
}

impl TermuxPkg
{
    /// Creates a new Termux pkg manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if not running on Android or the `pkg`
    /// binary cannot be found.
    pub async fn new() -> Result<Self, Error>
    {
        if Platform::current()? != Platform::Android
        {
            return Err(Error::UnsupportedPlatform(
                "Termux pkg is only available on Android".to_string(),
            ));
        }

        let cli_path = which::which("pkg")
            .map_err(|_| Error::BinaryNotFound("pkg".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `pkg list-installed`.
    ///
    /// Format is apt-style: `package/stability version arch [installed]`.
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                {
                    // "package/stability" -> "package"
                    let id = parts[0].split('/').next()?.to_string();
                    let version = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: version.parse().ok(),
                        latest_version: None,
                        manager_id: "termux-pkg".to_string(),
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

    /// Parses the output of `pkg list-upgradable`.
    ///
    /// Format: `package/stability new-version arch [upgradable from:
    /// old-version]`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(
            r"^([^/]+)/\S+\s+(\S+)\s+\S+\s+\[upgradable from:\s*(\S+)\]",
        )
        .unwrap();

        output
            .lines()
            .filter_map(|line| {
                re.captures(line).map(|caps| {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let latest = caps.get(2).unwrap().as_str();
                    let installed = caps.get(3).unwrap().as_str();

                    Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed.parse().ok(),
                        latest_version: latest.parse().ok(),
                        manager_id: "termux-pkg".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    /// Parses the output of `pkg search`.
    ///
    /// Format: `package/stability version arch - description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                // Split on " - " to separate the package info from the
                // description
                let (info, description) = match line.split_once(" - ")
                {
                    Some((i, d)) => (i, Some(d.trim().to_string())),
                    None => (line, None),
                };

                let parts: Vec<&str> = info.split_whitespace().collect();
                if parts.len() >= 2
                {
                    let id = parts[0].split('/').next()?.to_string();
                    let version = parts[1];

                    Some(Package {
                        id,
                        name: None,
                        description,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "termux-pkg".to_string(),
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
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "termux")
            .await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg(abstract_name)
            .run()
            .await;

        if let Ok(output) = result
        {
            for line in output.stdout.lines()
            {
                if let Some((info, _)) = line.split_once(" - ")
                {
                    let id = info.split_whitespace().next().unwrap_or("");
                    let id = id.split('/').next().unwrap_or("");
                    if id == abstract_name
                    {
                        return Ok(Some(abstract_name.to_string()));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for TermuxPkg
{
    fn id(&self) -> &'static str
    {
        "termux-pkg"
    }
    fn name(&self) -> &'static str
    {
        "Termux pkg"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["pkg"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Android]
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
            .arg("list-installed")
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
            .arg("list-upgradable")
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

        // No sudo: Termux runs in a non-root environment.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("-y")
            .arg(&pkg_spec)
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
            .arg("uninstall")
            .arg("-y")
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("update")
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("autoremove")
            .arg("-y")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        CommandBuilder::new(&self.cli_path)
            .arg("clean")
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
        let output = "curl/stable 7.81.0 aarch64 [installed]\nwget/stable \
                      1.21.2 aarch64 [installed]\n";
        let packages = TermuxPkg::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.81.0"
        );
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl/stable 7.82.0 aarch64 [upgradable from: 7.81.0]\n";
        let packages = TermuxPkg::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.81.0"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "7.82.0"
        );
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "curl/stable 7.81.0 aarch64 - command line tool for \
                      transferring data\nwget/stable 1.21.2 aarch64 - \
                      retrieves files from the web\n";
        let packages = TermuxPkg::parse_search_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert!(packages[0].description.is_some());
    }
}
