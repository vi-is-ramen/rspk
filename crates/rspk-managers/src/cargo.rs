//! Cargo package manager implementation for Rust crates.
//!
//! This module provides support for managing Rust crates installed
//! via `cargo install`. Cargo is the official package manager for
//! the Rust programming language.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::CratesIoClient;
use std::path::PathBuf;
use tokio::task::JoinSet;

/// The Cargo package manager for Rust crates.
///
/// Manages globally installed Rust binaries and their versions.
/// Unlike other managers, Cargo does not have an uninstall command,
/// so packages are removed by deleting binaries from `~/.cargo/bin`.
#[derive(Debug)]
pub struct Cargo
{
    cli_path:   String,
    #[allow(clippy::struct_field_names)]
    cargo_home: PathBuf,
    crates_io:  CratesIoClient,
}

impl Cargo
{
    /// Creates a new Cargo manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `cargo` binary cannot be found.
    #[allow(clippy::missing_panics_doc)] // Clippy false positive lol
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("cargo")
            .map_err(|_| Error::BinaryNotFound("cargo".to_string()))?
            .to_string_lossy()
            .to_string();
        let cargo_home = std::env::var("CARGO_HOME").map_or_else(
            |_| {
                dirs::home_dir()
                     // This doesn't panic on practice
                     .expect("Cannot determine home directory")
                     .join(".cargo")
            },
            PathBuf::from,
        );
        Ok(Self {
            cli_path,
            cargo_home,
            crates_io: CratesIoClient::new(),
        })
    }

    /// Parses the output of `cargo install --list`.
    ///
    /// Format:
    /// ```text
    /// package_name v1.2.3:
    ///     binary_name
    /// ```
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"^(\S+)\s+v(.+):$").unwrap();
        let mut packages = Vec::new();
        for line in output.lines()
        {
            if let Some(caps) = re.captures(line)
            {
                let id = caps.get(1).unwrap().as_str().to_string();
                let version = caps.get(2).unwrap().as_str();
                packages.push(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.parse().ok(),
                    latest_version: None,
                    manager_id: "cargo".to_string(),
                    arch: None,
                });
            }
        }
        packages
    }

    /// Parses the output of `cargo search`.
    ///
    /// Format: `"package_name" = "1.2.3"    # description`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r#"^"([^"]+)"\s*=\s*"([^"]+)"\s*#\s*(.*)$"#)
            .unwrap();
        let mut packages = Vec::new();
        for line in output.lines()
        {
            if let Some(caps) = re.captures(line)
            {
                packages.push(Package {
                    id:                caps
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_string(),
                    name:              None,
                    description:       Some(
                        caps.get(3).unwrap().as_str().trim().to_string(),
                    ),
                    installed_version: None,
                    latest_version:    caps
                        .get(2)
                        .unwrap()
                        .as_str()
                        .parse()
                        .ok(),
                    manager_id:        "cargo".to_string(),
                    arch:              None,
                });
            }
        }
        packages
    }
}

#[async_trait]
impl PackageManager for Cargo
{
    fn id(&self) -> &'static str
    {
        "cargo"
    }

    fn name(&self) -> &'static str
    {
        "Cargo"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["cargo"]
    }

    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Linux, Platform::MacOS, Platform::Windows]
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
            remove:          false,
            sync:            false,
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
            .map_err(|_| Error::ManagerUnavailable("cargo".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("--list")
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
        let installed = self.installed(ctx).await?;

        // Parallel crates.io queries via JoinSet (up to all at once;
        // reqwest connection pool handles throttling internally).
        let mut set: JoinSet<Option<Package>> = JoinSet::new();
        let crates_io = self.crates_io.clone();

        for pkg in installed
        {
            if let Some(current) = pkg.installed_version.clone()
            {
                let pkg_id = pkg.id.clone();
                let api = crates_io.clone();
                set.spawn(async move {
                    if let Ok(Some(latest_str)) =
                        api.latest_version(&pkg_id).await
                    {
                        if let Ok(latest) =
                            latest_str.parse::<semver::Version>()
                        {
                            if current < latest
                            {
                                return Some(Package {
                                    installed_version: Some(current),
                                    latest_version: Some(latest),
                                    ..pkg
                                });
                            }
                        }
                    }
                    None
                });
            }
        }

        let mut outdated = Vec::new();
        while let Some(res) = set.join_next().await
        {
            if let Ok(Some(pkg)) = res
            {
                outdated.push(pkg);
            }
        }
        Ok(outdated)
    }

    async fn resolve(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> Result<Option<String>, Error>
    {
        let _ = ctx;
        // Query crates.io to see if this crate exists
        if self
            .crates_io
            .exists(abstract_name)
            .await
            .is_ok_and(|exists| exists)
        {
            Ok(Some(abstract_name.to_string()))
        }
        else
        {
            Ok(None)
        }
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
        Ok(Self::parse_search_output(&output.stdout))
    }

    async fn install(
        &self,
        package_id: &str,
        version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .dry_run(ctx.dry_run);
        if let Some(v) = version
        {
            builder = builder.arg("--version").arg(v);
        }
        builder = builder.arg(package_id);
        let output = builder.run().await?;
        Ok(output.stdout)
    }

    async fn upgrade(
        &self,
        package_id: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        // cargo install --force reinstalls/updates the package
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("--force")
            .dry_run(ctx.dry_run);
        if let Some(id) = package_id
        {
            builder = builder.arg(id);
        }
        else
        {
            // Upgrade all installed packages
            let installed = self.installed(ctx).await?;
            for pkg in installed
            {
                builder = builder.arg(&pkg.id);
            }
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
        let _ = (package_id, ctx);
        Err(Error::UnsupportedCapability(
            "Cargo has not capability to remove binary crates.".to_string(),
        ))
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        let _ = ctx;
        // Cargo doesn't require explicit sync
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        let _ = ctx;
        // Clean registry cache
        let registry_dir = self.cargo_home.join("registry").join("cache");
        if registry_dir.exists()
        {
            tokio::fs::remove_dir_all(&registry_dir)
                .await
                .map_err(Error::Io)?;
        }
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
        let output = r#"ripgrep v13.0.0:
    rg
fd v8.5.3:
    fd
cargo-edit v0.11.7:
    cargo-add
    cargo-rm
"#;
        let packages = Cargo::parse_installed_output(output);
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].id, "ripgrep");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "13.0.0"
        );
        assert_eq!(packages[1].id, "fd");
        assert_eq!(packages[2].id, "cargo-edit");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = r#""ripgrep" = "13.0.0"    # A line-oriented search tool
"fd" = "8.5.3"    # A simple, fast and user-friendly alternative to 'find'
"cargo-edit" = "0.11.7"    # Adding, removing and upgrading Cargo dependencies
"#;
        let packages = Cargo::parse_search_output(output);
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].id, "ripgrep");
        assert_eq!(
            packages[0].description,
            Some("A line-oriented search tool".to_string())
        );
        assert_eq!(packages[1].id, "fd");
        assert_eq!(packages[2].id, "cargo-edit");
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = Cargo::parse_installed_output("");
        assert_eq!(packages.len(), 0);
        let packages = Cargo::parse_search_output("");
        assert_eq!(packages.len(), 0);
    }
}
