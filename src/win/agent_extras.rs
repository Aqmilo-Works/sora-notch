//! The Windows side of agent_extras (agent_extras.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub fn agent_new_session(app: AppHandle, folder: String, prompt: String, agent: Option<String>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let dir = PathBuf::from(folder.trim());
    if !dir.is_absolute() || !dir.is_dir() { return Err("That folder is not there.".into()); }
    let (exe, name) = launcher(agent.as_deref().unwrap_or("claude-code")).ok_or("That agent is not installed on this PC.")?;
    let mut cmd = std::process::Command::new(&exe);
    let p = prompt_arg(&prompt);
    if !p.is_empty() { cmd.arg(&p); }
    // CREATE_NEW_CONSOLE: its own window, so the session lives on when the notch closes.
    cmd.current_dir(&dir).creation_flags(0x0000_0010).spawn().map_err(|e| format!("Could not start {name}: {e}"))?;
    remember_folder(&app, &dir.display().to_string());
    Ok(())
}
/// Types text into the window in front as Unicode keystrokes (no clipboard is touched).
pub(super) fn type_text(text: &str) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let mut inputs = vec![];
    for u in text.encode_utf16() {
        for up in [false, true] {
            inputs.push(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: u, dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } } });
        }
    }
    unsafe { let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32); }
}
pub fn start_hotkeys() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};
    static ON: AtomicBool = AtomicBool::new(false);
    if ON.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(|| unsafe {
        for (id, vk) in [(0x5A01, b'A'), (0x5A02, b'T')] {
            if !types_a_character(vk as u16) { let _ = RegisterHotKey(None, id, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, vk as u32); }
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message != WM_HOTKEY || !crate::agents::running() { continue; }
            match msg.wParam.0 { 0x5A01 => crate::notch_ui::show_agents(), 0x5A02 => { if let Some(id) = crate::agents::terminal_target() { let _ = crate::agents::agent_open_terminal(id); } } _ => {} }
        }
    });
}
/// Whether Ctrl+Alt+key types something on any installed keyboard layout.
pub(super) fn types_a_character(vk: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        let n = GetKeyboardLayoutList(None);
        let mut list = vec![HKL::default(); n.max(0) as usize];
        let n = GetKeyboardLayoutList(Some(&mut list));
        let mut state = [0u8; 256];
        for k in [VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU] { state[k.0 as usize] = 0x80; }
        list.iter().take(n.max(0) as usize).any(|hkl| {
            let sc = MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC, *hkl);
            let mut buf = [0u16; 8];
            // Flag 4: do not change the keyboard state (a dead key stays as it was).
            ToUnicodeEx(vk as u32, sc, &state, &mut buf, 4, *hkl) > 0
        })
    }
}
/// Plays WAV bytes once, without waiting.
pub(super) fn play_wav(wav: Vec<u8>) {
    static PLAYING: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    // PlaySound reads from memory while it plays: the bytes stay alive until the next sound replaces them.
    if let Ok(mut keep) = PLAYING.lock() {
        *keep = wav;
        unsafe { let _ = windows::Win32::Media::Audio::PlaySoundW(windows::core::PCWSTR(keep.as_ptr() as *const u16), None, windows::Win32::Media::Audio::SND_MEMORY | windows::Win32::Media::Audio::SND_ASYNC | windows::Win32::Media::Audio::SND_NODEFAULT); }
    }
}
pub(super) const FILE_MANAGER: &str = "explorer.exe";
/// Minutes local time is ahead of UTC now (from the clock's two readings, rounded to a quarter hour).
pub(super) fn local_offset_min() -> i64 {
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    let (l, u) = unsafe { (GetLocalTime(), GetSystemTime()) };
    let mins = |t: windows::Win32::Foundation::SYSTEMTIME| days_from(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 1440 + t.wHour as i64 * 60 + t.wMinute as i64;
    ((mins(l) - mins(u)) as f64 / 15.0).round() as i64 * 15
}
/// Claude Code's program: the native install, npm's shim or anything on PATH.
pub(super) fn claude_exe() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if let Some(h) = std::env::var_os("USERPROFILE") { c.push(PathBuf::from(h).join(".local").join("bin").join("claude.exe")); }
    if let Some(path) = std::env::var_os("PATH") { for d in std::env::split_paths(&path) { c.push(d.join("claude.exe")); c.push(d.join("claude.cmd")); } }
    if let Some(a) = std::env::var_os("APPDATA") { c.push(PathBuf::from(a).join("npm").join("claude.cmd")); }
    c.into_iter().find(|p| p.is_file())
}
/// Windows types into any window in front.
pub(super) fn can_type() -> bool { true }
