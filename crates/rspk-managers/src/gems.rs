//! Ruby Gems package manager implementation for Ruby gems.
//!
//! This module provides support for managing Ruby gems installed
//! via the `gem` command. Ruby Gems is the official package manager
//! for the Ruby programming language.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RubyGemsClient;

/// The Ruby Gems package manager for Ruby gems.
///
/// Manages globally installed Ruby gems and their versions.
/// Gems can be installed system-wide (requiring sudo) or in user
/// directories (no sudo required).
#[derive(Debug)]
pub struct Gems
{
    cli_path: String,
    rubygems: RubyGemsClient,
}

impl Gems
{
    /// Creates a new Gems manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `gem` binary cannot be found.
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("gem")
            .map_err(|_| Error::BinaryNotFound("gem".to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            cli_path,
            rubygems: RubyGemsClient::new(),
        })
    }

    /// Parses the output of `gem list` for installed packages.
    ///
    /// Format: `gem_name (version1, version2, ...)`
    fn parse_installed_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"^(\S+)\s+\(([^)]+)\)").unwrap();
        let mut packages = Vec::new();

        for line in output.lines()
        {
            if let Some(caps) = re.captures(line)
            {
                let id = caps.get(1).unwrap().as_str().to_string();
                let versions_str = caps.get(2).unwrap().as_str();

                // Take the first version (usually the latest installed)
                let version = versions_str
                    .split(',')
                    .next()
                    .map(str::trim)
                    .and_then(|v| v.parse().ok());

                packages.push(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version,
                    latest_version: None,
                    manager_id: "gems".to_string(),
                    arch: None,
                });
            }
        }

        packages
    }

    /// Parses the output of `gem outdated` for outdated packages.
    ///
    /// Format: `gem_name (installed_version < latest_version)`
    fn parse_outdated_output(output: &str) -> Vec<Package>
    {
        let re = regex::Regex::new(r"^(\S+)\s+\((\S+)\s+<\s+(\S+)\)").unwrap();
        let mut packages = Vec::new();

        for line in output.lines()
        {
            if let Some(caps) = re.captures(line)
            {
                let id = caps.get(1).unwrap().as_str().to_string();
                let installed = caps.get(2).unwrap().as_str();
                let latest = caps.get(3).unwrap().as_str();

                packages.push(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: installed.parse().ok(),
                    latest_version: latest.parse().ok(),
                    manager_id: "gems".to_string(),
                    arch: None,
                });
            }
        }

        packages
    }

    /// Parses the output of `gem search` for search results.
    ///
    /// Format: `gem_name`
    ///
    /// Note: `gem search` only returns names by default. We use
    /// `gem search --details` to get descriptions when available.
    fn parse_search_output(output: &str, detailed: bool) -> Vec<Package>
    {
        let mut packages = Vec::new();

        if detailed
        {
            // Format: "gem_name (version)\n    description"
            let re = regex::Regex::new(r"^(\S+)\s+\(([^)]+)\)\s*$").unwrap();
            let mut lines = output.lines().peekable();

            while let Some(line) = lines.next()
            {
                if let Some(caps) = re.captures(line)
                {
                    let id = caps.get(1).unwrap().as_str().to_string();
                    let version = caps.get(2).unwrap().as_str();

                    // Next line might be description (indented)
                    let description = if let Some(next_line) = lines.peek()
                    {
                        if next_line.starts_with("    ")
                        {
                            let desc = next_line.trim().to_string();
                            lines.next();
                            Some(desc)
                        }
                        else
                        {
                            None
                        }
                    }
                    else
                    {
                        None
                    };

                    packages.push(Package {
                        id,
                        name: None,
                        description,
                        installed_version: None,
                        latest_version: version.parse().ok(),
                        manager_id: "gems".to_string(),
                        arch: None,
                    });
                }
            }
        }
        else
        {
            // Simple format: just gem names
            for line in output.lines()
            {
                let id = line.trim().to_string();
                if !id.is_empty()
                {
                    packages.push(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: None,
                        latest_version: None,
                        manager_id: "gems".to_string(),
                        arch: None,
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
        // Query RubyGems to see if this gem exists
        if self
            .rubygems
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
}

#[async_trait]
impl PackageManager for Gems
{
    fn id(&self) -> &'static str
    {
        "gems"
    }

    fn name(&self) -> &'static str
    {
        "RubyGems"
    }

    fn cli_names(&self) -> &'static [&'static str]
    {
        &["gem"]
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
            remove:          true,
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
            .map_err(|_| Error::ManagerUnavailable("gem".to_string()))
    }

    async fn installed(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let output = CommandBuilder::new(&self.cli_path)
            .arg("list")
            .arg("--local")
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
            .arg("outdated")
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
            eprintln!("[dry-run] resolving '{abstract_name}' via rubygems.org");
            return Ok(Some(abstract_name.to_string()));
        }

        self.resolve_internal(abstract_name).await
    }

    async fn search(
        &self,
        query: &str,
        extended: bool,
        _exact: bool,
        ctx: &ExecutionContext,
    ) -> Result<Vec<Package>, Error>
    {
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("--remote")
            .dry_run(ctx.dry_run);

        if extended
        {
            builder = builder.arg("--details");
        }

        builder = builder.arg(query);

        let output = builder.run().await?;

        if ctx.dry_run
        {
            return Ok(Vec::new());
        }

        Ok(Self::parse_search_output(&output.stdout, extended))
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
            .arg("--no-document")
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
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("update")
            .arg("--no-document")
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
            .arg("--all")
            .arg("--executables")
            .arg(package_id)
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, _ctx: &ExecutionContext) -> Result<(), Error>
    {
        // RubyGems doesn't require explicit sync
        Ok(())
    }

    async fn cleanup(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // Remove old versions of installed gems
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
    fn test_parse_installed_output()
    {
        let output = r#"*** LOCAL GEMS ***

rails (7.0.4, 6.1.7)
bundler (2.4.0)
rake (13.0.6)
"#;

        let packages = Gems::parse_installed_output(output);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "rails");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.0.4"
        );

        assert_eq!(packages[1].id, "bundler");
        assert_eq!(packages[2].id, "rake");
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = r#"rails (7.0.4 < 7.1.0)
bundler (2.4.0 < 2.5.0)
"#;

        let packages = Gems::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "rails");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "7.0.4"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "7.1.0"
        );

        assert_eq!(packages[1].id, "bundler");
    }

    #[test]
    fn test_parse_search_output_simple()
    {
        let output = "rails\nbundler\nrake\n";

        let packages = Gems::parse_search_output(output, false);
        assert_eq!(packages.len(), 3);

        assert_eq!(packages[0].id, "rails");
        assert_eq!(packages[1].id, "bundler");
        assert_eq!(packages[2].id, "rake");
    }

    #[test]
    fn test_parse_search_output_detailed()
    {
        let output = r#"rails (7.1.0)
    Full-stack web application framework.
bundler (2.5.0)
    The best way to manage your application's dependencies
"#;

        let packages = Gems::parse_search_output(output, true);
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].id, "rails");
        assert_eq!(
            packages[0].description,
            Some("Full-stack web application framework.".to_string())
        );

        assert_eq!(packages[1].id, "bundler");
        assert!(packages[1].description.is_some());
    }

    #[test]
    fn test_parse_empty_output()
    {
        let packages = Gems::parse_installed_output("");
        assert_eq!(packages.len(), 0);

        let packages = Gems::parse_outdated_output("");
        assert_eq!(packages.len(), 0);

        let packages = Gems::parse_search_output("", false);
        assert_eq!(packages.len(), 0);
    }
}
