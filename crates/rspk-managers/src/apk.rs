//! Alpine Package Keeper (apk) implementation.
//!
//! apk is the package manager for Alpine Linux, a security-oriented,
//! lightweight Linux distribution. apk is known for its speed and
//! small footprint, making it ideal for containers and embedded
//! systems. It is also available on other distributions via
//! `apk-tools`, though this implementation targets the Alpine flavor.

use async_trait::async_trait;
use rspk_core::{
    Capabilities, Error, ExecutionContext, Package, PackageManager, Platform,
    Priority,
};
use rspk_executor::CommandBuilder;
use rspk_regs::RepologyClient;

/// The apk package manager for Alpine Linux.
///
/// Fast, lightweight package manager optimized for Alpine Linux.
/// Most modifying operations require root privileges via sudo.
#[derive(Debug)]
pub struct Apk
{
    cli_path: String,
    repology: RepologyClient,
}

impl Apk
{
    /// Creates a new apk manager instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the `apk` binary cannot be found or if
    /// the discovered `apk` is not the Alpine apk (e.g. Android's
    /// APK signing tools).
    pub async fn new() -> Result<Self, Error>
    {
        let cli_path = which::which("apk")
            .map_err(|_| Error::BinaryNotFound("apk".to_string()))?
            .to_string_lossy()
            .to_string();

        // Sanity check: Alpine's `apk --version` outputs something like
        // "apk-tools 2.14.0, compiled for x86_64".
        // Android's `apk` (signing tool) has completely different output.
        let probe = CommandBuilder::new(&cli_path)
            .arg("--version")
            .run()
            .await
            .map_err(|_| Error::ManagerUnavailable("apk".to_string()))?;

        let first_line = probe.stdout.lines().next().unwrap_or("");
        if !first_line.contains("apk-tools")
        {
            return Err(Error::ManagerUnavailable(format!(
                "apk at {cli_path} is not Alpine apk (got '{first_line}')"
            )));
        }

        Ok(Self {
            cli_path,
            repology: RepologyClient::new(),
        })
    }

    /// Parses the output of `apk list --installed`.
    ///
    /// Format: `name-version description {repo} (arch) [installed]`
    /// Example: `curl-8.4.0-r0 command line tool {main} (x86_64) [installed]`
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

                // Extract the first token: "name-version"
                let first_token = trimmed.split_whitespace().next()?;
                let (id, version) = split_apk_nv(first_token);
                let arch = extract_bracketed(trimmed, '(');

