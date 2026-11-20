//! The macOS side of update (update.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// This system's entry in the manifest's release files: the release's .dmg (one for both CPUs), the same file people
/// download, so the release holds nothing else for macOS.
pub(super) const PLATFORM: &str = "macos";
/// The download is a disk image (UDIF: a "koly" block in its last 512 bytes). What is inside is checked once mounted.
pub(super) fn is_program(data: &[u8]) -> Result<(), String> {
    let ok = data.len() > 512 && &data[data.len() - 512..data.len() - 508] == b"koly";
    if ok { Ok(()) } else { Err("The download is not Sora for macOS.".into()) }
}
/// Mounts the .dmg (hidden, read-only), copies its Sora.app beside the running one (ditto keeps permissions, symlinks and
/// the signature), swaps the two bundles (the old one is removed on next start, platform::remove_old_update) and opens
/// the new one.
pub(super) fn swap_in(app: &tauri::AppHandle, data: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(err)?;
    let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).ok_or("Sora is not running from Sora.app, so it cannot update itself.")?.to_path_buf();
    let parent = bundle.parent().ok_or("No folder around Sora.app.")?;
    let work = parent.join(".sora-update");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("Cannot write beside Sora.app ({e}). Move it to Applications."))?;
    let fail = |msg: String| { let _ = std::fs::remove_dir_all(&work); Err(msg) };
    let dmg = work.join("update.dmg");
    if let Err(e) = std::fs::write(&dmg, data) { return fail(e.to_string()); }
    let mount = work.join("mnt");
    let attached = std::process::Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"]).arg(&mount).arg(&dmg)
        .stdout(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
    if !attached { return fail("The update could not be opened.".into()); }
    let fresh = work.join("Sora.app");
    let copied = mount.join("Sora.app").join("Contents").join("MacOS").is_dir()
        && std::process::Command::new("ditto").arg(mount.join("Sora.app")).arg(&fresh).status().is_ok_and(|s| s.success());
    let _ = std::process::Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    if !copied || !fresh.is_dir() { return fail("The update does not hold Sora.app.".into()); }
    let old = bundle.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&old);
    if let Err(e) = std::fs::rename(&bundle, &old) { return fail(format!("Could not replace Sora.app ({e}).")); }
    if let Err(e) = std::fs::rename(&fresh, &bundle) { let _ = std::fs::rename(&old, &bundle); return fail(format!("Could not install the update ({e}).")); }
    let _ = std::fs::remove_dir_all(&work);
    crate::focus::stop_owned(&app.state::<crate::focus::FocusState>());
    std::process::Command::new("open").arg("-n").arg(&bundle).args(["--args", "--updated"]).spawn().map_err(|e| format!("Updated, but Sora could not restart ({e}). Open it again manually."))?;
    app.exit(0);
    Ok(())
}
