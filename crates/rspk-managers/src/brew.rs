//! Homebrew package manager implementation.
//!
//! This module provides support for managing packages via Homebrew,
//! the "missing package manager for macOS" (also available on Linux
//! as Linuxbrew). Homebrew supports both formulae (CLI tools) and
//! casks (GUI applications).

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;
use serde::Deserialize;

/// The Homebrew package manager.
///
/// Manages both formulae (command-line tools) and casks (GUI applications).
/// Unlike many other managers, Homebrew does not require sudo and is
/// designed to install packages in user-writable locations.
#[derive(Debug)]
pub struct Brew
{
    cli_path: String,
    repology: RepologyClient,
}

impl Brew
{
    /// Creates a new Homebrew manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `brew` binary cannot be found on the system.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("brew")
            .map_err(|_| Error::BinaryNotFound("brew".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `brew list --formula --versions`.
    ///
    /// Format: `formula_name version1 version2 ...`
    fn parse_formulae_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let id = parts.next()?.to_string();
                let version = parts.next()?;

                Some(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.parse().ok(),
                    latest_version: None,
                    manager_id: "brew".to_string(),
                    arch: None,
                })
            })
            .collect()
    }

    /// Parses the output of `brew list --cask --versions`.
    ///
    /// Format: `cask_name version`
    fn parse_cask_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let id = parts.next()?.to_string();
                let version = parts.next()?;

                Some(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.parse().ok(),
                    latest_version: None,
                    manager_id: "brew".to_string(),
                    arch: Some("cask".to_string()),
                })
            })
            .collect()
    }

    /// Parses the output of `brew outdated --json=v2`.
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        #[derive(Deserialize)]
        struct OutdatedOutput
        {
            #[serde(default)]
            formulae: Vec<OutdatedFormula>,
            #[serde(default)]
            casks:    Vec<OutdatedCask>,
        }

        #[derive(Deserialize)]
        struct OutdatedFormula
        {
            name:               String,
            #[serde(default)]
            installed_versions: Vec<String>,
            current_version:    String,
        }

        #[derive(Deserialize)]
        struct OutdatedCask
        {
            name:               String,
            #[serde(default)]
            installed_versions: Vec<String>,
            current_version:    String,
        }

        let data: OutdatedOutput = match serde_json::from_str(output)
        {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let mut packages = Vec::new();

        for f in data.formulae
        {
            let installed =
                f.installed_versions.first().and_then(|v| v.parse().ok());
            packages.push(Package {
                id:                f.name,
                name:              None,
                description:       None,
                installed_version: installed,
                latest_version:    f.current_version.parse().ok(),
                manager_id:        "brew".to_string(),
                arch:              None,
            });
        }

        for c in data.casks
        {
            let installed =
                c.installed_versions.first().and_then(|v| v.parse().ok());
            packages.push(Package {
                id:                c.name,
                name:              None,
                description:       None,
                installed_version: installed,
                latest_version:    c.current_version.parse().ok(),
                manager_id:        "brew".to_string(),
                arch:              Some("cask".to_string()),
            });
        }

        packages
    }

    /// Parses the output of `brew search`.
    ///
    /// Format:
    /// ```text
    /// ==> Formulae
    /// name1    name2    name3
    ///
    /// ==> Casks
    /// cask1    cask2
    /// ```
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        let mut packages = Vec::new();
        let mut in_formulae = false;
        let mut in_casks = false;

        for line in output.lines()
        {
            let trimmed = line.trim();

            if trimmed == "==> Formulae"
            {
                in_formulae = true;
                in_casks = false;
                continue;
            }
            else if trimmed == "==> Casks"
            {
                in_casks = true;
                in_formulae = false;
                continue;
            }
            else if trimmed.starts_with("==>")
            {
                in_formulae = false;
                in_casks = false;
                continue;
            }

            if (in_formulae || in_casks) && !trimmed.is_empty()
            {
                for name in trimmed.split_whitespace()
                {
                    // Skip names with special markers
                    let clean_name = name.trim_end_matches(" (installed)");
                    if clean_name.is_empty()
                    {
                        continue;
                    }

                    packages.push(Package {
                        id:                clean_name.to_string(),
                        name:              None,
                        description:       None,
                        installed_version: None,
                        latest_version:    None,
                        manager_id:        "brew".to_string(),
                        arch:              if in_casks
                        {
                            Some("cask".to_string())
                        }
                        else
                        {
                            None
                        },
                    });
                }
            }
        }

        packages
    }

    /// Internal resolution logic without dry-run handling.
    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        // Try Repology first (maps "firefox" -> "firefox" for brew)
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "homebrew")
            .await
        {
            return Ok(Some(name));
        }

        // Fallback: try brew info directly
        let result = CommandBuilder::new(&self.cli_path)
            .arg("info")
            .arg("--json=v2")
            .arg(abstract_name)
            .run()
            .await;

        if let Ok(output) = result
        {
            if output.exit_code == 0 && !output.stdout.trim().is_empty()
            {
                return Ok(Some(abstract_name.to_string()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Brew
{
    fn id(&self) -> &'static str
    {
        "brew"
    }

    fn name(&self) -> &'static str
    {
        "Homebrew"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["brew"]
    }

    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::MacOS, Platform::Linux]
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
            .map_err(|_| Error::ManagerUnavailable("brew".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        // Formulae
        let formulae_output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--formula")
            .arg("--versions")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Casks
        let casks_output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--cask")
            .arg("--versions")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        let mut packages = Self::parse_formulae_output(&formulae_output.stdout);
        packages.extend(Self::parse_cask_output(&casks_output.stdout));

        Ok(packages)
    }

    async fn outdated(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("outdated")
            .arg("--json=v2")
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
                "[dry-run] resolving '{abstract_name}' via Repology and brew \
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
        // Homebrew doesn't support installing specific versions easily.
        // Note: Homebrew does NOT require sudo.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg(package_id)
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
        // Remove old versions and cached downloads
        CommandBuilder::new(&self.cli_path)
            .arg("cleanup")
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
    fn test_parse_formulae_output()
    {
        let output = r#"curl 8.4.0
wget 1.21.4
ripgrep 14.1.0
"#;

        let packages = Brew::parse_formulae_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "8.4.0"
        );

        assert_eq!(packages[1].id, "wget");
        assert_eq!(packages[2].id, "ripgrep");
    }

    #[test]
    fn test_parse_cask_output()
    {
        let output = r#"firefox 120.0
visual-studio-code 1.85.1
"#;

        let packages = Brew::parse_cask_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "firefox");
        assert_eq!(packages[0].arch, Some("cask".to_string()));
        assert_eq!(packages[1].id, "visual-studio-code");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"{
            "formulae": [
                {
                    "name": "wget",
                    "installed_versions": ["1.21.3"],
                    "current_version": "1.21.4"
                }
            ],
            "casks": [
                {
                    "name": "firefox",
                    "installed_versions": ["119.0"],
                    "current_version": "120.0"
                }
            ]
        }"#;

        let packages = Brew::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "wget");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "1.21.3"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "1.21.4"
        );

        assert_eq!(packages[1].id, "firefox");
        assert_eq!(packages[1].arch, Some("cask".to_string()));
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = r#"==> Formulae
curl    wget    ripgrep

==> Casks
firefox    google-chrome
"#;

        let packages = Brew::parse_search_output(output);
        assert_eq!(packages.len(), 5);

        assert_eq!(packages[0].id, "curl");
        assert!(packages[0].arch.is_none());

        assert_eq!(packages[3].id, "firefox");
        assert_eq!(packages[3].arch, Some("cask".to_string()));
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = Brew::parse_formulae_output("");
        assert_eq!(packages.len(), 0);

        let packages = Brew::parse_outdated_output("");
        assert_eq!(packages.len(), 0);
    }
}
