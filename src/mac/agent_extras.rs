//! The macOS side of agent_extras (agent_extras.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;
use std::ffi::c_void;

/// Opens a Terminal window in `folder` running the agent (Claude Code or Codex) with your prompt, so the session lives on
/// when the notch closes. The first time, macOS asks whether Sora may control Terminal.
#[tauri::command]
pub fn agent_new_session(app: AppHandle, folder: String, prompt: String, agent: Option<String>) -> Result<(), String> {
    let dir = PathBuf::from(folder.trim());
    if !dir.is_absolute() || !dir.is_dir() { return Err("That folder is not there.".into()); }
    let (exe, name) = launcher(agent.as_deref().unwrap_or("claude-code")).ok_or("That agent is not installed on this Mac.")?;
    let p = prompt_arg(&prompt);
    let mut line = format!("cd {} && {}", shell_quote(&dir.display().to_string()), shell_quote(&exe.display().to_string()));
    if !p.is_empty() { line.push(' '); line.push_str(&shell_quote(&p)); }
    let script = format!("tell application \"Terminal\"\nactivate\ndo script \"{}\"\nend tell", applescript_text(&line));
    let ok = std::process::Command::new("osascript").arg("-e").arg(script).status().map_err(|e| format!("Could not start {name}: {e}"))?.success();
    if !ok { return Err(format!("Terminal did not start {name}.")); }
    remember_folder(&app, &dir.display().to_string());
    Ok(())
}
/// One shell word in single quotes (pure; tested).
pub(super) fn shell_quote(s: &str) -> String { format!("'{}'", s.replace('\'', r"'\''")) }
/// Text inside an AppleScript string literal (pure; tested).
pub(super) fn applescript_text(s: &str) -> String { s.replace('\\', r"\\").replace('"', "\\\"") }

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" { fn AXIsProcessTrusted() -> bool; }
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
    fn CGEventKeyboardSetUnicodeString(event: *mut c_void, len: usize, text: *const u16);
    fn CGEventPost(tap: u32, event: *mut c_void);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" { fn CFRelease(cf: *const c_void); }
/// Typing into another app needs Settings > Privacy & Security > Accessibility for Sora; without it the path is copied.
pub(super) fn can_type() -> bool { unsafe { AXIsProcessTrusted() } }
/// Types text into the window in front as Unicode key events (no clipboard is touched), 16 characters per event.
pub(super) fn type_text(text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    for chunk in units.chunks(16) {
        for down in [true, false] {
            unsafe {
                let e = CGEventCreateKeyboardEvent(std::ptr::null(), 0, down);
                if e.is_null() { return; }
                CGEventKeyboardSetUnicodeString(e, chunk.len(), chunk.as_ptr());
                CGEventPost(0, e); // kCGHIDEventTap
                CFRelease(e);
            }
        }
    }
}
/// Ctrl+Alt+A jumps to the waiting permission or question (opens the Agents tab), Ctrl+Alt+T brings the session's window
/// forward, as in Coucou: polled from the keyboard state like the notch's own shortcut (desktop.rs).
pub fn start_hotkeys() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ON: AtomicBool = AtomicBool::new(false);
    if ON.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(|| {
        // kVK_ANSI_A and kVK_ANSI_T, with Control + Option held (kCGEventFlagMaskControl | kCGEventFlagMaskAlternate).
        let mut held = [true, true];
        loop {
            std::thread::sleep(std::time::Duration::from_millis(40));
            let flags = crate::platform::modifier_flags();
            let mods = flags & 0x40000 != 0 && flags & 0x80000 != 0 && flags & (0x20000 | 0x100000) == 0;
            for (i, key) in [0x00u16, 0x11].into_iter().enumerate() {
                let now = mods && crate::platform::key_down(key);
                if now && !held[i] && crate::agents::running() {
                    if i == 0 { crate::notch_ui::show_agents(); } else if let Some(id) = crate::agents::terminal_target() { let _ = crate::agents::agent_open_terminal(id); }
                }
                held[i] = now;
            }
        }
    });
}
/// Plays WAV bytes once, without waiting (afplay, from a temporary file kept until the next sound).
pub(super) fn play_wav(wav: Vec<u8>) {
    static FILE: Mutex<Option<tempfile::NamedTempFile>> = Mutex::new(None);
    let Ok(mut slot) = FILE.lock() else { return };
    let Ok(mut f) = tempfile::Builder::new().suffix(".wav").tempfile() else { return };
    if std::io::Write::write_all(&mut f, &wav).is_err() { return; }
    let _ = std::process::Command::new("afplay").arg(f.path()).spawn();
    *slot = Some(f);
}
pub(super) const FILE_MANAGER: &str = "open";
/// Minutes local time is ahead of UTC now (the C library's own time zone data).
pub(super) fn local_offset_min() -> i64 {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() { return 0; }
        tm.tm_gmtoff / 60
    }
}
/// Claude Code's program: the native install, Homebrew, npm or anything on PATH.
pub(super) fn claude_exe() -> Option<PathBuf> { crate::platform::find_cli("claude") }

#[cfg(test)]
mod mac_tests {
    #[test]
    fn terminal_command_is_quoted() {
        assert_eq!(super::shell_quote("it's here"), r"'it'\''s here'");
        assert_eq!(super::applescript_text(r#"say "hi" \ bye"#), r#"say \"hi\" \\ bye"#);
    }
}
