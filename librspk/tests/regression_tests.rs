use rspk_core::{
    LinuxFamily, Mgr, RepoEntry, determine_linux_family,
    find_package_for_manager, parse_os_release,
};
use std::collections::HashMap;

#[test]
fn test_parse_os_release_basic()
{
    let content = r#"
NAME="Ubuntu"
VERSION="20.04.4 LTS (Focal Fossa)"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Ubuntu 20.04.4 LTS"
"#;
    let parsed = parse_os_release(content);
    assert_eq!(parsed.get("ID").unwrap(), "ubuntu");
    assert_eq!(parsed.get("ID_LIKE").unwrap(), "debian");
}

#[test]
fn test_parse_os_release_quotes()
{
    let content = r#"
ID="arch"
NAME='Arch Linux'
"#;
    let parsed = parse_os_release(content);
    assert_eq!(parsed.get("ID").unwrap(), "arch");
    assert_eq!(parsed.get("NAME").unwrap(), "Arch Linux");
}

#[test]
fn test_determine_linux_family()
{
    let mut data = HashMap::new();
    data.insert("ID".to_string(), "ubuntu".to_string());
    assert_eq!(determine_linux_family(&data), LinuxFamily::Debian);

    data.clear();
    data.insert("ID".to_string(), "manjaro".to_string());
    data.insert("ID_LIKE".to_string(), "arch".to_string());
    assert_eq!(determine_linux_family(&data), LinuxFamily::Arch);

    data.clear();
    data.insert("ID".to_string(), "unknown".to_string());
    assert_eq!(determine_linux_family(&data), LinuxFamily::Unknown);
}

#[test]
fn test_find_package_for_manager_repology()
{
    let entries = vec![
        RepoEntry {
            repo:    "ubuntu_22_04".to_string(),
            srcname: Some("libssl-dev".to_string()),
            binname: Some("libssl-dev".to_string()),
        },
        RepoEntry {
            repo:    "arch".to_string(),
            srcname: Some("openssl".to_string()),
            binname: Some("openssl".to_string()),
        },
        RepoEntry {
            repo:    "fedora_38".to_string(),
            srcname: Some("openssl-devel".to_string()),
            binname: Some("openssl-devel".to_string()),
        },
    ];

    // Pass the abstract name we "searched" for
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Apt, "libssl-dev"),
        Some("libssl-dev".to_string())
    );
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Pacman, "openssl"),
        Some("openssl".to_string())
    );

    // Searching for "openssl" on Fedora should gracefully fallback to the
    // Fedora equivalent "openssl-devel"
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Dnf, "openssl"),
        Some("openssl-devel".to_string())
    );
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Brew, "openssl"),
        None
    );
}

#[test]
fn test_find_package_fallback_to_srcname()
{
    let entries = vec![RepoEntry {
        repo:    "arch".to_string(),
        srcname: Some("python-requests".to_string()),
        binname: None,
    }];
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Pacman, "python-requests"),
        Some("python-requests".to_string())
    );
}

#[test]
fn test_regression_git_subpackages()
{
    // Regression test for the bug where Repology returns multiple binary
    // packages for a single source project (e.g. git, git-man,
    // git-zsh-completion).
    let entries = vec![
        RepoEntry {
            repo:    "debian_12".to_string(),
            srcname: Some("git".to_string()),
            binname: Some("git-zsh-completion".to_string()),
        },
        RepoEntry {
            repo:    "debian_12".to_string(),
            srcname: Some("git".to_string()),
            binname: Some("git-man".to_string()),
        },
        RepoEntry {
            repo:    "debian_12".to_string(),
            srcname: Some("git".to_string()),
            binname: Some("git".to_string()),
        },
    ];

    // Even if subpackages appear first in the API response, it must select the
    // exact match "git"
    assert_eq!(
        find_package_for_manager(&entries, Mgr::Apt, "git"),
        Some("git".to_string())
    );
}
