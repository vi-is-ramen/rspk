use crate::err::{Error, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents Linux-based OS family.
/// 
/// # Variants
/// 
/// - `Arch` - Arch Linux.
/// - `Debian` - Debian and derivatives.
/// - `RedHat` - RHEL, Fedora and compatible.
/// - `Suse` - SUSE, OpenSUSE and compatible.
/// - `Android` - AOSP, Google Android, Termux env et al.
/// - `Unknown` - Any other (unsupported).
/// 
/// # Examples
/// 
/// ```
/// # use crate::LinuxFamily;
/// 
/// let linux_family = LinuxFamily::Arch;
/// 
/// match linux_family
/// {
///     LinuxFamily::Arch => eprintln!("Arch Linux!"),
///     LinuxFamily::Debian => eprintln!("Debian!"),
///     LinuxFamily::RedHat => eprintln!("RHEL/Fedora!"),
///     LinuxFamily::Suse => eprintln!("(Open)SUSE!"),
///     LinuxFamily::Android => eprintln!("A(OSP|ndroid)!"),
///     LinuxFamily::Unknown => eprintln!("IDK what is that :P"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxFamily
{
    /// Arch Linux
    Arch,

    /// Debian and derivatives
    Debian,

    /// RHEL, Fedora and compatible
    RedHat,

    /// SUSE, OpenSUSE and compatible
    Suse,

    /// AOSP, Google Android, Termux et al.
    Android,

    /// Unsupported.
    Unknown,
}

/// Represents operating system type.
/// 
/// # Variants
/// 
/// - `Linux(LinuxFamily)` - Linux-based OS.
/// - `Windows` - Microsoft Windows (DOS-based, NT-based).
/// - `MacOs` - Mach/XNU (Darwin) OSes.
/// - `Bsd` - BSD forks (FreeBSD, NetBSD et al.).
/// - `Unknown` - Unsupported OS.
/// 
/// # Examples
/// 
/// ```
/// # use crate::Os;
/// 
/// let os = Os::Linux;
/// 
/// match os
/// {
///     Os::Linux(v0) => eprintln!("Linux!"),
///     Os::Windows => eprintln!("MS Windows!"),
///     Os::MacOs => eprintln!("macOS!"),
///     Os::Bsd => eprintln!("BSD!"),
///     Os::Unknown => eprintln!("IDK what is that :P"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os
{
    /// Linux-based OS.
    Linux(LinuxFamily),

    /// Microsoft Windows (DOS-based, NT-based).
    Windows,

    /// Mach/XNU (Darwin) OSes.
    MacOs,

    /// BSD forks (FreeBSD, NetBSD et al.).
    Bsd,

    /// Unsupported OS.
    Unknown,
}

/// Represents package manager.
/// 
/// # Variants
/// 
/// - `Pacman` - Pacman (originally for Arch Linux).
/// - `Yum` - Yum (RHEL, et al.).
/// - `Yay` - Yay helper (Arch Linux).
/// - `Paru` - Paru helper (Arch Linux).
/// - `Apt` - APT (Debian and deriv.).
/// - `Aptitude` - Aptitude (Debian deriv.).
/// - `Apk` - Alpine Linux and compatible.
/// - `Pkg` - FreeBSD's `pkg`, Termux' `pkg`.
/// - `Rpm` - RPM (ALT Linux, et al.) and compatible.
/// - `Dnf` - DNF (RHEL, et al.).
/// - `Zypper` - Zypper (originally for SUSE).
/// - `Winget` - Windows Getter (for Windows NT 6.1+).
/// - `Brew` - Brew for macOS.
/// 
/// # Examples
/// 
/// ```
/// use crate::Mgr;
/// 
/// let mgr = Mgr::Pacman;
/// 
/// match mgr
/// {
///     Mgr::Pacman => eprintln!("Pacman!"),
///     Mgr::Yum => eprintln!("Yum!"),
///     Mgr::Yay => eprintln!("Yay helper!"),
///     Mgr::Paru => eprintln!("Paru helper!"),
///     Mgr::Apt => eprintln!("APT!"),
///     Mgr::Aptitude => eprintln!("Aptitude!"),
///     Mgr::Apk => eprintln!("APK!"),
///     Mgr::Pkg => eprintln!("pkg!"),
///     Mgr::Rpm => eprintln!("RPM!"),
///     Mgr::Dnf => eprintln!("DNF!"),
///     Mgr::Zypper => eprintln!("Zypper!"),
///     Mgr::Winget => eprintln!("Windows Getter!"),
///     Mgr::Brew => eprintln!("IDK what is that :P"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mgr
{
    /// Pacman
    Pacman,

    /// Yum
    Yum,

    /// Yay AUR helper
    Yay,

    /// Paru AUR helper
    Paru,

    /// APT
    Apt,

    /// Aptitude
    Aptitude,

    /// APK
    Apk,

    /// `pkg`
    Pkg,

    /// RPM
    Rpm,

    /// DNF
    Dnf,

    /// Zypper
    Zypper,

    /// Windows Getter
    Winget,

    /// Brew
    Brew,
}

impl Mgr
{
    /// Get canonical binary name of manager.
    /// 
    /// # Returns
    /// 
    /// - `&'static str` - String representation of the binary name.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use crate::Mgr;
    /// 
    /// println!("{}", Mgr::Yay.bin_name());
    /// ```
    pub const fn bin_name(&self) -> &'static str
    {
        match self
        {
            Mgr::Pacman => "pacman",
            Mgr::Yum => "yum",
            Mgr::Yay => "yay",
            Mgr::Paru => "paru",
            Mgr::Apt => "apt",
            Mgr::Aptitude => "aptitude",
            Mgr::Apk => "apk",
            Mgr::Pkg => "pkg",
            Mgr::Rpm => "rpm",
            Mgr::Dnf => "dnf",
            Mgr::Zypper => "zypper",
            Mgr::Winget => "winget",
            Mgr::Brew => "brew",
        }
    }

    /// Check if package manager is available.
    /// 
    /// # Returns
    /// 
    /// - `bool` - `true` if manager is available.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use crate::Mgr;
    /// 
    /// if Mgr::Apt.is_available()
    /// {
    ///     println!("I see your APT! :D");
    /// }
    /// ```
    pub fn is_available(&self) -> bool
    {
        which::which(self.bin_name()).is_ok()
    }

    /// Check if package manager invocation
    /// requires substitution (`su`/`sudo`/`runas`/UAC prompt).
    /// 
    /// # Returns
    /// 
    /// - `bool` - `true` if substitution is required.
    /// 
    /// # Examples
    /// 
    /// ```
    /// # use crate::Mgr;
    /// 
    /// if Mgr::Apk.requires_sudo()
    /// {
    ///     println!("I will be your root ;)");
    /// }
    /// ```
    pub const fn requires_sudo(&self) -> bool
    {
        matches!
        (
            self
            ,   Mgr::Apt
            |   Mgr::Aptitude
            |   Mgr::Pacman
            |   Mgr::Dnf
            |   Mgr::Yum
            |   Mgr::Zypper
            |   Mgr::Apk
            |   Mgr::Pkg
            |   Mgr::Rpm
        )
    }
}

impl std::str::FromStr for Mgr
{
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err>
    {
        match s.to_lowercase().as_str()
        {
            "pacman" => Ok(Mgr::Pacman),
            "yum" => Ok(Mgr::Yum),
            "yay" => Ok(Mgr::Yay),
            "paru" => Ok(Mgr::Paru),
            "apt" => Ok(Mgr::Apt),
            "aptitude" => Ok(Mgr::Aptitude),
            "apk" => Ok(Mgr::Apk),
            "pkg" => Ok(Mgr::Pkg),
            "rpm" => Ok(Mgr::Rpm),
            "dnf" => Ok(Mgr::Dnf),
            "zypper" => Ok(Mgr::Zypper),
            "winget" => Ok(Mgr::Winget),
            "brew" => Ok(Mgr::Brew),
            _ => Err(format!("Unknown manager: {}", s)),
        }
    }
}

impl std::fmt::Display for Mgr
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}", self.bin_name())
    }
}

