//! startup on macOS (main.rs swaps this file in when `cfg(win)` is off): "Open at login" is a LaunchAgent in
//! ~/Library/LaunchAgents that opens Sora at login. Same functions as startup.rs. It opens Sora by its bundle id (open -b),
//! not by the path it runs from now: opened from its disk image or from Downloads (which macOS runs from a random,
//! translocated folder) that path is gone after a restart; the bundle id finds Sora in Applications. macOS lists it under
//! System Settings > General > Login Items as Sora (AssociatedBundleIdentifiers).
use std::path::PathBuf;

const LABEL: &str = "com.nook.desktop";
fn agent() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("Home folder unavailable")?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents").join(format!("{LABEL}.plist")))
}
/// The plist that opens the app `bundle` (its bundle id) at login (pure; tested).
fn plist(bundle: &str) -> String {
    let bundle = bundle.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>{LABEL}</string>\n\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>/usr/bin/open</string>\n\t\t<string>-b</string>\n\t\t<string>{bundle}</string>\n\t</array>\n\t<key>AssociatedBundleIdentifiers</key>\n\t<string>{bundle}</string>\n\t<key>RunAtLoad</key>\n\t<true/>\n</dict>\n</plist>\n")
}
#[tauri::command]
pub fn startup_enabled() -> Result<bool, String> { Ok(agent()?.is_file()) }
#[tauri::command]
pub fn set_startup(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let file = agent()?;
    if enabled {
        if let Some(dir) = file.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
        std::fs::write(&file, plist(&app.config().identifier)).map_err(|_| "macOS could not save the startup preference.".to_string())?;
    } else if file.is_file() {
        std::fs::remove_file(&file).map_err(|_| "macOS could not update the startup preference.".to_string())?;
    }
    startup_enabled()
}
pub fn migrate_legacy() -> Result<(), String> { Ok(()) }
pub fn adopt_installed() {}
/// Uninstall (uninstall.rs): "Open at login" goes.
pub fn stop_for_uninstall(app: &tauri::AppHandle) { let _ = set_startup(app.clone(), false); }

#[cfg(test)]
mod tests {
    #[test]
    fn opens_sora_by_its_bundle_id() {
        let p = super::plist("com.nook.desktop");
        assert!(p.contains("<string>/usr/bin/open</string>\n\t\t<string>-b</string>\n\t\t<string>com.nook.desktop</string>"));
        assert!(p.contains("<key>AssociatedBundleIdentifiers</key>\n\t<string>com.nook.desktop</string>") && p.contains("<true/>"));
        assert!(super::plist("a&b").contains("a&amp;b"), "escaped");
    }
}
