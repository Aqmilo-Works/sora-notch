//! The macOS backend of `platform`, same functions as win/platform.rs.
//! The notch is the web notch (index.html) in a borderless, transparent window that sits just below the menu bar, which on a
//! MacBook with a camera notch is exactly as tall as the camera housing, so nothing of Sora is ever drawn behind the camera.
use crate::{err, AppResult, Media};
use std::{ffi::c_void, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::Duration};
use base64::Engine;
use tauri::Manager;

// ---------------------------------------------------------------- CoreGraphics (mouse and keyboard state, no permission needed)
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint { x: f64, y: f64 }
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceSecondsSinceLastEventType(state: i32, kind: u32) -> f64;
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" { fn CFRelease(cf: *const c_void); }
/// kCGEventSourceStateCombinedSessionState: what every app sees.
const SESSION: i32 = 0;

/// Left, right and middle mouse buttons held right now (read only: clicks still go to the app under the cursor).
pub(crate) fn mouse_down() -> [bool; 3] { [0, 1, 2].map(|b| unsafe { CGEventSourceButtonState(SESSION, b) }) }
/// The cursor in physical px, top-left origin (as Tauri gives window positions).
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    let p = unsafe {
        let e = CGEventCreate(std::ptr::null());
        if e.is_null() { return None; }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        p
    };
    // CoreGraphics gives points; Tauri's physical px are points times the display's scale.
    let k = SCALE.lock().ok().and_then(|s| *s).unwrap_or(2.0);
    Some(((p.x * k).round() as i32, (p.y * k).round() as i32))
}
/// The cursor in points, top-left origin of the main display (CoreGraphics' own coordinates).
pub(crate) fn cursor_points() -> Option<(f64, f64)> {
    unsafe {
        let e = CGEventCreate(std::ptr::null());
        if e.is_null() { return None; }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        Some((p.x, p.y))
    }
}
/// The scale of the notch's display, kept by place_window (Retina is 2).
static SCALE: Mutex<Option<f64>> = Mutex::new(None);
/// Whether `key` (a macOS virtual key code) is held, and the modifier flags held now (kCGEventFlagMask*).
pub(crate) fn key_down(key: u16) -> bool { unsafe { CGEventSourceKeyState(SESSION, key) } }
pub(crate) fn modifier_flags() -> u64 { unsafe { CGEventSourceFlagsState(SESSION) } }
/// Seconds since the last keyboard or mouse input (kCGAnyInputEventType).
pub(crate) fn idle_seconds() -> f64 { unsafe { CGEventSourceSecondsSinceLastEventType(SESSION, u32::MAX) } }

// ---------------------------------------------------------------- the notch window
/// How far below the display's top edge Sora may start: the menu bar, which is the camera notch's height on MacBooks that
/// have one. Physical px.
pub(crate) fn top_inset(monitor: &tauri::Monitor) -> i32 { (monitor.work_area().position.y - monitor.position().y).max(0) }

/// A borderless notch window on macOS: on every Space and over full-screen apps, above other floating windows, out of the
/// window cycle (Cmd-`), never in the Dock (Info.plist LSUIElement).
pub(crate) fn hide_frame(window: &tauri::WebviewWindow) {
    let _ = window.set_visible_on_all_workspaces(true);
    let Ok(ns) = window.ns_window() else { return };
    let ns = ns as *mut objc2::runtime::AnyObject;
    if ns.is_null() { return; }
    // NSWindowCollectionBehavior: CanJoinAllSpaces | Stationary | IgnoresCycle | FullScreenAuxiliary.
    const BEHAVIOR: usize = 1 << 0 | 1 << 4 | 1 << 6 | 1 << 8;
    unsafe {
        let now: usize = objc2::msg_send![ns, collectionBehavior];
        let _: () = objc2::msg_send![ns, setCollectionBehavior: now | BEHAVIOR];
        // NSStatusWindowLevel: over normal floating windows, as the menu bar's own items are.
        let _: () = objc2::msg_send![ns, setLevel: 25isize];
        let _: () = objc2::msg_send![ns, setHasShadow: false];
    }
}

