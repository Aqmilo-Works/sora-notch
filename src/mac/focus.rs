//! The macOS side of focus (focus.rs), its `os` module. Everything else there is shared.
//! macOS has no API for apps to turn Focus on, so Sora runs two Shortcuts you make once in the Shortcuts app:
//! "Sora Focus On" (Set Focus: Do Not Disturb, On) and "Sora Focus Off" (Set Focus: Off).
#![allow(unused_imports)]
use super::*;

const ON: &str = "Sora Focus On";
const OFF: &str = "Sora Focus Off";

/// The timer's end time Sora turned Focus on for, if it did.
#[derive(Default)]
pub struct FocusState(Mutex<Option<i64>>);

fn shortcut_exists(name: &str) -> bool {
    std::process::Command::new("shortcuts").arg("list").output().ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == name))
}
fn run_shortcut(name: &str) -> bool { std::process::Command::new("shortcuts").args(["run", name]).status().is_ok_and(|s| s.success()) }

#[tauri::command]
pub fn sync_windows_focus(state:tauri::State<FocusState>,end_time:Option<i64>)->Result<FocusStatus,String>{
    let mut owned = state.0.lock().map_err(|e| e.to_string())?;
    let on = FocusStatus { active: true, message: "Focus is on through your \u{201c}Sora Focus On\u{201d} shortcut.".into() };
    if owned.is_some() && *owned == end_time { return Ok(on); }
    if owned.take().is_some() && shortcut_exists(OFF) { run_shortcut(OFF); }
    if end_time.is_none() { return Ok(FocusStatus { active: false, message: String::new() }); }
    if !shortcut_exists(ON) {
        return Ok(FocusStatus { active: false, message: "To turn on Focus with the timer, make two Shortcuts: \u{201c}Sora Focus On\u{201d} (Set Focus: On) and \u{201c}Sora Focus Off\u{201d} (Set Focus: Off).".into() });
    }
    if !run_shortcut(ON) { return Ok(FocusStatus { active: false, message: "The \u{201c}Sora Focus On\u{201d} shortcut did not run.".into() }); }
    *owned = end_time;
    Ok(on)
}
pub fn stop_owned(state:&FocusState){
    if state.0.lock().ok().and_then(|mut o| o.take()).is_some() && shortcut_exists(OFF) { run_shortcut(OFF); }
}
