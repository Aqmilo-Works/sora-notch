//! The Windows side of steam (steam.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Steam's folder: the registry value Steam writes, else the usual place.
pub(super) fn steam_dir() -> Option<PathBuf> {
    use windows::{core::w, Win32::{Foundation::ERROR_SUCCESS, System::Registry::*}};
    let mut buf = [0u16; 520];
    let mut size = (buf.len() * 2) as u32;
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Valve\\Steam"), w!("SteamPath"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) } == ERROR_SUCCESS;
    let from_registry = ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(0)]))).filter(|p| p.join("steamapps").is_dir());
    from_registry.or_else(|| ["C:\\Program Files (x86)\\Steam", "C:\\Program Files\\Steam"].iter().map(PathBuf::from).find(|p| p.join("steamapps").is_dir()))
}
/// The game Steam is running now (0 for none), as Steam writes it to the registry.
#[tauri::command]
pub fn steam_running() -> u32 {
    use windows::{core::w, Win32::{Foundation::ERROR_SUCCESS, System::Registry::*}};
    let (mut v, mut size) = (0u32, 4u32);
    let ok = unsafe { RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Valve\\Steam"), w!("RunningAppID"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size)) } == ERROR_SUCCESS;
    if ok { v } else { 0 }
}