/// This Mac, once (system_profiler takes a moment; written to notch.log): {"name": "MacBook Air", "chip": "Apple M1", "model": "MacBookAir10,1",
/// "silicon": true, "notch": false}. `notch`: the main display has a camera notch (from the display itself, so a scaled
/// mode that puts the menu bar below the notch counts as none).
pub(crate) fn machine() -> Option<serde_json::Value> {
    static MACHINE: std::sync::OnceLock<Option<(String, String, String)>> = std::sync::OnceLock::new();
    let (name, chip, model) = MACHINE.get_or_init(|| {
        let out = std::process::Command::new("/usr/sbin/system_profiler").args(["-json", "SPHardwareDataType"]).output().ok()?;
        let (name, chip, model) = hardware(&String::from_utf8_lossy(&out.stdout))?;
        // An older system_profiler without the chip: the CPU's own name.
        let chip = if chip.is_empty() { std::process::Command::new("/usr/sbin/sysctl").args(["-n", "machdep.cpu.brand_string"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default() } else { chip };
        Some((name, chip, model))
    }).clone()?;
    let silicon = chip.starts_with("Apple");
    Some(serde_json::json!({ "name": name, "chip": chip, "model": model, "silicon": silicon, "notch": crate::notch_ui::camera_notch() }))
}
/// Name, chip (Apple silicon) or CPU (Intel) and model id from `system_profiler -json SPHardwareDataType` (pure; tested).
fn hardware(json: &str) -> Option<(String, String, String)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let hw = v.get("SPHardwareDataType")?.get(0)?;
    let text = |k: &str| hw.get(k).and_then(|s| s.as_str()).unwrap_or("").trim().to_string();
    let name = text("machine_name");
    if name.is_empty() { return None; }
    let chip = Some(text("chip_type")).filter(|c| !c.is_empty()).unwrap_or_else(|| text("cpu_type"));
    Some((name, chip, text("machine_model")))
}
/// How many left clicks the system has seen: a click shows as a change, however short it was.
pub(crate) fn left_clicks() -> u32 {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" { fn CGEventSourceCounterForEventType(state: i32, event: u32) -> u32; }
    // kCGEventSourceStateCombinedSessionState, kCGEventLeftMouseDown.
    unsafe { CGEventSourceCounterForEventType(0, 1) }
}
/// Brings a shown window in front even while another app is active: show() alone leaves it behind there, since macOS
/// no longer lets an app take the focus by itself.
pub(crate) fn show_on_top(window: &tauri::WebviewWindow) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ns) = w.ns_window() else { return };
        let ns = ns as *mut objc2::runtime::AnyObject;
        if ns.is_null() { return; }
        unsafe { let _: () = objc2::msg_send![ns, orderFrontRegardless]; }
    });
}

