//! Individual command handlers (inventory, install, search, etc.).
//!
//! Each public function in this module corresponds to one CLI
//! subcommand and takes the shared [`App`] as its first argument.

use anyhow::Result;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use rspk_api::resolver::{find_candidates, split_package_spec};
use rspk_core::PackageManager;
use std::sync::Arc;
use tokio::task::JoinSet;

use crate::app::App;

/// Lists available package managers.
pub async fn inventory(app: &App) -> Result<()>
{
    let managers = app.registry.available();
    if managers.is_empty()
    {
        println!("No package managers found.");
        return Ok(());
    }
    println!("Available package managers:");
    for mgr in managers
    {
        println!("  ✓ {} ({}) [{:?}]", mgr.name(), mgr.id(), mgr.priority());
    }
    Ok(())
}

/// Lists installed packages across all managers (parallel).
pub async fn installed(app: &App) -> Result<()>
{
    let mut set: JoinSet<Result<(String, Vec<rspk_core::Package>)>> =
        JoinSet::new();
    for mgr in app.registry.available()
    {
        let name = mgr.name().to_string();
        let mgr = mgr.clone();
        let ctx = app.ctx.clone();
        set.spawn(async move { Ok((name, mgr.installed(&ctx).await?)) });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await
    {
        results.push(res??);
    }

    // Deterministic order: sort by manager name
    results.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, packages) in results
    {
        if packages.is_empty() && !app.ctx.dry_run
        {
            continue;
        }
        println!("{name}:");
        for pkg in packages
        {
            let version = pkg
                .installed_version
                .as_ref()
                .map_or_else(|| "unknown".to_string(), ToString::to_string);
            println!("  {} {}", pkg.id, version);
        }
    }
    Ok(())
}