/// Represents platform configuration in distribution context.
/// 
/// # Fields
/// 
/// - `os` ([`Os`]) - OS type and family (if applicable).
/// - `managers` ([`Vec<Mgr>`]) - List of available package managers.
/// 
/// # Examples
/// 
/// ```
/// # use crate::Platform;
/// 
/// let s = Platform
/// {
///     os: value,
///     managers: value,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Platform
{
    /// OS type and family (if applicable).
    pub os:       Os,

    /// List of available package managers.
    pub managers: Vec<Mgr>,
}

/// Detect platform.
/// 
/// # Returns
/// 
/// - [`Result<Platform>`] - [`Platform`] or [`Err`].
/// 
/// # Errors
/// 
/// This function does not fail so far, but [`Result`]
/// type is used for future possible API changes.
/// 
/// # Examples
/// 
/// ```
/// # use crate::detect_platform;
/// 
/// if let Ok(plt) = detect_platform()
/// {
///     println!("{}", plt)
/// }
/// else
/// {
///     println!("IDK this platform LMAO :P");
/// }
/// ```
pub fn detect_platform() -> Result<Platform>
{
    match std::env::consts::OS
    {
        "linux" => detect_linux(),
        "windows" => detect_windows(),
        "macos" => detect_macos(),
        _ => detect_by_managers(),
    }
}

