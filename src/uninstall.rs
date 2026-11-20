//! "Uninstall Sora" in Settings, for the copies no installer manages: the portable Sora.exe and Sora.app on a Mac. It
//! leaves nothing behind, so a new download starts from the first-run setup: the coding agents' hooks that call Sora,
//! "Start with Windows" / "Open at login", every folder of Sora's data, cache and web data, and the program itself.
//! The .msi does the same on its own uninstall (installer/Sora.wxs); the Store removes a package's data itself.
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// How this copy is removed: "store" and "installer" through Windows' Apps settings, "self" with the button.
fn kind() -> &'static str {
    if crate::STORE { return "store"; }
    #[cfg(win)]
    if let (Some(local), Ok(exe)) = (std::env::var_os("LOCALAPPDATA"), std::env::current_exe()) {
        if exe.starts_with(PathBuf::from(local).join("Programs").join("Sora")) { return "installer"; }
    }
    "self"
}

#[tauri::command]
pub fn uninstall_kind() -> &'static str { kind() }

/// The program to delete: Sora.exe, or the Sora.app bundle it runs in (not one macOS runs from a disk image or a
/// translocated copy: that is not where it is installed).
fn program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if cfg!(win) { return Some(exe); }
    let app = exe.ancestors().nth(3)?.to_path_buf();
    let s = app.to_string_lossy();
    (s.ends_with(".app") && !s.contains("/AppTranslocation/") && !s.starts_with("/Volumes/")).then_some(app)
}

/// Every folder of Sora's own (pure apart from the paths it is given; tested): Tauri's data, cache, config and log
/// folders, and on a Mac the web data, saved state, preferences and the notch log.
fn data_dirs(tauri_dirs: Vec<PathBuf>, home: Option<&Path>, id: &str) -> Vec<PathBuf> {
    let mut v = tauri_dirs;
    if !cfg!(win) {
        if let Some(h) = home {
            let lib = h.join("Library");
            v.extend([lib.join("WebKit").join(id), lib.join("HTTPStorages").join(id), lib.join("Caches").join(id), lib.join("Logs").join("Sora"),
                lib.join("Saved Application State").join(format!("{id}.savedState")), lib.join("Preferences").join(format!("{id}.plist"))]);
        }
    }
    // Only folders named after Sora: never a parent such as ~/Library or %APPDATA% itself.
    v.retain(|p| p.to_string_lossy().contains(id) || p.ends_with("Sora"));
    v.sort();
    v.dedup();
    v
}

#[tauri::command]
pub fn uninstall_sora(app: AppHandle) -> Result<(), String> {
    if kind() != "self" { return Err("Uninstall Sora from Settings > Apps.".into()); }
    // The coding agents' hooks that call this Sora.
    if let Ok(serde_json::Value::Object(hooks)) = serde_json::to_value(crate::agents::agent_hooks()) {
        for (agent, on) in hooks { if on == true { let _ = crate::agents::agent_hooks_set(agent.replace('_', "-"), false); } }
    }
    crate::startup::stop_for_uninstall(&app);
    let p = app.path();
    let tauri_dirs = [p.app_data_dir(), p.app_local_data_dir(), p.app_cache_dir(), p.app_config_dir(), p.app_log_dir()].into_iter().flatten().collect();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut gone = data_dirs(tauri_dirs, home.as_deref(), &app.config().identifier);
    gone.extend(program());
    crate::platform::remove_after_exit(&gone)?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_only_soras_own_folders() {
        let home = Path::new("/Users/me");
        let d = data_dirs(vec![PathBuf::from("/Users/me/Library/Application Support/com.nook.desktop"), PathBuf::from("/Users/me/Library")], Some(home), "com.nook.desktop");
        assert!(d.iter().all(|p| p.to_string_lossy().contains("com.nook.desktop") || p.ends_with("Sora")), "{d:?}");
        assert!(!d.contains(&PathBuf::from("/Users/me/Library")));
        assert!(d.contains(&PathBuf::from("/Users/me/Library/Application Support/com.nook.desktop")));
    }
}
