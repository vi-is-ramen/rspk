//! Needsfile satisfaction pipeline.
//!
//! Reads a Needsfile, resolves each entry across managers, and
//! installs missing packages. Different managers are processed
//! in parallel; within a single manager, entries are installed
//! sequentially to avoid lock conflicts (dpkg, rpm, etc.).
//!
//! The manager selection strategy is injected via [`ManagerSelector`],
//! allowing the CLI to prompt the user interactively while the RPC
//! server always picks by priority.

use anyhow::Result;
use rspk_core::{ExecutionContext, PackageManager};
use rspk_managers::ManagerRegistry;
use rspk_needsfile::{EvalContext, NeedsEntry, resolve_needsfile};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::error;

use crate::resolver::{Candidate, find_candidates};

/// Result of satisfying a single [`NeedsEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SatisfyResult
{
    /// Package was successfully installed.
    Installed,
    /// Package was already installed.
    AlreadyInstalled,
}

/// Aggregate result of satisfying an entire Needsfile.
#[derive(Debug, Clone)]
pub struct SatisfySummary
{
    /// Total number of packages installed.
    pub installed: usize,
    /// Total number of packages already present (skipped).
    pub skipped:   usize,
    /// Packages that failed to install: `(package_name, error_message)`.
    pub failed:    Vec<(String, String)>,
    /// Total number of entries in the Needsfile.
    pub total:     usize,
}

impl SatisfySummary
{
    /// Returns `true` if all packages were satisfied without errors.
    pub fn is_success(&self) -> bool
    {
        self.failed.is_empty()
    }
}

/// Strategy for choosing a manager when multiple candidates exist.
///
/// The CLI provides an interactive implementation (using `dialoguer`);
/// the RPC server uses [`PrioritySelector`] which always picks the
/// highest-priority candidate.
pub trait ManagerSelector: Send + Sync
{
    /// Selects one candidate from a non-empty list.
    ///
    /// # Errors
    ///
    /// Returns an error if the user cancels the selection or if
    /// the list is empty.
    fn select(&self, candidates: Vec<Candidate>) -> Result<Candidate>;
}

/// Non-interactive selector: always picks the first (highest-priority)
/// candidate.
///
/// Used by the RPC server and by the CLI in `--quiet` mode.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrioritySelector;

impl ManagerSelector for PrioritySelector
{
    fn select(&self, candidates: Vec<Candidate>) -> Result<Candidate>
    {
        candidates.into_iter().next().ok_or_else(|| {
            anyhow::anyhow!("No package manager has this package available")
        })
    }
}

