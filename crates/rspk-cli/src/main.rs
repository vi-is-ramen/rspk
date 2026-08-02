//! CLI entry point for the `pk` meta package manager.
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

mod app;
mod cli;
mod commands;
mod progress_ui;
mod satisfy;

#[cfg(feature = "jsonrpc")]
mod rpc;

use clap::Parser;
use colored::Colorize;
use std::process::ExitCode;
use tracing::error;

use crate::app::App;
use crate::cli::Cli;
#[cfg(feature = "jsonrpc")]
use crate::cli::Commands;

#[tokio::main]
async fn main() -> ExitCode
{
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if cli.dry_run
    {
        eprintln!("{}", "→ dry-run mode enabled".dimmed());
    }
    if cli.quiet
    {
        eprintln!("{}", "→ quiet mode enabled (auto-select manager)".dimmed());
    }

    #[cfg(feature = "jsonrpc")]
    let rpc_mode = matches!(cli.command, Commands::Rpc);
    #[cfg(not(feature = "jsonrpc"))]
    let rpc_mode = false;

    #[cfg(feature = "telemetry")]
    let metrics = Some(rspk_telemetry::Metrics::new());

    match App::new(
        cli.dry_run,
        cli.quiet,
        cli.mode.clone(),
        cli.feature.clone(),
        #[cfg(feature = "telemetry")]
        metrics,
        rpc_mode,
    )
    .await
    {
        Ok(app) =>
        {
            if let Err(e) = app.run(cli.command).await
            {
                error!("Error: {:#}", e);
                ExitCode::FAILURE
            }
            else
            {
                ExitCode::SUCCESS
            }
        },
        Err(e) =>
        {
            error!("Failed to initialize: {:#}", e);
            ExitCode::FAILURE
        },
    }
}
