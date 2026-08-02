//! Nix package manager implementation.
//!
//! Nix is a purely functional package manager that treats packages
//! as immutable values. It supports both the classic `nix-env` interface
//! and the newer `nix profile` commands. Nix is cross-platform and
//! does not require sudo.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The Nix package manager.
///
/// A purely functional package manager that provides reproducible
/// builds and atomic upgrades. Supports multiple profiles and does
/// not require root privileges.
#[derive(Debug)]
pub struct Nix
{
    cli_path: String,
    repology: RepologyClient,
}

impl Nix
{
    /// Creates a new Nix manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("nix")
            .map_err(|_| Error::BinaryNotFound("nix".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `nix-env -q --json`.
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        #[derive(serde::Deserialize)]
        struct NixPkg
        {
            #[serde(rename = "pname")]
            name:      Option<String>,
            #[serde(default)]
            name_full: String,
            version:   Option<String>,
        }

        let packages: Vec<NixPkg> = match serde_json::from_str(output)
        {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };

        packages
            .into_iter()
            .map(|p| {
                let id = p.name.unwrap_or_else(|| p.name_full.clone());
                let version = p.version.and_then(|v| v.parse().ok());

                Package {
                    id,
                    name: Some(p.name_full),
                    description: None,
                    installed_version: version,
                    latest_version: None,
                    manager_id: "nix".to_string(),
                    arch: None,
                }
            })
            .collect()
    }

    /// Parses the output of `nix-env -qc` (comparison).
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        // Format: `name old-version > new-version`
        let re = regex::Regex::new(r"^(\S+)\s+(\S+)\s+[<>]\s+(\S+)").unwrap();

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
                        manager_id: "nix".to_string(),
                        arch: None,
                    }
                })
            })
            .collect()
    }

    async fn resolve_internal(
        &self,
        abstract_name: &str,
    ) -> Result<Option<String>, Error>
    {
        if let Ok(Some(name)) =
            self.repology.resolve_for_repo(abstract_name, "nix").await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
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
impl PackageManager for Nix
{
    fn id(&self) -> &'static str
    {
        "nix"
    }
    fn name(&self) -> &'static str
    {
        "Nix"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["nix", "nix-env"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Linux, Platform::MacOS]
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
            .map_err(|_| Error::ManagerUnavailable("nix".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new("nix-env")
            .arg("-q")
            .arg("--json")
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
        // nix-env --compare-versions compares installed with available
        let output = CommandBuilder::new("nix-env")
            .arg("-qc")
            .arg("--prebuilt-only")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        // Filter only lines with '<' or '>' indicating differences
        let filtered: String = output
            .stdout
            .lines()
            .filter(|line| line.contains('<') || line.contains('>'))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(Self::parse_outdated_output(&filtered))
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
                "[dry-run] resolving '{abstract_name}' via Repology and nix \
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
            .arg("nixpkgs#")
            .arg(query)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        // Parse new-style nix search output
        Ok(output
            .stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2
                {
                    // Format: "legacyPackages.x86_64-linux.pkg version
                    // description..."
                    let full_id = parts[0];
                    let version = parts[1];
                    let description =
                        parts.get(2).map(|_| parts[2..].join(" "));

                    // Extract just the package name
                    let id = full_id.split('.').next_back()?.to_string();

                    Some(Package {
                        id,
                        name: Some(full_id.to_string()),
                        description,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "nix".to_string(),
                        arch: None,
                    })
                }
                else
                {
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    async fn install(
        &self,
        package_id: &str,
        _version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        // Nix does not require sudo
        let output = CommandBuilder::new("nix-env")
            .arg("-iA")
            .arg(format!("nixpkgs.{package_id}"))
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
            CommandBuilder::new("nix-env")
                .arg("-uA")
                .arg(format!("nixpkgs.{id}"))
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new("nix-env")
                .arg("-u")
                .arg("*")
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
        let output = CommandBuilder::new("nix-env")
            .arg("-e")
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        CommandBuilder::new("nix-channel")
            .arg("--update")
            .dry_run(ctx.dry_run)
            .run()
            .await?;
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Remove old generations
        CommandBuilder::new("nix-env")
            .arg("--delete-generations")
            .arg("old")
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        // Run garbage collector
        CommandBuilder::new("nix-collect-garbage")
            .arg("-d")
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
            r#"[{"pname":"curl","name":"curl-7.81.0","version":"7.81.0"}]"#;
        let packages = Nix::parse_installed_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl 7.81.0 < 7.82.0\n";
        let packages = Nix::parse_outdated_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "curl");
    }
}
