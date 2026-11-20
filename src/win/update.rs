//! The Windows side of update (update.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// This system's entry in the manifest's release files.
pub(super) const PLATFORM: &str = "windows";
/// The download is a Windows program.
pub(super) fn is_program(data: &[u8]) -> Result<(), String> { if data.starts_with(b"MZ") { Ok(()) } else { Err("The download is not a Windows program.".into()) } }
pub(super) fn swap_in(app: &tauri::AppHandle, data: &[u8]) -> Result<(), String> {
    let current = std::env::current_exe().map_err(err)?;
    let fresh = current.with_file_name("Sora.exe.new");
    let mut old = current.clone().into_os_string();
    old.push(".old");
    let old = std::path::PathBuf::from(old);
    std::fs::write(&fresh, data).map_err(|e| format!("Cannot write to Sora's folder ({e}). Move Sora.exe somewhere you can edit, such as Documents."))?;
    let _ = std::fs::remove_file(&old);
    // A running .exe can be renamed on Windows; the new file takes its place and the old one is removed on next start.
    if let Err(e) = std::fs::rename(&current, &old) { let _ = std::fs::remove_file(&fresh); return Err(format!("Could not replace Sora.exe ({e}).")); }
    if let Err(e) = std::fs::rename(&fresh, &current) { let _ = std::fs::rename(&old, &current); return Err(format!("Could not install the update ({e}).")); }
    crate::focus::stop_owned(&app.state::<crate::focus::FocusState>());
    std::process::Command::new(&current).arg("--updated").spawn().map_err(|e| format!("Updated, but Sora could not restart ({e}). Open it again manually."))?;
    app.exit(0);
    Ok(())
}
