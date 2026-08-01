//! Pk CLI interface implementation.

#![deny(missing_docs, missing_debug_implementations)]

use anyhow::{Context, Result};
use clap::Parser;
use rspk_core::{
    Mgr, get_install_command, resolve_package_name, run_command, select_manager,
};
use std::fs;

#[derive(Parser)]
#[command(name = "pk", about = "Universal package installer", version = rspk_core::version::STRING)]
struct Cli
{
    /// Package names or @file (e.g., libllvm or @deps.txt)
    #[arg(required = true)]
    packages: Vec<String>,

    /// Explicitly choose package manager (e.g., apt, pacman, dnf)
    #[arg(short, long)]
    manager: Option<Mgr>,
}

fn main() -> Result<()>
{
    let cli = Cli::parse();

    let mgr =
        select_manager(cli.manager).map_err(|e| anyhow::anyhow!("{:#}", e))?;

    let mut abstract_packages = Vec::new();
    for arg in &cli.packages
    {
        if let Some(filename) = arg.strip_prefix('@')
        {
            let content = fs::read_to_string(filename)
                .with_context(|| format!("Failed to read {}", filename))?;
            let lines = content
                .lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(String::from)
                .collect::<Vec<_>>();
            abstract_packages.extend(lines);
        }
        else
        {
            abstract_packages.push(arg.clone());
        }
    }

    if abstract_packages.is_empty()
    {
        anyhow::bail!("No packages to install");
    }

    let real_packages: Vec<String> = abstract_packages
        .iter()
        .map(|pkg| resolve_package_name(pkg, mgr))
        .collect();

    let install_cmd =
        get_install_command(mgr).map_err(|e| anyhow::anyhow!("{:#}", e))?;

    let status = run_command(install_cmd, real_packages, mgr)
        .map_err(|e| anyhow::anyhow!("{:#}", e))?;

    if !status.success()
    {
        anyhow::bail!("Installation failed");
    }

    Ok(())
}
