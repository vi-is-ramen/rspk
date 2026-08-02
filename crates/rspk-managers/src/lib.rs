//! Package manager implementations and registry.

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

pub mod apk;
pub mod apt;
pub mod aptitude;
pub mod brew;
pub mod cargo;
pub mod choco;
pub mod dnf;
pub mod flatpak;
pub mod freebsd_pkg;
pub mod gems;
pub mod macports;
pub mod netbsd_pkg;
pub mod nix;
pub mod npm;
pub mod openbsd_pkg;
pub mod pacman;
pub mod paru;
pub mod scoop;
pub mod snap;
pub mod termux_pkg;
pub mod winget;
pub mod xbps;
pub mod yay;
pub mod yum;
pub mod zypper;

use rspk_core::{Error, ExecutionContext, PackageManager, Platform};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;

/// A registry of all known package managers.
#[derive(Debug)]
pub struct ManagerRegistry
{
    managers: HashMap<&'static str, Arc<dyn PackageManager>>,
    platform: Platform,
}

impl ManagerRegistry
{
    /// Creates a new registry by discovering available managers.
    ///
    /// All managers are probed in parallel for maximum startup speed.
    pub async fn discover() -> Result<Self, Error>
    {
        let platform = Platform::current()?;
        let mut managers: HashMap<&'static str, Arc<dyn PackageManager>> =
            HashMap::new();

        let mut set: JoinSet<Option<(&'static str, Arc<dyn PackageManager>)>> =
            JoinSet::new();

        macro_rules! spawn_discover {
            ($manager_type:ty) => {
                set.spawn(async {
                    match <$manager_type>::new().await
                    {
                        Ok(mgr)
                            if mgr.is_available().await.unwrap_or(false) =>
                        {
                            Some((
                                mgr.id(),
                                Arc::new(mgr) as Arc<dyn PackageManager>,
                            ))
                        },
                        _ => None,
                    }
                });
            };
        }

        // Cross-platform
        spawn_discover!(npm::Npm);
        spawn_discover!(cargo::Cargo);
        spawn_discover!(gems::Gems);

        // Nix on Linux and macOS
        if matches!(platform, Platform::Linux | Platform::MacOS)
        {
            spawn_discover!(nix::Nix);
        }

        // macOS
        if platform == Platform::MacOS
        {
            spawn_discover!(brew::Brew);
            spawn_discover!(macports::MacPorts);
        }

        // Linux
        if platform == Platform::Linux
        {
            spawn_discover!(apk::Apk);
            spawn_discover!(apt::Apt);
            spawn_discover!(aptitude::Aptitude);
            spawn_discover!(dnf::Dnf);
            spawn_discover!(yum::Yum);
            spawn_discover!(zypper::Zypper);
            spawn_discover!(xbps::Xbps);
            spawn_discover!(pacman::Pacman);
            spawn_discover!(yay::Yay);
            spawn_discover!(paru::Paru);
            spawn_discover!(flatpak::Flatpak);
            spawn_discover!(snap::Snap);
            spawn_discover!(brew::Brew); // Linuxbrew
        }

        // Windows
        if platform == Platform::Windows
        {
            spawn_discover!(winget::Winget);
            spawn_discover!(scoop::Scoop);
            spawn_discover!(choco::Choco);
        }

        // FreeBSD and DragonFly BSD share the same `pkg` binary.
        if matches!(platform, Platform::FreeBSD | Platform::DragonFly)
        {
            spawn_discover!(freebsd_pkg::FreeBsdPkg);
        }

        // OpenBSD: classic pkg_add / pkg_delete / pkg_info toolset.
        if platform == Platform::OpenBSD
        {
            spawn_discover!(openbsd_pkg::OpenBsdPkg);
        }

        // NetBSD: pkgin.
        if platform == Platform::NetBSD
        {
            spawn_discover!(netbsd_pkg::Pkgin);
        }

        // Android: Termux `pkg` (apt wrapper). Distinct from FreeBSD's pkg.
        if platform == Platform::Android
        {
            spawn_discover!(termux_pkg::TermuxPkg);
        }

        while let Some(res) = set.join_next().await
        {
            if let Ok(Some((id, mgr))) = res
            {
                managers.insert(id, mgr);
            }
        }

        Ok(Self { managers, platform })
    }

    /// Returns a list of all available managers.
    pub fn available(&self) -> Vec<Arc<dyn PackageManager>>
    {
        self.managers.values().cloned().collect()
    }

    /// Returns a specific manager by ID.
    pub fn get(&self, id: &str) -> Result<Arc<dyn PackageManager>, Error>
    {
        self.managers
            .get(id)
            .cloned()
            .ok_or_else(|| Error::ManagerUnavailable(id.to_string()))
    }

    /// Returns the current platform.
    pub fn platform(&self) -> Platform
    {
        self.platform
    }

    /// Resolves an abstract package name across all available managers.
    ///
    /// All resolution requests are executed in parallel.
    pub async fn resolve_all(
        &self,
        abstract_name: &str,
        ctx: &ExecutionContext,
    ) -> HashMap<&'static str, Option<String>>
    {
        let mut set: JoinSet<(&'static str, Option<String>)> = JoinSet::new();

        for (id, manager) in &self.managers
        {
            if !manager.capabilities().resolve
            {
                continue;
            }
            let id = *id;
            let name = abstract_name.to_string();
            let ctx = ctx.clone();
            let mgr = manager.clone();
            set.spawn(async move {
                let resolved = mgr.resolve(&name, &ctx).await.ok().flatten();
                (id, resolved)
            });
        }

        let mut results = HashMap::new();
        while let Some(res) = set.join_next().await
        {
            if let Ok((id, resolved)) = res
            {
                results.insert(id, resolved);
            }
        }
        results
    }
}