/// The closed notch's clickable box inside the window (physical px: left, top, right, bottom), or None while open.
static HOT: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);
static WATCH: AtomicBool = AtomicBool::new(false);
/// Moves and sizes the notch window (physical px). Windows clips a closed notch to its shape; macOS has no window regions,
/// so the transparent window ignores the mouse while closed, except while the pointer is over the mini notch itself.
pub(crate) fn place_window(window: &tauri::WebviewWindow, x: i32, y: i32, width: u32, height: u32, expanded: bool, k: f64) -> AppResult<()> {
    if let Ok(mut s) = SCALE.lock() { *s = window.scale_factor().ok(); }
    let same = window.outer_position().ok() == Some(tauri::PhysicalPosition::new(x, y)) && window.outer_size().ok() == Some(tauri::PhysicalSize::new(width, height));
    if !same {
        window.set_position(tauri::PhysicalPosition::new(x, y)).map_err(err)?;
        window.set_size(tauri::PhysicalSize::new(width, height)).map_err(err)?;
    }
    // The same box as the Windows region: 292 x 66 design px plus the two 13 px ears.
    let s = |v: f64| (v * k).round() as i32;
    let (l, r) = ((width as i32 - s(292.0)) / 2 - s(12.0), (width as i32 + s(292.0)) / 2 + s(12.0));
    *HOT.lock().map_err(err)? = (!expanded).then_some((l, 0, r, s(66.0)));
    let _ = window.set_ignore_cursor_events(!expanded);
    if !WATCH.swap(true, Ordering::SeqCst) {
        let w = window.clone();
        std::thread::spawn(move || {
            let mut inside = false;
            loop {
                std::thread::sleep(Duration::from_millis(30));
                // The edge handle (desktop.rs) keeps the whole window click-through and watches the hover itself.
                let edge = w.app_handle().try_state::<crate::desktop::Desktop>().is_some_and(|d| d.edge.load(Ordering::SeqCst));
                let Some(hot) = HOT.lock().ok().and_then(|h| *h).filter(|_| !edge) else { inside = false; continue };
                let (Some((cx, cy)), Ok(pos)) = (cursor_pos(), w.outer_position()) else { continue };
                let (rx, ry) = (cx - pos.x, cy - pos.y);
                let now = rx >= hot.0 && rx < hot.2 && ry >= hot.1 && ry < hot.3;
                if now != inside { inside = now; let _ = w.set_ignore_cursor_events(!now); }
            }
        });
    }
    Ok(())
}

pub(crate) fn open_path(target: &str) -> AppResult<()> {
    let ok = std::process::Command::new("open").arg(target).status().map_err(err)?.success();
    if ok { Ok(()) } else { Err("macOS could not open this file".into()) }
}

// ---------------------------------------------------------------- Now playing (Spotify and Music, through AppleScript)
// macOS 15.4 closed MediaRemote to other apps, so the two players are asked directly. Neither is ever started by this:
// "is running" is checked first. The first ask shows macOS's Automation prompt (Info.plist NSAppleEventsUsageDescription).
const NOW: &str = r#"
on pick(p)
    if p is "spotify" then
        tell application "Spotify"
            if player state is stopped then return ""
            set t to current track
            return "spotify" & tab & (player state as text) & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & (player position as text) & tab & ((duration of t) / 1000 as text) & tab & (artwork url of t)
        end tell
    else
        tell application "Music"
            if player state is stopped then return ""
            set t to current track
            return "music" & tab & (player state as text) & tab & (name of t) & tab & (artist of t) & tab & (album of t) & tab & (player position as text) & tab & (duration of t as text) & tab & ""
        end tell
    end if
end pick
set found to ""
repeat with p in {"spotify", "music"}
    set appName to "Spotify"
    if p as text is "music" then set appName to "Music"
    if application appName is running then
        try
            set r to pick(p as text)
            if r is not "" then
                if r contains (tab & "playing" & tab) then return r
                if found is "" then set found to r
            end if
        end try
    end if