fn detect_linux() -> Result<Platform>
{
    let content = read_os_release()?;
    let data = parse_os_release(&content);
    let family = determine_linux_family(&data);
    let managers = find_available_managers();
    Ok(Platform {
        os: Os::Linux(family),
        managers,
    })
}

fn read_os_release() -> Result<String>
{
    let paths = ["/etc/os-release", "/usr/lib/os-release"];
    for path in paths
    {
        if Path::new(path).exists()
        {
            return fs::read_to_string(path).map_err(Error::OsReleaseRead);
        }
    }
    Err(Error::OsReleaseParse)
}

#[doc(hidden)]
pub fn parse_os_release(content: &str) -> HashMap<String, String>
{
    let mut map = HashMap::new();
    for line in content.lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#')
        {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
        {
            let key = key.trim().to_string();
            let value = value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string();
            map.insert(key, value);
        }
    }
    map
}

#[doc(hidden)]
pub fn determine_linux_family(data: &HashMap<String, String>) -> LinuxFamily
{
    if let Some(id) = data.get("ID")
    {
        match id.as_str()
        {
            "arch" => return LinuxFamily::Arch,
            "debian" | "ubuntu" | "linuxmint" | "kali" =>
            {
                return LinuxFamily::Debian
            },
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" =>
            {
                return LinuxFamily::RedHat
            },
            "suse" | "opensuse" => return LinuxFamily::Suse,
            "android" => return LinuxFamily::Android,
            _ =>
            {},
        }
    }
    if let Some(like) = data.get("ID_LIKE")
    {
        for part in like.split_whitespace()
        {
            match part
            {
                "arch" => return LinuxFamily::Arch,
                "debian" | "ubuntu" => return LinuxFamily::Debian,
                "rhel" | "fedora" | "centos" => return LinuxFamily::RedHat,
                "suse" | "opensuse" => return LinuxFamily::Suse,
                "android" => return LinuxFamily::Android,
                _ =>
                {},
            }
        }
    }
    LinuxFamily::Unknown
}

fn detect_windows() -> Result<Platform>
{
    let managers = if Mgr::Winget.is_available()
    {
        vec![Mgr::Winget]
    }
    else
    {
        vec![]
    };
    Ok(Platform {
        os: Os::Windows,
        managers,
    })
}

fn detect_macos() -> Result<Platform>
{
    let managers = find_available_managers();
    Ok(Platform {
        os: Os::MacOs,
        managers,
    })
}

fn detect_by_managers() -> Result<Platform>
{
    let managers = find_available_managers();
    Ok(Platform {
        os: Os::Unknown,
        managers,
    })
}

fn find_available_managers() -> Vec<Mgr>
{
    use Mgr::*;
    let all = [
        Brew, Pacman, Yum, Yay, Paru, Apt, Aptitude, Apk, Pkg, Rpm, Dnf,
        Zypper, Winget,
    ];
    all.into_iter().filter(|m| m.is_available()).collect()
}
