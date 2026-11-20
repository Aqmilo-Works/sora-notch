use std::os::windows::process::CommandExt;
use std::process::{Command, Output};
const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
fn registry(args: &[&str]) -> Result<Output, String> {
    let system = std::env::var_os("SystemRoot").ok_or("Windows directory unavailable")?;
    Command::new(std::path::PathBuf::from(system).join("System32").join("reg.exe"))
        .args(args).creation_flags(0x08000000).output().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn startup_enabled() -> Result<bool, String> {
    let result = registry(&["query", RUN, "/v", "Sora"])?;
    Ok(result.status.success())
}
#[tauri::command]
pub fn set_startup(enabled: bool) -> Result<bool, String> {
    if !enabled && !startup_enabled()? { return Ok(false); }
    let result = if enabled {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\"", executable.to_str().ok_or("Unsupported executable path")?);
        registry(&["add", RUN, "/v", "Sora", "/t", "REG_SZ", "/d", &command, "/f"])?
    } else {
        registry(&["delete", RUN, "/v", "Sora", "/f"])?
    };
    if !result.status.success() { return Err("Windows could not update the startup preference.".into()); }
    let actual = startup_enabled()?;
    if actual != enabled { return Err("Windows did not save the startup preference.".into()); }
    Ok(actual)
}

// Preserve an existing opt-in when upgrading the former Nook executable.
pub fn migrate_legacy() -> Result<(),String> {
 if registry(&["query", RUN, "/v", "Nook"])?.status.success() {
  if !startup_enabled()? {set_startup(true)?;}
  let result=registry(&["delete", RUN, "/v", "Nook", "/f"])?;
  if !result.status.success(){return Err("Could not remove the old startup entry.".into());}
 }
 Ok(())
}

/// Uninstall (uninstall.rs): "Start with Windows" goes when it starts this copy (another copy's entry stays).
pub fn stop_for_uninstall(_app: &tauri::AppHandle) {
    let Ok(exe) = std::env::current_exe() else { return };
    let Ok(out) = registry(&["query", RUN, "/v", "Sora"]) else { return };
    if out.status.success() && String::from_utf8_lossy(&out.stdout).to_lowercase().contains(&exe.to_string_lossy().to_lowercase()) { let _ = set_startup(false); }
}

/// A Sora installed by the .msi (%LOCALAPPDATA%\Programs\Sora) takes over "Start with Windows" from a portable Sora.exe that had it.
pub fn adopt_installed() {
    let (Some(local), Ok(exe)) = (std::env::var_os("LOCALAPPDATA"), std::env::current_exe()) else { return };
    if !exe.starts_with(std::path::PathBuf::from(local).join("Programs").join("Sora")) { return; }
    let Ok(out) = registry(&["query", RUN, "/v", "Sora"]) else { return };
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    if out.status.success() && !text.contains(&exe.to_string_lossy().to_lowercase()) { let _ = set_startup(true); }
}