end repeat
return found
"#;
fn osascript(script: &str) -> Option<String> {
    let out = std::process::Command::new("osascript").arg("-e").arg(script).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string())
}
/// A number AppleScript printed, in any locale ("12,5" or "12.5").
fn number(s: &str) -> f64 { s.trim().replace(',', ".").parse().unwrap_or(0.0) }
/// One "app<TAB>state<TAB>title<TAB>artist<TAB>album<TAB>position<TAB>duration<TAB>art" line (pure; tested).
fn parse_now(line: &str) -> Option<(String, bool, String, String, String, f64, f64, String)> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 8 || f[2].is_empty() { return None; }
    Some((f[0].into(), f[1] == "playing", f[2].into(), f[3].into(), f[4].into(), number(f[5]), number(f[6]), f[7].trim().into()))
}
fn bundle(app: &str) -> &'static str { if app == "spotify" { "com.spotify.client" } else { "com.apple.Music" } }
pub(crate) fn media_read(with_art: bool) -> AppResult<Option<Media>> {
    let Some((app, playing, title, artist, album, position, duration, art_url)) = osascript(NOW).as_deref().and_then(parse_now) else { return Ok(None) };
    // The cover is fetched (and base64-encoded) once per track, as on Windows.
    static ART: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
    let key = format!("{title}|{artist}|{album}|{app}");
    let artwork = if !with_art || art_url.is_empty() { None } else if let Some(a) = ART.lock().ok().filter(|a| a.0 == key && !a.1.is_empty()).map(|a| a.1.clone()) { Some(a) } else {
        let fetched = tauri::async_runtime::block_on(async {
            let res = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?.get(&art_url).send().await.ok()?.error_for_status().ok()?;
            let mime = res.headers().get("content-type").and_then(|v| v.to_str().ok()).filter(|m| m.starts_with("image/")).unwrap_or("image/jpeg").to_string();
            let bytes = res.bytes().await.ok().filter(|b| b.len() < 4 << 20)?;
            Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
        });
        if let (Some(a), Ok(mut slot)) = (&fetched, ART.lock()) { *slot = (key, a.clone()); }
        fetched
    };
    Ok(Some(Media { title, artist, album, source: bundle(&app).into(), playing, position: position.clamp(0.0, duration.max(0.0)), duration, playback_rate: 1.0, artwork, can_seek: true, can_play: true, can_next: true, can_previous: true }))
}
/// Drives the player that is showing: "toggle", "next", "previous" or "seek" (position in seconds).
pub(crate) fn media_command(action: &str, position: Option<f64>) -> AppResult<()> {
    let Some((app, ..)) = osascript(NOW).as_deref().and_then(parse_now) else { return Err("Nothing is playing.".into()) };
    let verb = match action {
        "toggle" => "playpause".to_string(),
        "next" => "next track".to_string(),
        "previous" => "previous track".to_string(),
        "seek" => {
            let p = position.unwrap_or(0.0);
            if !p.is_finite() || p < 0.0 { return Err("Invalid position".into()); }
            format!("set player position to {p:.2}")
        }
        _ => return Err("Unknown control".into()),
    };
    let name = if app == "spotify" { "Spotify" } else { "Music" };
    osascript(&format!("tell application \"{name}\" to {verb}")).map(|_| ()).ok_or_else(|| "This player does not support that control".into())
}
pub(crate) fn media_source_icon(source: String) -> AppResult<String> {
    let app = match source.as_str() { "com.spotify.client" => "/Applications/Spotify.app", "com.apple.Music" => "/System/Applications/Music.app", _ => return Err("Media source changed".into()) };
    shell_image(app.into(), true)
}

// ---------------------------------------------------------------- pictures
/// The icon the Finder shows for `path` (an app's own), as a 128 px PNG.
fn app_icon(path: &str) -> Option<Vec<u8>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    if path.is_empty() { return None; }
    let image = NSWorkspace::sharedWorkspace().iconForFile(&objc2_foundation::NSString::from_str(path));
    let tiff = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &objc2_foundation::NSDictionary::new()) }?;
    // The largest of the icon's sizes comes out; 128 px is plenty for a 20 px badge on a 2x screen.
    let img = image::load_from_memory(&png.to_vec()).ok()?.resize(128, 128, image::imageops::FilterType::Lanczos3);
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}
/// The Finder's picture for a path as a PNG data URL (Quick Look: a thumbnail, or the icon at 256 px).
pub(crate) fn shell_image(path: String, icon_only: bool) -> AppResult<String> {
    // An app's icon (or any file's) from NSWorkspace: Quick Look gave nothing for Safari (it lives in a sealed volume).
    if icon_only { return app_icon(&path).map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))).ok_or_else(|| "Icon unavailable".into()); }
    let dir = tempfile::tempdir().map_err(err)?;
    let size = if icon_only { "256" } else { "128" };
    let mut cmd = std::process::Command::new("qlmanage");
    cmd.arg("-t").args(["-s", size]).arg("-o").arg(dir.path());
    if icon_only { cmd.arg("-i"); }
    let ok = cmd.arg(&path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map_err(err)?.success();
    let png = std::fs::read_dir(dir.path()).map_err(err)?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "png"));
    let bytes = png.filter(|_| ok).and_then(|p| std::fs::read(p).ok()).ok_or("Thumbnail unavailable")?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The signed manifest only has Windows helpers.
