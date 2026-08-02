//! CLI-specific Needsfile satisfaction with interactive prompts.
//!
//! Wraps [`rspk_api::satisfy`] with a [`ManagerSelector`]
//! implementation that prompts the user via `dialoguer` when
//! multiple managers can provide a package.

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use rspk_api::resolver::Candidate;
use rspk_api::satisfy::{self, ManagerSelector};

use crate::app::App;

/// Interactive manager selector for the CLI.
///
/// In quiet mode, delegates to [`PrioritySelector`]. Otherwise,
/// prompts the user to choose among candidates.
#[derive(Debug)]
struct CliSelector
{
    quiet: bool,
}

impl ManagerSelector for CliSelector
{
    fn select(&self, candidates: Vec<Candidate>) -> Result<Candidate>
    {
        if candidates.is_empty()
        {
            anyhow::bail!("No package manager has this package available");
        }

        if candidates.len() == 1 || self.quiet
        {
            let selected = candidates.into_iter().next().unwrap();
            println!(
                "→ Auto-selected {} (priority: {:?})",
                selected.manager.name().green(),
                selected.priority
            );
            return Ok(selected);
        }

        let items: Vec<String> = candidates
            .iter()
            .map(|c| {
                let priority_label = match c.priority
                {
                    rspk_core::Priority::System => "system".green(),
                    rspk_core::Priority::Universal => "universal".yellow(),
                    rspk_core::Priority::Language => "language".blue(),
                    rspk_core::Priority::Auxiliary => "auxiliary".dimmed(),
                };
                format!(
                    "{} - {} [{}]",
                    c.manager.name(),
                    c.resolved_id,
                    priority_label
                )
            })
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Package found in multiple managers. Select one:")
            .items(&items)
            .default(0)
            .interact()?;

        Ok(candidates.into_iter().nth(selection).unwrap())
    }
}

/// Satisfies package requirements from a Needsfile.
///
/// Delegates to [`rspk_api::satisfy`] with CLI-specific formatting
/// and interactive manager selection.
pub async fn satisfy(app: &App, path: &str, no_resolve: bool) -> Result<()>
{
    let selector = CliSelector {
        quiet: app.ctx.quiet,
    };

    let summary = satisfy::satisfy(
        &app.registry,
        &app.ctx,
        &app.eval_ctx,
        path,
        no_resolve,
        &selector,
    )
    .await?;

    if summary.total == 0
    {
        println!("Needsfile is empty, nothing to do.");
        return Ok(());
    }

    println!(
        "{}",
        format!("Satisfying {} package(s) from {}", summary.total, path).cyan()
    );
    println!();
    println!("{}", "Summary:".bold());
    println!("  {} installed", summary.installed.to_string().green());
    println!(
        "  {} already installed",
        summary.skipped.to_string().yellow()
    );

    if !summary.failed.is_empty()
    {
        println!("  {} failed", summary.failed.len().to_string().red());
        for (pkg, err) in &summary.failed
        {
            eprintln!("  ✗ {}: {}", pkg, err);
        }
        anyhow::bail!("{} package(s) failed to install", summary.failed.len());
    }

    Ok(())
}