/// Lists outdated packages across all managers (parallel).
pub async fn outdated(app: &App) -> Result<()>
{
    let mut set: JoinSet<Result<(String, Vec<rspk_core::Package>)>> =
        JoinSet::new();
    for mgr in app.registry.available()
    {
        let name = mgr.name().to_string();
        let mgr = mgr.clone();
        let ctx = app.ctx.clone();
        set.spawn(async move { Ok((name, mgr.outdated(&ctx).await?)) });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await
    {
        results.push(res??);
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, packages) in results
    {
        if packages.is_empty() && !app.ctx.dry_run
        {
            continue;
        }
        println!("{name}:");
        for pkg in packages
        {
            let installed = pkg
                .installed_version
                .as_ref()
                .map_or_else(|| "unknown".to_string(), ToString::to_string);
            let latest = pkg
                .latest_version
                .as_ref()
                .map_or_else(|| "unknown".to_string(), ToString::to_string);
            println!("  {} {} -> {}", pkg.id, installed, latest);
        }
    }
    Ok(())
}

/// Installs a package using an explicit or auto-selected manager.
pub async fn install(
    app: &App,
    package: &str,
    manager: Option<&str>,
    no_resolve: bool,
) -> Result<()>
{
    let (raw_name, version) = split_package_spec(package);

    if let Some(mgr_id) = manager
    {
        let mgr = app.registry.get(mgr_id)?;
        let resolved_id = if no_resolve || !mgr.capabilities().resolve
        {
            raw_name.to_string()
        }
        else
        {
            match mgr.resolve(raw_name, &app.ctx).await?
            {
                Some(id) => id,
                None =>
                {
                    if app.ctx.quiet || app.ctx.dry_run
                    {
                        raw_name.to_string()
                    }
                    else
                    {
                        anyhow::bail!(
                            "Could not resolve '{}' in {}. Use --no-resolve \
                             to pass as-is.",
                            raw_name,
                            mgr.name()
                        )
                    }
                },
            }
        };
        if resolved_id != raw_name
        {
            println!(
                "→ resolved '{}' to '{}' in {}",
                raw_name,
                resolved_id,
                mgr.name()
            );
        }
        println!("Installing {} using {}...", resolved_id, mgr.name());
        let output = mgr.install(&resolved_id, version, &app.ctx).await?;
        if !output.is_empty()
        {
            println!("{output}");
        }
        return Ok(());
    }

    let candidates =
        find_candidates(&app.registry, package, &app.ctx, no_resolve).await?;
    let selected = app.select_candidate(candidates)?;
    println!(
        "Installing {} using {}...",
        selected.resolved_id,
        selected.manager.name()
    );
    let output = selected
        .manager
        .install(&selected.resolved_id, version, &app.ctx)
        .await?;
    if !output.is_empty()
    {
        println!("{output}");
    }
    Ok(())
}

/// Upgrades packages using an explicit or auto-selected manager.
pub async fn upgrade(
    app: &App,
    package: Option<&str>,
    manager: Option<&str>,
    no_resolve: bool,
) -> Result<()>
{
    let mgr = if let Some(mgr_id) = manager
    {
        app.registry.get(mgr_id)?
    }
    else if let Some(spec) = package
    {
        let candidates =
            find_candidates(&app.registry, spec, &app.ctx, no_resolve).await?;
        let selected = app.select_candidate(candidates)?;
        let output = selected
            .manager
            .upgrade(Some(&selected.resolved_id), &app.ctx)
            .await?;
        if !output.is_empty()
        {
            println!("{output}");
        }
        return Ok(());
    }
    else
    {
        anyhow::bail!("Either specify --manager or provide a package name");
    };

    let resolved = if let Some(spec) = package
    {
        let (raw_name, _) = split_package_spec(spec);
        if no_resolve || !mgr.capabilities().resolve
        {
            Some(raw_name.to_string())
        }
        else
        {
            mgr.resolve(raw_name, &app.ctx).await?
        }
    }
    else
    {
        None
    };

    let label = resolved.as_deref().unwrap_or("all packages");
    println!("Upgrading {} using {}...", label, mgr.name());
    let output = mgr.upgrade(resolved.as_deref(), &app.ctx).await?;
    if !output.is_empty()
    {
        println!("{output}");
    }
    Ok(())
}

/// Uninstalls a package using an explicit or auto-detected manager.
pub async fn uninstall(
    app: &App,
    package: &str,
    manager: Option<&str>,
    no_resolve: bool,
) -> Result<()>
{
    let (raw_name, _) = split_package_spec(package);

    let mgr: Arc<dyn PackageManager> = if let Some(mgr_id) = manager
    {
        app.registry.get(mgr_id)?
    }
    else
    {
        let mut installed_managers: Vec<Arc<dyn PackageManager>> = Vec::new();
        for mgr in app.registry.available()
        {
            if !mgr.capabilities().list_installed
            {
                continue;
            }
            let installed = mgr.installed(&app.ctx).await?;
            if installed.iter().any(|p| p.id == raw_name)
            {
                installed_managers.push(mgr);
            }
        }

        if installed_managers.is_empty()
        {
            anyhow::bail!(
                "Package '{raw_name}' is not installed in any manager. Use \
                 --manager to specify."
            );
        }

        if installed_managers.len() == 1 || app.ctx.quiet
        {
            installed_managers[0].clone()
        }
        else
        {
            let items: Vec<String> = installed_managers
                .iter()
                .map(|m| m.name().to_string())
                .collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(
                    "Package installed in multiple managers. Select one:",
                )
                .items(&items)
                .default(0)
                .interact()?;
            installed_managers[selection].clone()
        }
    };

    let resolved_id = if no_resolve || !mgr.capabilities().resolve
    {
        raw_name.to_string()
    }
    else
    {
        mgr.resolve(raw_name, &app.ctx)
            .await?
            .unwrap_or_else(|| raw_name.to_string())
    };

    println!("Uninstalling {} using {}...", resolved_id, mgr.name());
    let output = mgr.remove(&resolved_id, &app.ctx).await?;
    if !output.is_empty()
    {
        println!("{output}");
    }
    Ok(())
}

/// Searches for packages in an explicit or auto-selected manager.
pub async fn search(
    app: &App,
    query: &str,
    manager: Option<&str>,
    extended: bool,
    exact: bool,
) -> Result<()>
{
    let mgr =
        if let Some(mgr_id) = manager
        {
            app.registry.get(mgr_id)?
        }
        else
        {
            app.registry.available().into_iter().next().ok_or_else(|| {
                anyhow::anyhow!("No package manager available")
            })?
        };

    println!("Searching for '{}' using {}...", query, mgr.name());
    let packages = mgr.search(query, extended, exact, &app.ctx).await?;

    if packages.is_empty() && !app.ctx.dry_run
    {
        println!("No packages found.");
        return Ok(());
    }

    for pkg in packages
    {
        let desc = pkg.description.as_deref().unwrap_or("");
        if desc.is_empty()
        {
            println!("{}", pkg.id);
        }
        else
        {
            println!("{} - {}", pkg.id, desc);
        }
    }
    Ok(())
}

/// Synchronizes repositories for an explicit or auto-selected manager.
pub async fn sync(app: &App, manager: Option<&str>) -> Result<()>
{
    let mgr =
        if let Some(mgr_id) = manager
        {
            app.registry.get(mgr_id)?
        }
        else
        {
            app.registry.available().into_iter().next().ok_or_else(|| {
                anyhow::anyhow!("No package manager available")
            })?
        };

    println!("Synchronizing repositories using {}...", mgr.name());
    mgr.sync(&app.ctx).await?;
    if !app.ctx.dry_run
    {
        println!("Done.");
    }
    Ok(())
}

/// Cleans caches for an explicit or auto-selected manager.
pub async fn cleanup(app: &App, manager: Option<&str>) -> Result<()>
{
    let mgr =
        if let Some(mgr_id) = manager
        {
            app.registry.get(mgr_id)?
        }
        else
        {
            app.registry.available().into_iter().next().ok_or_else(|| {
                anyhow::anyhow!("No package manager available")
            })?
        };

    println!("Cleaning up using {}...", mgr.name());
    mgr.cleanup(&app.ctx).await?;
    if !app.ctx.dry_run
    {
        println!("Done.");
    }
    Ok(())
}

/// Resolves an abstract package name across one or all managers.
pub async fn resolve(app: &App, name: &str, manager: Option<&str>)
-> Result<()>
{
    if let Some(mgr_id) = manager
    {
        let mgr = app.registry.get(mgr_id)?;
        if !mgr.capabilities().resolve
        {
            anyhow::bail!("Manager '{mgr_id}' does not support resolution");
        }
        println!("Resolving '{}' in {}...", name, mgr.name());
        match mgr.resolve(name, &app.ctx).await?
        {
            Some(resolved) => println!("✓ Found: {resolved}"),
            None => println!("✗ Not found in {}", mgr.name()),
        }
    }
    else
    {
        println!("Resolving '{name}' across all managers...");
        let results = app.registry.resolve_all(name, &app.ctx).await;
        if results.is_empty()
        {
            println!("No managers support resolution.");
            return Ok(());
        }
        for (mgr_id, resolved) in results
        {
            match resolved
            {
                Some(n) => println!("✓ {mgr_id}: {n}"),
                None => println!("✗ {mgr_id}: not found"),
            }
        }
    }
    Ok(())
}

/// Generates an SBOM from all installed packages.
pub async fn sbom(
    app: &App,
    format_str: &str,
    output_path: Option<&str>,
    manager: Option<&str>,
    component_name: Option<&str>,
    component_version: Option<&str>,
) -> Result<()>
{
    use rspk_sbom::{SbomFormat, SbomOptions, generate};

    let format: SbomFormat =
        format_str.parse().map_err(|e: String| anyhow::anyhow!(e))?;

    let opts = SbomOptions {
        manager:           manager.map(String::from),
        component_name:    component_name.map(String::from),
        component_version: component_version.map(String::from),
    };

    let sbom = generate(&app.registry, &app.ctx, Some(opts)).await?;

    let json = sbom.to_json(format)?;

    if let Some(path) = output_path
    {
        std::fs::write(path, &json)?;
        println!(
            "✓ SBOM written to {} ({} components, {} format)",
            path,
            sbom.component_count(),
            format
        );
    }
    else
    {
        println!("{json}");
    }

    Ok(())
}