pub(crate) const CHAT_HELPER_UPDATES: bool = false;
/// A menu bar icon: with no Dock icon and no app menu (LSUIElement), it is the sure way to Settings and Quit.
pub(crate) const TRAY_VISIBLE: bool = true;
/// The Beta features (Meeting, the chat apps) are not offered on macOS (they did not work well there).
pub(crate) const BETA: bool = false;
/// Sora.app replaces itself from the signed Sora-macos-<cpu>.zip of a GitHub release (update.rs; build-macos.sh --release).
pub(crate) const SELF_UPDATES: bool = true;

/// Deletes `paths` once this Sora has exited (uninstall.rs); Sora.app goes to the Trash, the data is removed.
pub(crate) fn remove_after_exit(paths: &[std::path::PathBuf]) -> AppResult<()> {
    let script = "while kill -0 \"$0\" 2>/dev/null; do sleep 0.3; done; for p in \"$@\"; do case \"$p\" in *.app) mv -f \"$p\" \"$HOME/.Trash/\" 2>/dev/null || rm -rf \"$p\";; *) rm -rf \"$p\";; esac; done";
    std::process::Command::new("/bin/sh").arg("-c").arg(script).arg(std::process::id().to_string()).args(paths)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("Could not start the uninstall ({e})."))?;
    Ok(())
}
/// The bundle a self-update replaced (Sora.app.old) is removed on the next start.
pub(crate) fn remove_old_update() {
    let Ok(exe) = std::env::current_exe() else { return };
    if let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) { let _ = std::fs::remove_dir_all(bundle.with_extension("app.old")); }
}
/// A command-line tool by name: PATH, then where installers put them. An app opened from the Finder gets a short PATH
/// (no Homebrew, no ~/.local/bin), so those places are looked at too.
pub(crate) fn find_cli(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(std::path::PathBuf::from));
    if let Some(h) = home { for d in [".local/bin", ".npm-global/bin", ".bun/bin", ".volta/bin", ".claude/local"] { dirs.push(h.join(d)); } }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}
/// Child processes never open a console window on macOS.
pub(crate) fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command { cmd }


#[cfg(test)]
mod tests {
    use super::hardware;
    #[test]
    fn this_mac_from_system_profiler() {
        let m1 = r#"{"SPHardwareDataType":[{"chip_type":"Apple M1","machine_model":"MacBookAir10,1","machine_name":"MacBook Air"}]}"#;
        assert_eq!(hardware(m1), Some(("MacBook Air".into(), "Apple M1".into(), "MacBookAir10,1".into())));
        let intel = r#"{"SPHardwareDataType":[{"cpu_type":"Quad-Core Intel Core i5","machine_model":"MacBookPro16,2","machine_name":"MacBook Pro"}]}"#;
        assert_eq!(hardware(intel).map(|h| h.1), Some("Quad-Core Intel Core i5".into()));
        assert_eq!(hardware("{}"), None);
    }
    #[test]
    fn reads_the_players_line() {
        let s = super::parse_now("spotify\tplaying\tSong\tArtist\tAlbum\t12,5\t200.25\thttps://i.scdn.co/x").unwrap();
        assert_eq!((s.0.as_str(), s.1, s.2.as_str(), s.5, s.6, s.7.as_str()), ("spotify", true, "Song", 12.5, 200.25, "https://i.scdn.co/x"));
        assert!(super::parse_now("").is_none() && super::parse_now("music\tpaused\t\ta\tb\t1\t2\t").is_none());
    }
}
