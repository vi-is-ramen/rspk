//! Flatpak package manager implementation.
//!
//! Flatpak is a universal package manager for Linux that provides
//! sandboxed applications. Unlike system managers, Flatpak packages
//! are isolated from the host system and include their own
//! dependencies.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The Flatpak package manager.
///
/// Manages sandboxed Linux applications from Flathub and other remotes.
/// Does not require sudo as it installs to user-writable locations
/// or system locations via polkit.
#[derive(Debug)]
pub struct Flatpak
{
    cli_path: String,
    repology: RepologyClient,
}

impl Flatpak
{
    /// Creates a new Flatpak manager instance.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("flatpak")
            .map_err(|_| Error::BinaryNotFound("flatpak".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `flatpak list --app --columns=application,version`.
    ///
    /// Format: `application_id    version`
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

                    if !id.is_empty() && !id.starts_with("Application")
                    {
                        Some(Package {
                            id,
                            name: None,
                            description: None,
                            installed_version: version.parse().ok(),
                            latest_version: None,
                            manager_id: "flatpak".to_string(),
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

    /// Parses the output of `flatpak search`.
    ///
    /// Format: `name    description    application_id    version    branch
    /// remotes`
    fn parse_search_output(output: &str) -> Vec<Package>
    {
        output
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 4
                {
                    let name = parts[0].trim().to_string();
                    let description =
                        parts.get(1).map(|d| d.trim().to_string());
                    let id = parts[2].trim().to_string();
                    let version = parts.get(3).map(|v| v.trim());

                    // Skip header
                    if name == "Name" || id.is_empty()
                    {
                        return None;
                    }

                    Some(Package {
                        id,
                        name: Some(name),
                        description,
                        installed_version: None,
                        latest_version: version.and_then(|v| v.parse().ok()),
                        manager_id: "flatpak".to_string(),
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
        // Flatpak uses reverse-DNS identifiers like org.mozilla.firefox.
        // First check Repology, then fall back to flatpak search.
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "flathub")
            .await
        {
            return Ok(Some(name));
        }

        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("--columns=application")
            .arg(abstract_name)
            .run()
            .await;

        if let Ok(output) = result
        {
            // Look for exact match in application IDs
            for line in output.stdout.lines()
            {
                let id = line.trim();
                if id == abstract_name
                    || id.ends_with(&format!(".{abstract_name}"))
                    || id.to_lowercase().contains(&abstract_name.to_lowercase())
                {
                    return Ok(Some(id.to_string()));
                }
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl PackageManager for Flatpak
{
    fn id(&self) -> &'static str
    {
        "flatpak"
    }
    fn name(&self) -> &'static str
    {
        "Flatpak"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["flatpak"]
    }
    fn platforms(&self) -> &'static [Platform]
    {
        &[Platform::Linux]
    }

    fn capabilities(&self) -> Capabilities
    {
        Capabilities {
            list_installed:  true,
            detect_outdated: false,
            search:          true,
            install:         true,
            upgrade_one:     false,
            upgrade_all:     true,
            remove:          true,
            sync:            false,
            cleanup:         true,
            resolve:         true,
        }
    }

    fn priority(&self) -> Priority
    {
        Priority::Universal
    }

    async fn is_available(&self) -> Result<bool, Error>
    {
        CommandBuilder::new(&self.cli_path)
            .arg("--version")
            .run()
            .await
            .map(|_| true)
            .map_err(|_| Error::ManagerUnavailable("flatpak".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--app")
            .arg("--columns=application,version")
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
        // Flatpak doesn't have a direct "list outdated" command.
        // We rely on `flatpak update` to handle everything.
        if ctx.dry_run
        {
            return Ok(Vec::new());
        }
        Ok(Vec::new())
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
                 flatpak search"
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
        // Flatpak does not support installing specific versions.
        // -y for non-interactive, --or-update to update if already installed.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("install")
            .arg("-y")
            .arg("flathub")
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
            .arg("update")
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

    async fn sync(&self, _ctx: &ExecutionContext) -> Result<(), Error>
    {
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Remove unused runtimes and extensions
        CommandBuilder::new(&self.cli_path)
            .arg("uninstall")
            .arg("--unused")
            .arg("-y")
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
        let output = "org.mozilla.firefox\t120.0\norg.gimp.GIMP\t2.10.36\n";
        let packages = Flatpak::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "org.mozilla.firefox");
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "Name\tDescription\tApplication \
                      ID\tVersion\tBranch\tRemotes\nFirefox\tMozilla \
                      Firefox\torg.mozilla.firefox\t120.0\tstable\tflathub\n";
        let packages = Flatpak::parse_search_output(output);
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].id, "org.mozilla.firefox");
    }
}
