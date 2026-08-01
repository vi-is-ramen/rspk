use rspk_core::{Mgr, get_install_command, priority};
use std::str::FromStr;

#[test]
fn test_mgr_from_str()
{
    assert_eq!(Mgr::from_str("apt").unwrap(), Mgr::Apt);
    assert_eq!(Mgr::from_str("PACMAN").unwrap(), Mgr::Pacman); // Case insensitive
    assert_eq!(Mgr::from_str("brew").unwrap(), Mgr::Brew);
    assert!(Mgr::from_str("unknown_mgr").is_err());
}

#[test]
fn test_mgr_requires_sudo()
{
    assert!(Mgr::Apt.requires_sudo());
    assert!(Mgr::Pacman.requires_sudo());
    assert!(!Mgr::Yay.requires_sudo()); // AUR helpers don't need root
    assert!(!Mgr::Paru.requires_sudo());
    assert!(!Mgr::Brew.requires_sudo());
    assert!(!Mgr::Winget.requires_sudo());
}

#[test]
fn test_get_install_command_flags()
{
    let apt_cmd = get_install_command(Mgr::Apt).unwrap();
    assert_eq!(apt_cmd, vec!["apt", "install", "-y"]);

    let pacman_cmd = get_install_command(Mgr::Pacman).unwrap();
    assert_eq!(pacman_cmd, vec!["pacman", "-S", "--noconfirm", "--needed"]);

    let winget_cmd = get_install_command(Mgr::Winget).unwrap();
    assert!(winget_cmd.contains(&"--accept-package-agreements".to_string()));

    let brew_cmd = get_install_command(Mgr::Brew).unwrap();
    assert_eq!(brew_cmd, vec!["brew", "install"]);
}

#[test]
fn test_manager_priority()
{
    // Brew and AUR helpers should generally be preferred over system managers
    // if available
    assert!(priority(&Mgr::Brew) < priority(&Mgr::Pacman));
    assert!(priority(&Mgr::Pacman) < priority(&Mgr::Apt));
    assert!(priority(&Mgr::Yay) < priority(&Mgr::Pacman));
}
