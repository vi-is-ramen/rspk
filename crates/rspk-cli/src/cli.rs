//! Command-line argument definitions using `clap`.
use clap::{Parser, Subcommand};

/// Command-line arguments parser.
#[derive(Parser)]
#[command(
    name = "pk",
    version,
    about = "Meta package manager for multiple package managers"
)]
pub struct Cli
{
    /// Enable dry-run mode (show commands without executing them).
    #[arg(long, short = 'n', global = true)]
    pub dry_run: bool,
    /// Quiet mode: auto-select the first available manager without
    /// prompting. When a package is available in multiple managers,
    /// the one with highest priority is chosen (system managers like
    /// apt/dnf are preferred over universal ones like flatpak/snap).
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,
    /// Active mode for Needsfile conditions (e.g. "dev", "prod").
    ///
    /// The value is available inside Needsfiles as `mode = "..."`.
    #[arg(long, global = true)]
    pub mode: Option<String>,
    /// Enable a named feature for Needsfile conditions.
    ///
    /// May be passed multiple times. Features are available inside
    /// Needsfiles as `feature "..."`.
    #[arg(long, global = true)]
    pub feature: Vec<String>,
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands.
#[derive(Subcommand)]
pub enum Commands
{
    /// List available package managers.
    Inventory,
    /// List installed packages.
    Installed,
    /// List outdated packages.
    Outdated,
    /// Install a package.
    Install
    {
        /// Package identifier (e.g., "lodash" or "lodash=4.17.21").
        package: String,
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
        /// Skip package name resolution (pass name as-is).
        #[arg(long)]
        no_resolve: bool,
    },
    /// Upgrade packages.
    Upgrade
    {
        /// Specific package to upgrade, or all if omitted.
        package: Option<String>,
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
        /// Skip package name resolution (pass name as-is).
        #[arg(long)]
        no_resolve: bool,
    },
    /// Uninstall a package.
    Uninstall
    {
        /// Package identifier.
        package: String,
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
        /// Skip package name resolution (pass name as-is).
        #[arg(long)]
        no_resolve: bool,
    },
    /// Search for packages.
    Search
    {
        /// Search query.
        query: String,
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
        /// Search in descriptions too.
        #[arg(long)]
        extended: bool,
        /// Match query exactly.
        #[arg(long)]
        exact: bool,
    },
    /// Synchronize package repositories.
    Sync
    {
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
    },
    /// Clean up caches and temporary files.
    Cleanup
    {
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
    },
    /// Resolve an abstract package name across managers.
    Resolve
    {
        /// The abstract package name to resolve.
        name: String,
        /// Force use of a specific package manager.
        #[arg(long)]
        manager: Option<String>,
    },
    /// Install packages from a Needsfile.
    Satisfy
    {
        /// Path to the Needsfile.
        path: String,
        /// Skip package name resolution (pass names as-is).
        #[arg(long)]
        no_resolve: bool,
    },
    
    /// Start the JSON-RPC 2.0 server on stdio.
    ///
    /// Reads newline-delimited JSON-RPC requests from stdin and
    /// writes responses to stdout. Use this mode to drive `pk`
    /// from scripts or GUI applications.
    #[cfg(feature = "jsonrpc")]
    Rpc,

    /// Generate a Software Bill of Materials (SBOM).
    ///
    /// Collects all installed packages across every discovered
    /// package manager and produces a machine-readable SBOM in
    /// CycloneDX or SPDX format.
    Sbom
    {
        /// Output format: cyclonedx (default) or spdx.
        #[arg(long, default_value = "cyclonedx")]
        format: String,

        /// Write output to a file instead of stdout.
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Restrict to a single package manager.
        #[arg(long)]
        manager: Option<String>,

        /// Name for the subject component in the SBOM.
        #[arg(long)]
        component_name: Option<String>,

        /// Version for the subject component.
        #[arg(long)]
        component_version: Option<String>,
    },
}

impl Commands
{
    /// Returns the canonical name of the command (for metrics/labels).
    #[allow(unused)]
    pub fn name(&self) -> &'static str
    {
        match self
        {
            Commands::Inventory => "inventory",
            Commands::Installed => "installed",
            Commands::Outdated => "outdated",
            Commands::Install { .. } => "install",
            Commands::Upgrade { .. } => "upgrade",
            Commands::Uninstall { .. } => "uninstall",
            Commands::Search { .. } => "search",
            Commands::Sync { .. } => "sync",
            Commands::Cleanup { .. } => "cleanup",
            Commands::Resolve { .. } => "resolve",
            Commands::Satisfy { .. } => "satisfy",
            #[cfg(feature = "jsonrpc")]
            Commands::Rpc => "rpc",
            Commands::Sbom { .. } => "sbom",
        }
    }
}
