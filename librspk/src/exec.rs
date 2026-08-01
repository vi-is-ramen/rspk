use crate::Mgr;
use crate::err::{Error, Result};
use std::process::{Command, ExitStatus};

/// Get installation command for package manager.
/// 
/// # Arguments
/// 
/// - `mgr` ([`Mgr`]) - Package manager.
/// 
/// # Returns
/// 
/// - [`Result<Vec<String>>`] - List of CLI arguments.
/// 
/// # Errors
/// 
/// This function does not fail so far, but [`Result`]
/// type is used for future possible API changes.
/// 
/// # Examples
/// 
/// ```
/// # use crate::get_install_command;
/// 
/// let _ = get_install_command();
/// ```
pub fn get_install_command(mgr: Mgr) -> Result<Vec<String>>
{
    use Mgr::*;

    let base: Vec<&str> = match mgr
    {
        Apt => vec!["apt", "install", "-y"],
        Aptitude => vec!["aptitude", "install", "-y"],
        Pacman => vec!["pacman", "-S", "--noconfirm", "--needed"],
        Yay => vec!["yay", "-S", "--noconfirm", "--needed"],
        Paru => vec!["paru", "-S", "--noconfirm", "--needed"],
        Dnf => vec!["dnf", "install", "-y"],
        Yum => vec!["yum", "install", "-y"],
        Zypper => vec!["zypper", "install", "-y"],
        Apk => vec!["apk", "add"],
        Pkg => vec!["pkg", "install", "-y"],
        Winget => vec![
            "winget",
            "install",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ],
        Brew => vec!["brew", "install"],
        Rpm => vec!["rpm", "-i"],
    };

    Ok(base.into_iter().map(String::from).collect())
}

#[doc(hidden)]
pub fn is_root() -> bool
{
    #[cfg(unix)]
    {
        match std::process::Command::new("id").arg("-u").output()
        {
            Ok(out) if out.status.success() =>
            {
                String::from_utf8_lossy(&out.stdout).trim() == "0"
            },

            _ => false,
        }
    }

    #[cfg(not(unix))]
    {
        false
    }
}

/// Run the command. This function is safe to
/// invoke in both situations then you are root
/// or you are not root.
/// 
/// # Arguments
/// 
/// - `cmd` (`Vec<String>`) - CLI base.
/// - `args` (`Vec<String>`) - CLI arguments.
/// - `mgr` (`Mgr`) - Manager to be used.
/// 
/// # Returns
/// 
/// - `Result<ExitStatus>` - Exit status of the command.
/// 
/// # Errors
/// 
/// [`Error::Command`] if command invokation failed.
/// 
/// # Examples
/// 
/// ```
/// # use crate::{run_command, Apt};
/// 
/// let mgr = Mgr::Apt;
/// 
/// if mgr.is_available()
/// {
///     assert_eq(0, run_command
///     (
///         vec![mgr.bin_name()],
///         vec!["install", "libssl-dev", "-y"],
///         mgr
///     ).expect("Failed to spawn APT"),
///     "APT failed");
/// }
/// ```
pub fn run_command(
    cmd: Vec<String>,
    args: Vec<String>,
    mgr: Mgr,
) -> Result<ExitStatus>
{
    let mut full_cmd = Vec::new();

    if mgr.requires_sudo() && !is_root()
    {
        full_cmd.push("sudo".to_string());
    }

    full_cmd.extend(cmd);
    full_cmd.extend(args);

    if full_cmd.is_empty()
    {
        return Err(Error::Command("Empty command".to_string()));
    }

    eprintln!("▶️  Executing: {:?}", full_cmd);

    let status = Command::new(&full_cmd[0])
        .args(&full_cmd[1..])
        .status()
        .map_err(|e| Error::Command(e.to_string()))?;

    Ok(status)
}
