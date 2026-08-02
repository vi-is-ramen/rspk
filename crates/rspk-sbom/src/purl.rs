//! Package URL (PURL) generation.
//!
//! Implements the [PURL specification](https://github.com/package-url/purl-spec)
//! mapping each `pk` manager to its canonical PURL type and namespace.

use rspk_core::Package;
use std::fmt;

/// A parsed Package URL.
///
/// Format: `pkg:<type>/<namespace>/<name>@<version>?<qualifiers>#<subpath>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purl
{
    /// PURL type (e.g. `deb`, `rpm`, `cargo`, `npm`).
    pub ptype:     String,
    /// Optional namespace (e.g. `debian`, `fedora`, `@angular`).
    pub namespace: Option<String>,
    /// Package name.
    pub name:      String,
    /// Optional version.
    pub version:   Option<String>,
    /// Qualifiers (e.g. `arch=amd64`, `distro=alpine`).
    pub qualifiers: Vec<(String, String)>,
}

impl Purl
{
    /// Builds a PURL from a [`Package`] and its manager ID.
    ///
    /// The mapping follows the
    /// [known PURL types](https://github.com/package-url/purl-spec/blob/master/PURL-TYPES.rst).
    pub fn from_package(pkg: &Package, manager_id: &str) -> Self
    {
        let version = pkg
            .installed_version
            .as_ref()
            .map(ToString::to_string);

        let mut qualifiers = Vec::new();
        if let Some(arch) = &pkg.arch
        {
            qualifiers.push(("arch".to_string(), arch.clone()));
        }

        match manager_id
        {
            // ── Debian / Ubuntu ──
            "apt" | "aptitude" => Self {
                ptype:     "deb".into(),
                namespace: Some("debian".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Alpine ──
            "apk" => Self {
                ptype:     "apk".into(),
                namespace: Some("alpine".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Arch / AUR ──
            "pacman" | "yay" | "paru" => Self {
                ptype:     "pacman".into(),
                namespace: Some("arch".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Fedora / RHEL / CentOS ──
            "dnf" | "yum" => Self {
                ptype:     "rpm".into(),
                namespace: Some("fedora".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── openSUSE / SLE ──
            "zypper" => Self {
                ptype:     "rpm".into(),
                namespace: Some("opensuse".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Void Linux ──
            "xbps" => Self {
                ptype:     "xbps".into(),
                namespace: Some("void".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Homebrew / Linuxbrew ──
            "brew" => Self {
                ptype:     "brew".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── MacPorts ──
            "macports" => Self {
                ptype:     "macports".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Cargo / crates.io ──
            "cargo" => Self {
                ptype:     "cargo".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── npm ──
            "npm" =>
            {
                // Scoped packages: @angular/core → namespace=@angular
                let (ns, name) = if let Some(rest) =
                    pkg.id.strip_prefix('@')
                {
                    if let Some((scope, pkg_name)) = rest.split_once('/')
                    {
                        (
                            Some(format!("@{scope}")),
                            pkg_name.to_string(),
                        )
                    }
                    else
                    {
                        (None, pkg.id.clone())
                    }
                }
                else
                {
                    (None, pkg.id.clone())
                };
                Self {
                    ptype: "npm".into(),
                    namespace: ns,
                    name,
                    version,
                    qualifiers,
                }
            },

            // ── RubyGems ──
            "gems" => Self {
                ptype:     "gem".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Flatpak ──
            "flatpak" => Self {
                ptype:     "flatpak".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Snap ──
            "snap" => Self {
                ptype:     "snap".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Nix ──
            "nix" => Self {
                ptype:     "nix".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Windows: winget ──
            "winget" => Self {
                ptype:     "winget".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Windows: Chocolatey ──
            "choco" => Self {
                ptype:     "chocolatey".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Windows: Scoop ──
            "scoop" => Self {
                ptype:     "scoop".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── FreeBSD / DragonFly ──
            "freebsd-pkg" => Self {
                ptype:     "freebsd".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── OpenBSD ──
            "openbsd-pkg" => Self {
                ptype:     "openbsd".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── NetBSD / pkgsrc ──
            "pkgin" => Self {
                ptype:     "pkgsrc".into(),
                namespace: None,
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Termux (Android) ──
            "termux-pkg" => Self {
                ptype:     "deb".into(),
                namespace: Some("termux".into()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },

            // ── Fallback ──
            other => Self {
                ptype:     "generic".into(),
                namespace: Some(other.to_string()),
                name:      pkg.id.clone(),
                version,
                qualifiers,
            },
        }
    }
}

impl fmt::Display for Purl
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        write!(f, "pkg:{}", self.ptype)?;

        if let Some(ns) = &self.namespace
        {
            write!(f, "/{}", percent_encode(ns))?;
        }

        write!(f, "/{}", percent_encode(&self.name))?;

        if let Some(v) = &self.version
        {
            write!(f, "@{}", percent_encode(v))?;
        }

        if !self.qualifiers.is_empty()
        {
            write!(f, "?")?;
            for (i, (k, v)) in self.qualifiers.iter().enumerate()
            {
                if i > 0
                {
                    write!(f, "&")?;
                }
                write!(f, "{k}={}", percent_encode(v))?;
            }
        }

        Ok(())
    }
}

/// Minimal percent-encoding per PURL spec (RFC 3986 unreserved chars
/// are kept as-is).
fn percent_encode(s: &str) -> String
{
    let mut out = String::with_capacity(s.len());
    for b in s.bytes()
    {
        match b
        {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'/' => out.push(b as char),
            _ =>
            {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            },
        }
    }
    out
}

#[cfg(test)]
mod tests
{
    use super::*;
    use rspk_core::Package;

    fn pkg(id: &str, ver: &str, mgr: &str) -> Package
    {
        Package {
            id:                id.to_string(),
            name:              None,
            description:       None,
            installed_version: ver.parse().ok(),
            latest_version:    None,
            manager_id:        mgr.to_string(),
            arch:              None,
        }
    }

    #[test]
    fn test_apt_purl()
    {
        let p = Purl::from_package(&pkg("curl", "7.81.0", "apt"), "apt");
        assert_eq!(p.to_string(), "pkg:deb/debian/curl@7.81.0");
    }

    #[test]
    fn test_apk_purl_with_arch()
    {
        let mut p = pkg("curl", "8.4.0-r0", "apk");
        p.arch = Some("x86_64".to_string());
        let purl = Purl::from_package(&p, "apk");
        assert_eq!(
            purl.to_string(),
            "pkg:apk/alpine/curl@8.4.0-r0?arch=x86_64"
        );
    }

    #[test]
    fn test_cargo_purl()
    {
        let p = Purl::from_package(&pkg("ripgrep", "14.1.0", "cargo"), "cargo");
        assert_eq!(p.to_string(), "pkg:cargo/ripgrep@14.1.0");
    }

    #[test]
    fn test_npm_scoped_purl()
    {
        let p = Purl::from_package(
            &pkg("@angular/core", "16.0.0", "npm"),
            "npm",
        );
        assert_eq!(
            p.to_string(),
            "pkg:npm/%40angular/core@16.0.0"
        );
    }

    #[test]
    fn test_brew_purl()
    {
        let p = Purl::from_package(&pkg("wget", "1.21.4", "brew"), "brew");
        assert_eq!(p.to_string(), "pkg:brew/wget@1.21.4");
    }

    #[test]
    fn test_winget_purl()
    {
        let p = Purl::from_package(
            &pkg("Microsoft.VisualStudioCode", "1.85.1", "winget"),
            "winget",
        );
        assert_eq!(
            p.to_string(),
            "pkg:winget/Microsoft.VisualStudioCode@1.85.1"
        );
    }

    #[test]
    fn test_flatpak_purl()
    {
        let p = Purl::from_package(
            &pkg("org.mozilla.firefox", "120.0.0", "flatpak"),
            "flatpak",
        );
        assert_eq!(
            p.to_string(),
            "pkg:flatpak/org.mozilla.firefox@120.0.0"
        );
    }

    #[test]
    fn test_freebsd_purl()
    {
        let p = Purl::from_package(&pkg("curl", "7.81.0", "freebsd-pkg"), "freebsd-pkg");
        assert_eq!(p.to_string(), "pkg:freebsd/curl@7.81.0");
    }

    #[test]
    fn test_termux_purl()
    {
        let p = Purl::from_package(&pkg("curl", "7.81.0", "termux-pkg"), "termux-pkg");
        assert_eq!(p.to_string(), "pkg:deb/termux/curl@7.81.0");
    }

    #[test]
    fn test_unknown_manager_fallback()
    {
        let p = Purl::from_package(&pkg("foo", "1.0.0", "custom"), "custom");
        assert_eq!(p.to_string(), "pkg:generic/custom/foo@1.0.0");
    }
}
