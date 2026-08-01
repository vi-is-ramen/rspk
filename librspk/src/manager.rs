use crate::detect::{Mgr, detect_platform};
use crate::err::{Error, Result};

/// Select available package manager in the system.
/// 
/// # Arguments
/// 
/// - `preferred` (`Option<Mgr>`) - Preffered package manager (optional).
/// 
/// # Returns
/// 
/// - `Result<Mgr>` - Selected manager.
/// 
/// # Errors
/// 
/// [`Error::ManagerNotAvailable`] if preferred manager is defined and is not available.
/// [`Error::NoManagerFound`] if system has not any managers.
/// 
/// # Examples
/// 
/// ```
/// use crate::select_manager;
/// 
/// println!("Package manager: {}", select_manager());
/// ```
pub fn select_manager(preferred: Option<Mgr>) -> Result<Mgr>
{
    let platform = detect_platform()?;
    if platform.managers.is_empty()
    {
        return Err(Error::NoManagerFound);
    }
    if let Some(pref) = preferred
    {
        if platform.managers.contains(&pref)
        {
            return Ok(pref);
        }
        else
        {
            return Err(Error::ManagerNotAvailable(pref.bin_name().to_string()));
        }
    }
    let mut sorted = platform.managers;
    sorted.sort_by_key(priority);
    Ok(sorted[0])
}

#[doc(hidden)]
pub fn priority(mgr: &Mgr) -> u8
{
    use Mgr::*;
    match mgr
    {
        Brew => 5,
        Yay => 10,
        Paru => 11,
        Pacman => 20,
        Aptitude => 30,
        Apt => 31,
        Dnf => 40,
        Yum => 41,
        Zypper => 50,
        Apk => 60,
        Pkg => 70,
        Winget => 80,
        Rpm => 90,
    }
}