/// Satisfies package requirements from a Needsfile.
///
/// Reads the Needsfile, parses each entry, and ensures all
/// packages are installed (and optionally up-to-date).
///
/// Entries are grouped by manager and processed in parallel across
/// different managers. Within a single manager, entries are processed
/// sequentially to avoid lock conflicts (dpkg, rpm, etc.).
///
/// When an entry has no explicit manager, `selector` is used to
/// choose among candidates.
#[allow(clippy::too_many_lines)]
pub async fn satisfy(
    registry: &ManagerRegistry,
    ctx: &ExecutionContext,
    eval_ctx: &EvalContext,
    path: &str,
    no_resolve: bool,
    selector: &dyn ManagerSelector,
) -> Result<SatisfySummary>
{
    let entries = match resolve_needsfile(path, eval_ctx)
    {
        Ok(entries) => entries,
        Err(e) =>
        {
            anyhow::bail!(
                "failed to parse Needsfile '{}' ({} error{}): {}",
                path,
                e.errors.len(),
                if e.errors.len() == 1 { "" } else { "s" },
                e.render()
            );
        },
    };

    let total = entries.len();
    if entries.is_empty()
    {
        return Ok(SatisfySummary {
            installed: 0,
            skipped:   0,
            failed:    Vec::new(),
            total:     0,
        });
    }

    // Phase 1: determine manager + resolved_id for each entry.
    // Group by manager for sequential installation within each group.
    #[allow(clippy::type_complexity)]
    let mut by_manager: HashMap<
        String,
        Vec<(NeedsEntry, String, Arc<dyn PackageManager>)>,
    > = HashMap::new();

    for entry in &entries
    {
        let (mgr_id, resolved_id, mgr_arc) = if let Some(mgr_id) =
            &entry.manager
        {
            let mgr = registry.get(mgr_id)?;
            let resolved = if no_resolve || !mgr.capabilities().resolve
            {
                entry.package.clone()
            }
            else
            {
                mgr.resolve(&entry.package, ctx)
                    .await?
                    .unwrap_or_else(|| entry.package.clone())
            };
            (mgr_id.clone(), resolved, mgr)
        }
        else
        {
            let candidates =
                find_candidates(registry, &entry.package, ctx, no_resolve)
                    .await?;
            let selected = selector.select(candidates)?;
            let id = selected.manager.id().to_string();
            let resolved = selected.resolved_id.clone();
            let arc = selected.manager.clone();
            (id, resolved, arc)
        };
        by_manager.entry(mgr_id).or_default().push((
            entry.clone(),
            resolved_id,
            mgr_arc,
        ));
    }

    // Phase 2: install in parallel across different managers.
    #[allow(clippy::type_complexity)]
    let mut set: JoinSet<
        Result<(String, usize, usize, Vec<(String, String)>)>,
    > = JoinSet::new();

    for (_mgr_id, group) in by_manager
    {
        let ctx = ctx.clone();
        let mgr_name = group[0].2.name().to_string();
        let mgr = group[0].2.clone();

        set.spawn(async move {
            let mut installed = 0;
            let mut skipped = 0;
            let mut failed: Vec<(String, String)> = Vec::new();

            for (entry, resolved_id, _mgr_arc) in group
            {
                match satisfy_entry(&*mgr, &entry, &resolved_id, &ctx).await
                {
                    Ok(SatisfyResult::Installed) => installed += 1,
                    Ok(SatisfyResult::AlreadyInstalled) => skipped += 1,
                    Err(e) =>
                    {
                        failed.push((entry.package.clone(), e.to_string()))
                    },
                }
            }
            Ok((mgr_name, installed, skipped, failed))
        });
    }

    let mut total_installed = 0;
    let mut total_skipped = 0;
    let mut all_failed: Vec<(String, String)> = Vec::new();

    while let Some(res) = set.join_next().await
    {
        let (mgr_name, installed, skipped, failed) = res??;
        total_installed += installed;
        total_skipped += skipped;
        for (pkg, err) in &failed
        {
            error!("Failed to satisfy '{}' via {}: {}", pkg, mgr_name, err);
        }
        all_failed.extend(failed);
    }

    Ok(SatisfySummary {
        installed: total_installed,
        skipped: total_skipped,
        failed: all_failed,
        total,
    })
}

/// Satisfies a single [`NeedsEntry`] using a specific manager and
/// resolved ID.
async fn satisfy_entry(
    mgr: &dyn PackageManager,
    entry: &NeedsEntry,
    resolved_id: &str,
    ctx: &ExecutionContext,
) -> Result<SatisfyResult>
{
    // Check if already installed.
    if mgr.capabilities().list_installed
    {
        let installed = mgr.installed(ctx).await?;
        if let Some(pkg) = installed.iter().find(|p| p.id == *resolved_id)
        {
            if let Some(required_version) = &entry.version
            {
                if let Some(installed_version) = &pkg.installed_version
                {
                    if installed_version.to_string() == *required_version
                    {
                        return Ok(SatisfyResult::AlreadyInstalled);
                    }
                }
            }
            else
            {
                return Ok(SatisfyResult::AlreadyInstalled);
            }
        }
    }

    // Install the package.
    let _output = mgr
        .install(resolved_id, entry.version.as_deref(), ctx)
        .await?;

    Ok(SatisfyResult::Installed)
}