                Some(Package {
                    id,
                    name: None,
                    description: None,
                    installed_version: version.and_then(|v| v.parse().ok()),
                    latest_version: None,
                    manager_id: "apk".to_string(),
                    arch,
                })
            })
            .collect()
    }

    /// Parses the output of `apk version -v -l '<'`.
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

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 && parts[1] == "<"
                {
                    let (id, installed) = split_apk_nv(parts[0]);
                    let latest = parts[2];

                    Some(Package {
                        id,
                        name: None,
                        description: None,
                        installed_version: installed
                            .and_then(|v| v.parse().ok()),
                        latest_version: latest.parse().ok(),
                        manager_id: "apk".to_string(),
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

    /// Parses the output of `apk search -v`.
    ///
    /// Format: `name-version - description`
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

                // Split on first " - " to separate pkg info from description
                let (nv_part, description) = match trimmed.split_once(" - ")
                {
                    Some((a, b)) => (a, Some(b.trim().to_string())),
                    None => (trimmed, None),
                };

                let first_token = nv_part.split_whitespace().next()?;
                let (id, version) = split_apk_nv(first_token);

                Some(Package {
                    id,
                    name: None,
                    description,
                    installed_version: None,
                    latest_version: version.and_then(|v| v.parse().ok()),
                    manager_id: "apk".to_string(),
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
        if let Ok(Some(name)) = self
            .repology
            .resolve_for_repo(abstract_name, "alpine")
            .await
        {
            return Ok(Some(name));
        }

        // Try exact-match search.
        let result = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .arg("--exact")
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

/// Splits an apk `name-version` string at the first dash followed
/// by a digit (the version separator).
///
/// apk uses the convention that the version starts with a digit and
/// is separated from the package name by a dash. Version strings
/// themselves may contain dashes (e.g. `8.4.0-r0` where `-r0` is
/// the package revision), so we cannot simply split on the last dash.
///
/// # Examples
///
/// - `"curl-8.4.0-r0"` → `("curl", Some("8.4.0-r0"))`
/// - `"py3-numpy-1.24.0-r0"` → `("py3-numpy", Some("1.24.0-r0"))`
/// - `"bash"` → `("bash", None)`
/// - `"7zip-5.0-r1"` → `("7zip", Some("5.0-r1"))`
fn split_apk_nv(s: &str) -> (String, Option<&str>)
{
    let bytes = s.as_bytes();
    for i in 0..bytes.len()
    {
        if bytes[i] == b'-'
            && i + 1 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
        {
            return (s[..i].to_string(), Some(&s[i + 1..]));
        }
    }
    (s.to_string(), None)
}

/// Extracts the content between matching brackets, e.g. "(x86_64)".
#[allow(clippy::doc_markdown)]
fn extract_bracketed(s: &str, open: char) -> Option<String>
{
    let close = match open
    {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };

    let start = s.find(open)?;
    let end = s[start..].find(close)? + start;
    Some(s[start + 1..end].to_string())
}

#[async_trait]
impl PackageManager for Apk
{
    fn id(&self) -> &'static str
    {
        "apk"
    }
    fn name(&self) -> &'static str
    {
        "apk"
    }
    fn cli_names(&self) -> &'static [&'static str]
    {
        &["apk"]
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
            .arg("--installed")
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
        // `apk version -v -l '<'` lists packages where installed < available.
        let output = CommandBuilder::new(&self.cli_path)
            .arg("version")
            .arg("-v")
            .arg("-l")
            .arg("<")
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
                "[dry-run] resolving '{abstract_name}' via Repology and apk \
                 search --exact"
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
        let mut builder = CommandBuilder::new(&self.cli_path)
            .arg("search")
            .dry_run(ctx.dry_run);

        if extended
        {
            // -v shows versions and descriptions
            builder = builder.arg("-v");
        }
        if exact
        {
            builder = builder.arg("--exact");
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
        version: Option<&str>,
        ctx: &ExecutionContext,
    ) -> Result<String, Error>
    {
        // apk supports pinning a specific version via `name=version`.
        let pkg_spec = if let Some(v) = version
        {
            format!("{package_id}={v}")
        }
        else
        {
            package_id.to_string()
        };

        let output = CommandBuilder::new(&self.cli_path)
            .arg("add")
            .arg(&pkg_spec)
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
        // `apk upgrade` upgrades all packages.
        // `apk add --upgrade name` upgrades a specific one.
        let output = if let Some(id) = package_id
        {
            CommandBuilder::new(&self.cli_path)
                .arg("add")
                .arg("--upgrade")
                .arg(id)
                .with_sudo()
                .dry_run(ctx.dry_run)
                .run()
                .await?
        }
        else
        {
            CommandBuilder::new(&self.cli_path)
                .arg("upgrade")
                .with_sudo()
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
        let output = CommandBuilder::new(&self.cli_path)
            .arg("del")
            .arg(package_id)
            .with_sudo()
            .dry_run(ctx.dry_run)
            .run()
            .await?;

        Ok(output.stdout)
    }

    async fn sync(&self, ctx: &ExecutionContext) -> Result<(), Error>
    {
        // `apk update` refreshes the repository index.
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
        // `apk cache clean` removes downloaded package files.
        CommandBuilder::new(&self.cli_path)
            .arg("cache")
            .arg("clean")
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
    fn test_split_apk_nv()
    {
        assert_eq!(
            split_apk_nv("curl-8.4.0-r0"),
            ("curl".to_string(), Some("8.4.0-r0"))
        );
        assert_eq!(
            split_apk_nv("py3-numpy-1.24.0-r0"),
            ("py3-numpy".to_string(), Some("1.24.0-r0"))
        );
        assert_eq!(split_apk_nv("bash"), ("bash".to_string(), None));
    }

    #[test]
    fn test_extract_bracketed()
    {
        assert_eq!(
            extract_bracketed("curl-8.4.0 {main} (x86_64) [installed]", '('),
            Some("x86_64".to_string())
        );
        assert_eq!(extract_bracketed("no-brackets", '('), None);
    }

    #[test]
    fn test_parse_installed_output()
    {
        let output = "curl-8.4.0-r0 command line tool {main} (x86_64) \
                      [installed]\nwget-1.21.4-r0 network retriever {main} \
                      (x86_64) [installed]\n";
        let packages = Apk::parse_installed_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "8.4.0-r0"
        );
        assert_eq!(packages[0].arch, Some("x86_64".to_string()));
    }

    #[test]
    fn test_parse_outdated_output()
    {
        let output = "curl-8.4.0-r0    <    8.5.0-r0\nwget-1.21.4-r0    <    \
                      1.21.5-r0\n";
        let packages = Apk::parse_outdated_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].installed_version.as_ref().unwrap().to_string(),
            "8.4.0-r0"
        );
        assert_eq!(
            packages[0].latest_version.as_ref().unwrap().to_string(),
            "8.5.0-r0"
        );
    }

    #[test]
    fn test_parse_search_output()
    {
        let output = "curl-8.4.0-r0 - command line tool for transferring \
                      data\nwget-1.21.4-r0 - network retriever\n";
        let packages = Apk::parse_search_output(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].id, "curl");
        assert_eq!(
            packages[0].description,
            Some("command line tool for transferring data".to_string())
        );
    }

    #[test]
    fn test_parse_empty_output()
    {
        assert!(Apk::parse_installed_output("").is_empty());
        assert!(Apk::parse_outdated_output("").is_empty());
        assert!(Apk::parse_search_output("").is_empty());
    }
}
