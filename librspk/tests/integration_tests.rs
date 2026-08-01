use rspk_core::{Mgr, detect_platform, resolve_package_name};

#[test]
fn test_detect_platform_does_not_panic()
{
    let platform = detect_platform();
    assert!(platform.is_ok());
    let p = platform.unwrap();
    println!("Detected OS: {:?}", p.os);
    println!("Detected Managers: {:?}", p.managers);
}

#[test]
fn test_resolve_package_name_fallback()
{
    // Should hit network/cache, fail to find mapping, and return original
    // string
    let abstract_name = "definitely-not-a-real-package-12345";
    let resolved = resolve_package_name(abstract_name, Mgr::Apt);
    assert_eq!(resolved, abstract_name);
}

#[test]
fn test_resolve_package_name_known()
{
    // Requires network access to Repology
    let resolved = resolve_package_name("git", Mgr::Apt);
    assert_eq!(resolved, "git");

    let resolved_arch = resolve_package_name("git", Mgr::Pacman);
    assert_eq!(resolved_arch, "git");
}
