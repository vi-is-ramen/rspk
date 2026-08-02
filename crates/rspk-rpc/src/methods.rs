//! Method registration and dispatch.

use crate::protocol::{RpcError, Value};
use rspk_api::resolver::{find_candidates, split_package_spec};
use rspk_api::satisfy::{self, PrioritySelector};
use rspk_core::{ExecutionContext, PackageManager};
use rspk_managers::ManagerRegistry;
use rspk_needsfile::EvalContext;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Shared application state passed to every RPC method.
#[derive(Debug, Clone)]
pub struct RpcContext
{
    /// Discovered package managers.
    pub registry: Arc<ManagerRegistry>,
    /// Execution flags (dry-run, quiet).
    pub ctx:      ExecutionContext,
    /// Evaluation context for Needsfile conditions (OS, managers, etc.).
    pub eval_ctx: EvalContext,
}

/// Type alias for an async RPC method handler.
pub type MethodHandler =
    fn(
        RpcContext,
        Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>;

/// Registry of available RPC methods.
#[derive(Debug, Clone)]
pub struct MethodRegistry
{
    handlers:     HashMap<&'static str, MethodHandler>,
    descriptions: HashMap<&'static str, &'static str>,
}

impl MethodRegistry
{
    /// Builds the default registry with all built-in methods.
    pub fn builtin() -> Self
    {
        let mut r = Self {
            handlers:     HashMap::new(),
            descriptions: HashMap::new(),
        };
        r.register(
            "system.listMethods",
            methods::list_methods,
            "List all available RPC methods",
        );
        r.register(
            "system.describe",
            methods::describe_method,
            "Describe a method (params, result)",
        );
        r.register(
            "inventory",
            methods::inventory,
            "List discovered package managers",
        );
        r.register(
            "installed",
            methods::installed,
            "List installed packages across all managers",
        );
        r.register(
            "outdated",
            methods::outdated,
            "List outdated packages across all managers",
        );
        r.register(
            "search",
            methods::search,
            "Search for packages in a manager",
        );
        r.register(
            "resolve",
            methods::resolve,
            "Resolve an abstract package name across managers",
        );
        r.register("install", methods::install, "Install a package");
        r.register(
            "upgrade",
            methods::upgrade,
            "Upgrade a package (or all packages)",
        );
        r.register("uninstall", methods::uninstall, "Uninstall a package");
        r.register("sync", methods::sync, "Synchronize package repositories");
        r.register("cleanup", methods::cleanup, "Clean up caches");
        r.register(
            "satisfy",
            methods::satisfy,
            "Install packages from a Needsfile",
        );
        r.register("sbom", methods::sbom, "Generate SBOM (CycloneDX or SPDX)");
        r
    }

    /// Registers a handler under the given name.
    pub fn register(
        &mut self,
        name: &'static str,
        handler: MethodHandler,
        description: &'static str,
    )
    {
        self.handlers.insert(name, handler);
        self.descriptions.insert(name, description);
    }

    /// Dispatches a method call.
    pub async fn call(
        &self,
        ctx: RpcContext,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, RpcError>
    {
        let handler = self
            .handlers
            .get(method)
            .ok_or_else(|| RpcError::method_not_found(method))?;
        handler(ctx, params).await
    }

    /// Returns the list of registered method names.
    pub fn names(&self) -> Vec<&'static str>
    {
        let mut v: Vec<_> = self.handlers.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// Returns the description of a method, if registered.
    pub fn description(&self, name: &str) -> Option<&'static str>
    {
        self.descriptions.get(name).copied()
    }
}

/// Built-in method implementations.
#[allow(clippy::module_inception)]
mod methods
{
    #[allow(clippy::wildcard_imports)]
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    /// Helper: parse `params` into a typed struct, or return `InvalidParams`.
    fn parse_params<T: for<'de> Deserialize<'de>>(
        params: Option<Value>,
    ) -> Result<T, RpcError>
    {
        let v = params.unwrap_or(Value::Null);
        serde_json::from_value(v).map_err(|e| {
            RpcError::invalid_params(format!("invalid params: {e}"))
        })
    }

    /// Helper: convert manager errors into internal RPC errors.
    #[allow(clippy::needless_pass_by_value)]
    fn map_err(e: rspk_core::Error) -> RpcError
    {
        RpcError::internal(e.to_string())
    }

    /// Helper: serialize a value via serde, mapping errors.
    fn to_value<T: Serialize>(v: T) -> Result<Value, RpcError>
    {
        serde_json::to_value(v).map_err(|e| RpcError::internal(e.to_string()))
    }

    pub fn list_methods(
        _ctx: RpcContext,
        _params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            Ok(json!({
                "methods": [
                    "system.listMethods", "system.describe",
                    "inventory", "installed", "outdated",
                    "search", "resolve", "install", "upgrade",
                    "uninstall", "sync", "cleanup", "satisfy",
                    "sbom"
                ]
            }))
        })
    }

    pub fn describe_method(
        _ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                method: String,
            }
            let p: P = parse_params(params)?;
            let (params_schema, result_schema) = match p.method.as_str()
            {
                "inventory" => (
                    "null",
                    "{managers: [{id, name, priority, platforms, \
                     capabilities}]}",
                ),
                "installed" => (
                    "{manager?: string}",
                    "{packages: [{id, name, version, manager, arch?}]}",
                ),
                "outdated" => (
                    "{manager?: string}",
                    "{packages: [{id, installed, latest, manager}]}",
                ),
                "search" => (
                    "{query, manager?, extended?, exact?}",
                    "{packages: [{id, description?, version?, manager}]}",
                ),
                "resolve" => (
                    "{name, manager?}",
                    "{results: {manager_id: resolved_name|null}}",
                ),
                "install" => (
                    "{package, manager?, version?, no_resolve?}",
                    "{installed: bool, resolved_id, manager, output}",
                ),
                "upgrade" => (
                    "{package?, manager?, no_resolve?}",
                    "{upgraded: bool, output}",
                ),
                "uninstall" => (
                    "{package, manager?, no_resolve?}",
                    "{removed: bool, output}",
                ),
                "sync" => ("{manager?}", "{synced: bool}"),
                "cleanup" => ("{manager?}", "{cleaned: bool}"),
                "satisfy" => (
                    "{path, no_resolve?}",
                    "{installed, skipped, failed: [{package, error}]}",
                ),
                "system.listMethods" => ("null", "{methods: [string]}"),
                "system.describe" => (
                    "{method: string}",
                    "{method, description, params_schema, result_schema}",
                ),
                "sbom" => (
                    "{format?, manager?, component_name?, component_version?}",
                    "{format, components, sbom: {...}}",
                ),
                _ => return Err(RpcError::method_not_found(&p.method)),
            };
            Ok(json!({
                "method": p.method,
                "params_schema": params_schema,
                "result_schema": result_schema
            }))
        })
    }

    pub fn inventory(
        ctx: RpcContext,
        _params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Serialize)]
            struct ManagerInfo
            {
                id:           &'static str,
                name:         &'static str,
                priority:     rspk_core::Priority,
                platforms:    Vec<rspk_core::Platform>,
                capabilities: Vec<&'static str>,
            }
            let managers: Vec<ManagerInfo> = ctx
                .registry
                .available()
                .into_iter()
                .map(|m| {
                    let caps = m.capabilities();
                    let mut c = Vec::new();
                    if caps.list_installed
                    {
                        c.push("list_installed");
                    }
                    if caps.detect_outdated
                    {
                        c.push("detect_outdated");
                    }
                    if caps.search
                    {
                        c.push("search");
                    }
                    if caps.install
                    {
                        c.push("install");
                    }
                    if caps.upgrade_one
                    {
                        c.push("upgrade_one");
                    }
                    if caps.upgrade_all
                    {
                        c.push("upgrade_all");
                    }
                    if caps.remove
                    {
                        c.push("remove");
                    }
                    if caps.sync
                    {
                        c.push("sync");
                    }
                    if caps.cleanup
                    {
                        c.push("cleanup");
                    }
                    if caps.resolve
                    {
                        c.push("resolve");
                    }
                    ManagerInfo {
                        id:           m.id(),
                        name:         m.name(),
                        priority:     m.priority(),
                        platforms:    m.platforms().to_vec(),
                        capabilities: c,
                    }
                })
                .collect();
            to_value(json!({ "managers": managers }))
        })
    }

    pub fn installed(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                manager: Option<String>,
            }
            #[derive(Serialize)]
            struct Pkg
            {
                id:      String,
                name:    Option<String>,
                version: Option<String>,
                manager: String,
                arch:    Option<String>,
            }
            let p: P = parse_params(params).unwrap_or_default();
            let managers: Vec<Arc<dyn PackageManager>> = if let Some(id) =
                &p.manager
            {
                vec![ctx.registry.get(id).map_err(map_err)?]
            }
            else
            {
                ctx.registry.available()
            };
            let mut packages = Vec::new();
            for m in managers
            {
                let list = m.installed(&ctx.ctx).await.map_err(map_err)?;
                for pkg in list
                {
                    packages.push(Pkg {
                        id:      pkg.id,
                        name:    pkg.name,
                        version: pkg.installed_version.map(|v| v.to_string()),
                        manager: m.id().to_string(),
                        arch:    pkg.arch,
                    });
                }
            }
            to_value(json!({ "packages": packages }))
        })
    }

    pub fn outdated(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                manager: Option<String>,
            }
            #[derive(Serialize)]
            struct Pkg
            {
                id:        String,
                installed: Option<String>,
                latest:    Option<String>,
                manager:   String,
            }
            let p: P = parse_params(params).unwrap_or_default();
            let managers: Vec<Arc<dyn PackageManager>> = if let Some(id) =
                &p.manager
            {
                vec![ctx.registry.get(id).map_err(map_err)?]
            }
            else
            {
                ctx.registry.available()
            };
            let mut packages = Vec::new();
            for m in managers
            {
                let list = m.outdated(&ctx.ctx).await.map_err(map_err)?;
                for pkg in list
                {
                    packages.push(Pkg {
                        id:        pkg.id,
                        installed: pkg.installed_version.map(|v| v.to_string()),
                        latest:    pkg.latest_version.map(|v| v.to_string()),
                        manager:   m.id().to_string(),
                    });
                }
            }
            to_value(json!({ "packages": packages }))
        })
    }

    pub fn search(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                query:    String,
                manager:  Option<String>,
                #[serde(default)]
                extended: bool,
                #[serde(default)]
                exact:    bool,
            }
            #[derive(Serialize)]
            struct Pkg
            {
                id:          String,
                description: Option<String>,
                version:     Option<String>,
                manager:     String,
            }
            let p: P = parse_params(params)?;
            let m: Arc<dyn PackageManager> = if let Some(id) = &p.manager
            {
                ctx.registry.get(id).map_err(map_err)?
            }
            else
            {
                ctx.registry.available().into_iter().next().ok_or_else(
                    || RpcError::internal("no managers available"),
                )?
            };
            let results = m
                .search(&p.query, p.extended, p.exact, &ctx.ctx)
                .await
                .map_err(map_err)?;
            let packages: Vec<Pkg> = results
                .into_iter()
                .map(|pkg| Pkg {
                    id:          pkg.id,
                    description: pkg.description,
                    version:     pkg.latest_version.map(|v| v.to_string()),
                    manager:     m.id().to_string(),
                })
                .collect();
            to_value(json!({ "packages": packages }))
        })
    }

    pub fn resolve(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                name:    String,
                manager: Option<String>,
            }
            let p: P = parse_params(params)?;
            let results = if let Some(id) = &p.manager
            {
                let m = ctx.registry.get(id).map_err(map_err)?;
                let r = m.resolve(&p.name, &ctx.ctx).await.map_err(map_err)?;
                let mut map = serde_json::Map::new();
                map.insert(id.clone(), r.map_or(Value::Null, Value::String));
                map
            }
            else
            {
                let r = ctx.registry.resolve_all(&p.name, &ctx.ctx).await;
                let mut map = serde_json::Map::new();
                for (k, v) in r
                {
                    map.insert(
                        k.to_string(),
                        v.map_or(Value::Null, Value::String),
                    );
                }
                map
            };
            Ok(Value::Object(serde_json::Map::from_iter([(
                "results".to_string(),
                Value::Object(results),
            )])))
        })
    }

    pub fn install(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                package:    String,
                manager:    Option<String>,
                version:    Option<String>,
                #[serde(default)]
                no_resolve: bool,
            }
            let p: P = parse_params(params)?;
            let (m, resolved_id) = resolve_package(
                &ctx,
                &p.package,
                p.manager.as_deref(),
                p.no_resolve,
            )
            .await?;
            let output = m
                .install(&resolved_id, p.version.as_deref(), &ctx.ctx)
                .await
                .map_err(map_err)?;
            to_value(json!({
                "installed": true,
                "resolved_id": resolved_id,
                "manager": m.id(),
                "output": output
            }))
        })
    }

    pub fn upgrade(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                package:    Option<String>,
                manager:    Option<String>,
                #[serde(default)]
                no_resolve: bool,
            }
            let p: P = parse_params(params).unwrap_or_default();
            let (m, pkg_id) = if let Some(id) = &p.manager
            {
                let m = ctx.registry.get(id).map_err(map_err)?;
                let resolved = if let Some(spec) = &p.package
                {
                    if p.no_resolve || !m.capabilities().resolve
                    {
                        Some(spec.clone())
                    }
                    else
                    {
                        m.resolve(spec, &ctx.ctx).await.map_err(map_err)?
                    }
                }
                else
                {
                    None
                };
                (m, resolved)
            }
            else if let Some(spec) = &p.package
            {
                let (m, id) =
                    resolve_package(&ctx, spec, None, p.no_resolve).await?;
                (m, Some(id))
            }
            else
            {
                return Err(RpcError::invalid_params(
                    "either `manager` or `package` is required",
                ));
            };
            let output = m
                .upgrade(pkg_id.as_deref(), &ctx.ctx)
                .await
                .map_err(map_err)?;
            to_value(json!({ "upgraded": true, "output": output }))
        })
    }

    pub fn uninstall(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                package:    String,
                manager:    Option<String>,
                #[serde(default)]
                no_resolve: bool,
            }
            let p: P = parse_params(params)?;
            let (m, resolved_id) = resolve_package(
                &ctx,
                &p.package,
                p.manager.as_deref(),
                p.no_resolve,
            )
            .await?;
            let output =
                m.remove(&resolved_id, &ctx.ctx).await.map_err(map_err)?;
            to_value(json!({ "removed": true, "output": output }))
        })
    }

    pub fn sync(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                manager: Option<String>,
            }
            let p: P = parse_params(params).unwrap_or_default();
            let m = select_manager(&ctx, p.manager.as_deref())?;
            m.sync(&ctx.ctx).await.map_err(map_err)?;
            to_value(json!({ "synced": true }))
        })
    }

    pub fn cleanup(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                manager: Option<String>,
            }
            let p: P = parse_params(params).unwrap_or_default();
            let m = select_manager(&ctx, p.manager.as_deref())?;
            m.cleanup(&ctx.ctx).await.map_err(map_err)?;
            to_value(json!({ "cleaned": true }))
        })
    }

    pub fn satisfy(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct P
            {
                path:       String,
                #[serde(default)]
                no_resolve: bool,
            }
            #[derive(Serialize)]
            struct Fail
            {
                package: String,
                error:   String,
            }

            let p: P = parse_params(params)?;

            let summary = satisfy::satisfy(
                &ctx.registry,
                &ctx.ctx,
                &ctx.eval_ctx,
                &p.path,
                p.no_resolve,
                &PrioritySelector,
            )
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;

            let failed: Vec<Fail> = summary
                .failed
                .into_iter()
                .map(|(package, error)| Fail { package, error })
                .collect();

            to_value(json!({
                "installed": summary.installed,
                "skipped": summary.skipped,
                "failed": failed
            }))
        })
    }

    pub fn sbom(
        ctx: RpcContext,
        params: Option<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RpcError>> + Send>>
    {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct P
            {
                #[serde(default = "default_format")]
                format:            String,
                manager:           Option<String>,
                component_name:    Option<String>,
                component_version: Option<String>,
            }
            fn default_format() -> String
            {
                "cyclonedx".to_string()
            }

            let p: P = parse_params(params).unwrap_or_default();
            let format: rspk_sbom::SbomFormat = p
                .format
                .parse()
                .map_err(|e: String| RpcError::invalid_params(e))?;

            let opts = rspk_sbom::SbomOptions {
                manager:           p.manager,
                component_name:    p.component_name,
                component_version: p.component_version,
            };

            let sbom = rspk_sbom::generate(&ctx.registry, &ctx.ctx, Some(opts))
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;

            let json_str = sbom
                .to_json(format)
                .map_err(|e| RpcError::internal(e.to_string()))?;

            // Return as raw JSON string (the SBOM is already JSON)
            Ok(json!({
                "format": format.to_string(),
                "components": sbom.component_count(),
                "sbom": serde_json::from_str::<Value>(&json_str)
                    .unwrap_or(Value::String(json_str))
            }))
        })
    }

    /// Internal: resolve package name to (manager, `resolved_id`).
    async fn resolve_package(
        ctx: &RpcContext,
        package: &str,
        manager: Option<&str>,
        no_resolve: bool,
    ) -> Result<(Arc<dyn PackageManager>, String), RpcError>
    {
        let (raw_name, _version) = split_package_spec(package);

        if let Some(id) = manager
        {
            let m = ctx.registry.get(id).map_err(map_err)?;
            let resolved = if no_resolve || !m.capabilities().resolve
            {
                raw_name.to_string()
            }
            else
            {
                m.resolve(raw_name, &ctx.ctx)
                    .await
                    .map_err(map_err)?
                    .unwrap_or_else(|| raw_name.to_string())
            };
            Ok((m, resolved))
        }
        else
        {
            let candidates =
                find_candidates(&ctx.registry, package, &ctx.ctx, no_resolve)
                    .await
                    .map_err(|e| RpcError::internal(e.to_string()))?;

            let selected = candidates.into_iter().next().ok_or_else(|| {
                RpcError::invalid_params(format!(
                    "could not resolve '{raw_name}' in any manager",
                ))
            })?;

            Ok((selected.manager, selected.resolved_id))
        }
    }

    /// Internal: pick a manager by id, or the first available.
    fn select_manager(
        ctx: &RpcContext,
        manager: Option<&str>,
    ) -> Result<Arc<dyn PackageManager>, RpcError>
    {
        if let Some(id) = manager
        {
            ctx.registry.get(id).map_err(map_err)
        }
        else
        {
            ctx.registry
                .available()
                .into_iter()
                .next()
                .ok_or_else(|| RpcError::internal("no managers available"))
        }
    }
}
