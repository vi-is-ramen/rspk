//! Package resolution logic.
//!
//! Resolves abstract package names (e.g. `"lodash"`, `"curl=8.4.0"`)
//! into concrete manager + package-ID pairs by probing all available
//! managers in parallel.

use anyhow::Result;
use rspk_core::{ExecutionContext, PackageManager, Priority};
use rspk_managers::ManagerRegistry;
use std::sync::Arc;
use tokio::task::JoinSet;

/// A candidate manager for installing a package, along with the
/// resolved package ID in that manager.
///
/// Returned by [`find_candidates`] after probing all available
/// managers in parallel.
#[derive(Debug, Clone)]
pub struct Candidate
{
    /// The package manager that can handle this package.
    pub manager:     Arc<dyn PackageManager>,
    /// The resolved (manager-specific) package ID.
    pub resolved_id: String,
    /// The priority of this manager for automatic selection.
    pub priority:    Priority,
}

/// Splits a package spec into name and optional version.
///
/// Only `=` is used as a separator. The `@` character is treated
/// as part of the package name or version, so `pkg@scope` is a
/// valid package name (common in npm scoped packages like
/// `@angular/core`).
///
/// # Examples
///
/// - `"lodash"` → `("lodash", None)`
/// - `"lodash=4.17.21"` → `("lodash", Some("4.17.21"))`
/// - `"@angular/core"` → `("@angular/core", None)`
/// - `"@angular/core=16.0.0"` → `("@angular/core", Some("16.0.0"))`
pub fn split_package_spec(spec: &str) -> (&str, Option<&str>)
{
    if let Some((n, v)) = spec.split_once('=')
    {
        (n, Some(v))
    }
    else
    {
        (spec, None)
    }
}

/// Finds candidate managers for a package spec.
///
/// Resolution across managers is executed in parallel via
/// [`JoinSet`]. The returned vector is sorted by [`Priority`]
/// (lowest value = highest priority = first element).
pub async fn find_candidates(
    registry: &ManagerRegistry,
    package_spec: &str,
    ctx: &ExecutionContext,
    no_resolve: bool,
) -> Result<Vec<Candidate>>
{
    let (raw_name, _version) = split_package_spec(package_spec);
    let mut set: JoinSet<Result<Option<Candidate>>> = JoinSet::new();

    for mgr in registry.available()
    {
        if !mgr.capabilities().install
        {
            continue;
        }
        let priority = mgr.priority();
        let mgr = mgr.clone();
        let raw = raw_name.to_string();
        let ctx = ctx.clone();
        let skip_resolve = no_resolve || !mgr.capabilities().resolve;

        set.spawn(async move {
            let resolved_id = if skip_resolve
            {
                Some(raw.clone())
            }
            else
            {
                mgr.resolve(&raw, &ctx).await?
            };
            Ok(resolved_id.map(|id| Candidate {
                manager: mgr,
                resolved_id: id,
                priority,
            }))
        });
    }

    let mut candidates = Vec::new();
    while let Some(res) = set.join_next().await
    {
        if let Some(c) = res??
        {
            candidates.push(c);
        }
    }
    candidates.sort_by_key(|c| c.priority);
    Ok(candidates)
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_split_no_version()
    {
        assert_eq!(split_package_spec("lodash"), ("lodash", None));
    }

    #[test]
    fn test_split_with_version()
    {
        assert_eq!(
            split_package_spec("lodash=4.17.21"),
            ("lodash", Some("4.17.21"))
        );
    }

    #[test]
    fn test_split_scoped_package()
    {
        assert_eq!(
            split_package_spec("@angular/core"),
            ("@angular/core", None)
        );
    }

    #[test]
    fn test_split_scoped_with_version()
    {
        assert_eq!(
            split_package_spec("@angular/core=16.0.0"),
            ("@angular/core", Some("16.0.0"))
        );
    }
}
