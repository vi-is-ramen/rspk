//! Core application state and dispatch logic.

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use rspk_api::resolver::Candidate;
use rspk_core::{ExecutionContext, Priority};
use rspk_managers::ManagerRegistry;
use rspk_needsfile::EvalContext;
use std::sync::Arc;

use crate::cli::Commands;
use crate::commands;
use crate::progress_ui::IndicatifReporter;
use crate::satisfy;

/// CLI application state.
///
/// Owns the discovered manager registry and the shared execution
/// context (dry-run / quiet flags).
#[derive(Debug)]
pub struct App
{
    /// Registry of all discovered package managers.
    pub registry: ManagerRegistry,
    /// Shared execution context.
    pub ctx: ExecutionContext,
    /// OpenTelemetry metrics handle.
    #[cfg(feature = "telemetry")]
    #[allow(unused)]
    pub metrics: Option<rspk_telemetry::Metrics>,
    /// Evaluation context for Needsfile conditions (OS, features,
    /// mode, available managers).
    pub eval_ctx: EvalContext,
}

impl App
{
    /// Creates a new application instance by discovering available
    /// package managers in parallel.
    pub async fn new(
        dry_run: bool,
        quiet: bool,
        mode: Option<String>,
        features: Vec<String>,
        #[cfg(feature = "telemetry")] metrics: Option<rspk_telemetry::Metrics>,
        rpc_mode: bool,
    ) -> Result<Self>
    {
        let registry = ManagerRegistry::discover().await?;

        #[cfg(feature = "telemetry")]
        #[allow(clippy::cast_possible_wrap)]
        if let Some(m) = &metrics
        {
            m.set_managers_available(registry.available().len() as i64);
        }

        // В RPC-режиме stdout зарезервирован под JSON — прогресс-бары
        // ломают протокол.
        let progress: Arc<dyn rspk_core::ProgressReporter> = if rpc_mode
        {
            Arc::new(rspk_core::NullProgressReporter)
        }
        else
        {
            Arc::new(IndicatifReporter::new())
        };

        let ctx = ExecutionContext {
            dry_run,
            quiet,
            progress,
        };

        // Build EvalContext from real platform + discovered managers.
        let mut eval_ctx = EvalContext::for_current_platform();
        for mgr in registry.available()
        {
            eval_ctx = eval_ctx.with_manager(mgr.id());
        }
        if let Some(m) = mode
        {
            eval_ctx = eval_ctx.with_mode(m);
        }
        for f in features
        {
            eval_ctx = eval_ctx.with_feature(f);
        }

        Ok(Self
        {
            registry,
            ctx,
            #[cfg(feature = "telemetry")]
            metrics,
            eval_ctx,
        })
    }

    /// Dispatches the parsed command to the appropriate handler.
    pub async fn run(&self, command: Commands) -> Result<()>
    {
        match command
        {
            Commands::Inventory => commands::inventory(self).await,
            Commands::Installed => commands::installed(self).await,
            Commands::Outdated => commands::outdated(self).await,
            Commands::Install {
                package,
                manager,
                no_resolve,
            } => commands::install(self, &package, manager.as_deref(), no_resolve).await,
            Commands::Upgrade {
                package,
                manager,
                no_resolve,
            } => {
                commands::upgrade(self, package.as_deref(), manager.as_deref(), no_resolve).await
            }
            Commands::Uninstall {
                package,
                manager,
                no_resolve,
            } => {
                commands::uninstall(self, &package, manager.as_deref(), no_resolve).await
            }
            Commands::Search {
                query,
                manager,
                extended,
                exact,
            } => commands::search(self, &query, manager.as_deref(), extended, exact).await,
            Commands::Sync { manager } => commands::sync(self, manager.as_deref()).await,
            Commands::Cleanup { manager } => commands::cleanup(self, manager.as_deref()).await,
            Commands::Resolve { name, manager } => {
                commands::resolve(self, &name, manager.as_deref()).await
            }
            Commands::Satisfy { path, no_resolve } => {
                satisfy::satisfy(self, &path, no_resolve).await
            }
            #[cfg(feature = "jsonrpc")]
            Commands::Rpc => crate::rpc::serve(self).await,
            Commands::Sbom {
                format,
                output,
                manager,
                component_name,
                component_version,
            } => {
                commands::sbom(
                    self,
                    &format,
                    output.as_deref(),
                    manager.as_deref(),
                    component_name.as_deref(),
                    component_version.as_deref(),
                )
                .await
            },
        }
    }

    /// Extracts the manager registry for external use (e.g. RPC server).
    ///
    /// After this call, `self` is effectively unusable — but that's
    /// fine because `rpc::serve` consumes the remainder of the CLI
    /// session.
    #[cfg(feature = "jsonrpc")]
    pub fn take_registry(&self) -> anyhow::Result<ManagerRegistry>
    {
        let _ = self;
        // Re-discover is cheap thanks to parallel discovery.
        Ok(tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(ManagerRegistry::discover())
        })?)
    }

    /// Selects the best candidate manager for a package, either
    /// automatically (quiet mode) or by prompting the user.
    pub fn select_candidate(&self, candidates: Vec<Candidate>) -> Result<Candidate>
    {
        if candidates.is_empty()
        {
            anyhow::bail!("No package manager has this package available");
        }

        if candidates.len() == 1 || self.ctx.quiet
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
                    Priority::System => "system".green(),
                    Priority::Universal => "universal".yellow(),
                    Priority::Language => "language".blue(),
                    Priority::Auxiliary => "auxiliary".dimmed(),
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
