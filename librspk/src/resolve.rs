use crate::Mgr;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn cache_dir() -> PathBuf
{
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "rspk")
    {
        return proj_dirs.cache_dir().to_path_buf();
    }
    if let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(".cache/rspk");
    }
    PathBuf::from(".cache/rspk")
}

fn get_cached(pkg: &str) -> Option<RepologyResponse>
{
    let path = cache_dir().join(format!("{}.json", pkg));
    if !path.exists()
    {
        return None;
    }
    let metadata = fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = SystemTime::now().duration_since(modified).ok()?;
    if elapsed > CACHE_TTL
    {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_cache(pkg: &str, data: &RepologyResponse) -> Result<(), String>
{
    let dir = cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", pkg));
    let json = serde_json::to_string(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn fetch_from_repology(pkg: &str) -> Result<RepologyResponse, String>
{
    let url = format!("https://repology.org/api/v1/project/{}", pkg);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("rspk/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&url).send().map_err(|e| e.to_string())?;

    if !resp.status().is_success()
    {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let entries: RepologyResponse = resp.json().map_err(|e| e.to_string())?;
    Ok(entries)
}

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Represent package manager entry.
/// 
/// # Fields
/// 
/// - `repo` (`String`) - Repository of the manager.
/// - `srcname` (`Option<String>`) - Source package name.
/// - `binname` (`Option<String>`) - Name of the contained binary.
/// 
/// # Examples
/// 
/// ```
/// use crate::RepoEntry;
/// 
/// let s = RepoEntry
/// {
///     repo: "debian_13",
///     srcname: Some("neovim"),
///     binname: Some("nvim"),
/// };
/// ```
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RepoEntry
{
    /// Repository of the manager.
    pub repo:    String,

    /// Source package name.
    pub srcname: Option<String>,

    /// Name of the contained binary.
    pub binname: Option<String>,
}

/// Represents responce from the Repology service.
pub type RepologyResponse = Vec<RepoEntry>;

/// Resolve package name for given package manager.
/// 
/// # Arguments
/// 
/// - `abstract_name` (`&str`) - Abstract name of the package.
/// - `mgr` (`Mgr`) - Package manager which package name to search for.
/// 
/// # Returns
/// 
/// - `String` - Package name.
/// 
/// # Examples
/// 
/// ```
/// # use crate::resolve_package_name;
/// 
/// let _ = resolve_package_name();
/// ```
pub fn resolve_package_name(abstract_name: &str, mgr: Mgr) -> String
{
    if let Some(cached) = get_cached(abstract_name)
    && let Some(real) = find_package_for_manager(&cached, mgr, abstract_name)
    {
        return real;
    }

    match fetch_from_repology(abstract_name)
    {
        Ok(entries) =>
        {
            let _ = save_cache(abstract_name, &entries);
            if let Some(real) =
                find_package_for_manager(&entries, mgr, abstract_name)
            {
                return real;
            }
            eprintln!(
                "⚠️  No mapping for '{}' with {} in Repology",
                abstract_name, mgr
            );
            abstract_name.to_string()
        },
        Err(e) =>
        {
            eprintln!(
                "⚠️  Failed to query Repology for '{}': {}",
                abstract_name, e
            );
            abstract_name.to_string()
        },
    }
}

#[doc(hidden)]
pub fn find_package_for_manager(
    entries: &RepologyResponse,
    mgr: Mgr,
    abstract_name: &str,
) -> Option<String>
{
    let is_match = |repo: &str| match mgr
    {
        Mgr::Apt | Mgr::Aptitude =>
        {
            repo.starts_with("debian")
                || repo.starts_with("ubuntu")
                || repo.starts_with("linuxmint")
                || repo.starts_with("kali")
        },
        Mgr::Pacman | Mgr::Yay | Mgr::Paru => repo == "arch" || repo == "aur",
        Mgr::Dnf =>
        {
            repo.starts_with("fedora")
                || repo.starts_with("centos")
                || repo.starts_with("rhel")
                || repo.starts_with("rocky")
                || repo.starts_with("almalinux")
        },
        Mgr::Yum =>
        {
            repo.starts_with("centos")
                || repo.starts_with("rhel")
                || repo.starts_with("rocky")
                || repo.starts_with("almalinux")
                || repo.starts_with("fedora")
        },
        Mgr::Zypper => repo.starts_with("opensuse") || repo.starts_with("suse"),
        Mgr::Apk => repo.starts_with("alpine"),
        Mgr::Pkg => repo == "freebsd",
        Mgr::Brew => repo == "homebrew" || repo == "macports",
        Mgr::Winget => repo == "winget",
        _ => false,
    };

    let mut best_match: Option<String> = None;
    let mut first_match: Option<String> = None;

    for entry in entries
    {
        if is_match(&entry.repo)
        {
            // Prefer the binary name, but fall back to source name if binary is
            // missing
            let name = entry.binname.as_deref().or(entry.srcname.as_deref());

            if let Some(n) = name
            {
                if first_match.is_none()
                {
                    first_match = Some(n.to_string());
                }

                // 1st Preference: Exact match with the abstract name we
                // searched for
                if n == abstract_name
                {
                    return Some(n.to_string()); // Short-circuit, can't get better than this
                }

                // 2nd Preference: binname matches srcname (usually the "main"
                // package)
                if best_match.is_none()
                    && entry.binname.as_deref() == entry.srcname.as_deref()
                {
                    best_match = Some(n.to_string());
                }
            }
        }
    }

    best_match.or(first_match)
}
