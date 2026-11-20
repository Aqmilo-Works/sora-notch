//! The Agents tab's extras, as in Coucou: start a Claude Code session from the notch, drop a file's path into a session,
//! sounds for agent moments (yours can replace them), and the weekly recap.
//! Behaviour follows Coucou (github.com/louis-cfm/coucou, MIT, Copyright (c) 2026 Louis Raillé; see THIRD-PARTY-NOTICES.md).
#![cfg_attr(not(win), allow(dead_code))]
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Mutex};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
// What differs per OS lives in win/agent_extras.rs and mac/agent_extras.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/agent_extras.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/agent_extras.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

fn data_dir(app: &AppHandle) -> Option<PathBuf> { app.path().app_data_dir().ok() }

// ---------------------------------------------------------------- a new Claude Code session
/// Folders sessions ran in (newest first), then the ones picked before; only folders that still exist, 8 at most.
#[tauri::command]
pub fn agent_folders(app: AppHandle) -> Vec<String> {
    let saved: Vec<String> = data_dir(&app).and_then(|d| std::fs::read(d.join("agent-folders.json")).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut out: Vec<String> = vec![];
    for f in crate::agents::recent_folders().into_iter().chain(saved) {
        if !f.is_empty() && Path::new(&f).is_dir() && !out.iter().any(|o| o.eq_ignore_ascii_case(&f)) { out.push(f); }
        if out.len() >= 8 { break; }
    }
    out
}
fn remember_folder(app: &AppHandle, folder: &str) {
    let Some(dir) = data_dir(app) else { return };
    let mut list = agent_folders(app.clone());
    list.retain(|f| !f.eq_ignore_ascii_case(folder));
    list.insert(0, folder.to_string());
    list.truncate(8);
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("agent-folders.json"), serde_json::to_vec(&list).unwrap_or_default());
}
#[tauri::command]
pub async fn agent_pick_folder() -> Option<String> {
    let _aside = crate::Aside::new();
    rfd::AsyncFileDialog::new().set_title("Folder for the new session").pick_folder().await.map(|p| p.path().display().to_string())
}
/// What you typed, as one line the agent takes as its first prompt: no control characters, 4000 characters at most (pure; tested).
pub fn prompt_arg(s: &str) -> String {
    let one: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(4000).collect()
}
/// The agents a new session can start with (their CLI is on this PC), and the name to show.
fn launcher(agent: &str) -> Option<(PathBuf, &'static str)> {
    match agent {
        "claude-code" => claude_exe().map(|p| (p, "Claude Code")),
        "codex" => crate::agent_usage::codex_exe().map(|p| (p, "Codex")),
        _ => None,
    }
}
/// Which agents a new session can start with here, in the order the sheet shows them.
#[tauri::command]
pub fn agent_launchers() -> Vec<&'static str> { ["claude-code", "codex"].into_iter().filter(|a| launcher(a).is_some()).collect() }

// ---------------------------------------------------------------- a file dropped on a session
/// A path as a terminal drop writes it: quoted when it has a space (pure; tested).
pub fn dropped_path(p: &str) -> String { if p.contains(' ') { format!("\"{p}\" ") } else { format!("{p} ") } }
/// Brings the session's window forward and types the file's path into its prompt; when no window is found, the path
/// is copied instead (the page says so).
#[tauri::command]
pub fn agent_drop(session: String, path: String) -> Result<&'static str, String> {
    if !Path::new(&path).exists() { return Err("That file is gone.".into()); }
    let text = dropped_path(&path);
    if can_type() && crate::agents::focus_session(&session) {
        std::thread::sleep(std::time::Duration::from_millis(120));
        type_text(&text);
        Ok("typed")
    } else {
        crate::board::board_put_text(text.trim().to_string()).map_err(|e| e.to_string())?;
        Ok("copied")
    }
}

// ---------------------------------------------------------------- shortcuts

// ---------------------------------------------------------------- sounds
/// The sound for an agent moment: yours (finish / approval / question .wav in Sora's sounds folder) or Sora's own.
pub fn play(app: &AppHandle, moment: &str) {
    if !crate::desktop::preferences(app).agent_sounds { return; }
    let own = data_dir(app).map(|d| d.join("sounds").join(format!("{moment}.wav"))).filter(|p| p.is_file()).and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() < 4 << 20);
    play_wav(own.unwrap_or_else(|| tone(moment)));
}
/// Sora's own sounds: two soft notes rising (finished), two falling (it needs you), three for a question (pure).
pub fn tone(moment: &str) -> Vec<u8> {
    let notes: &[f32] = match moment { "approval" => &[880.0, 659.25], "question" => &[659.25, 783.99, 987.77], _ => &[659.25, 987.77] };
    let rate = 44_100u32;
    let len = ((notes.len() as f32 * 0.14 + 0.5) * rate as f32) as usize;
    let mut s = vec![0f32; len];
    for (i, hz) in notes.iter().enumerate() {
        let start = (i as f32 * 0.14 * rate as f32) as usize;
        for n in 0..(0.45 * rate as f32) as usize {
            let t = n as f32 / rate as f32;
            let gain = if t < 0.015 { 0.07 * t / 0.015 } else { 0.07 * (0.001f32 / 0.07).powf(((t - 0.015) / 0.43).min(1.0)) };
            if let Some(x) = s.get_mut(start + n) { *x += gain * (std::f32::consts::TAU * hz * t).sin(); }
        }
    }
    let data: Vec<u8> = s.iter().flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut w = Vec::with_capacity(44 + data.len());
    w.extend_from_slice(b"RIFF"); w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes()); w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); w.extend_from_slice(&(rate * 2).to_le_bytes()); w.extend_from_slice(&2u16.to_le_bytes()); w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data"); w.extend_from_slice(&(data.len() as u32).to_le_bytes()); w.extend_from_slice(&data);
    w
}
/// Opens the folder where your own sounds go (made on first use).
#[tauri::command]
pub fn agent_sounds_folder(app: AppHandle) -> Result<(), String> {
    let dir = data_dir(&app).ok_or("No app folder")?.join("sounds");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new(FILE_MANAGER).arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- the weekly recap
/// One week of counts: never a command, a file path, file contents or a prompt (as Coucou's recap.json).
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Week {
    pub sessions: u32, pub turns: u32, pub minutes: f64, pub files: u32, pub added: u64, pub removed: u64,
    pub commands: u32, pub permissions: u32, pub questions: u32, pub longest_minutes: f64,
    pub agents: BTreeMap<String, u32>, pub projects: BTreeMap<String, u32>, pub days: BTreeMap<String, u32>,
}
#[derive(Serialize, Deserialize, Default)]
struct Recap { on: Option<bool>, weeks: BTreeMap<String, Week>, seen: String }
static RECAP: Mutex<Option<Recap>> = Mutex::new(None);
fn recap_file(app: &AppHandle) -> Option<PathBuf> { data_dir(app).map(|d| d.join("recap.json")) }
/// The Monday (local time) that starts the week of `ms`, as "YYYY-MM-DD", and the weekday name (pure; tested).
pub fn week_of(ms: i64, offset_min: i64) -> (String, &'static str) {
    let days = (ms / 60000 + offset_min).div_euclid(1440);
    // 1970-01-01 was a Thursday.
    let wd = (days + 3).rem_euclid(7);
    let monday = days - wd;
    const NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    (civil(monday), NAMES[wd as usize])
}
fn civil(days: i64) -> String {
    let z = days + 719_468; let era = z.div_euclid(146_097); let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}
fn days_from(y: i64, m: i64, d: i64) -> i64 { let y = if m <= 2 { y - 1 } else { y }; let era = y.div_euclid(400); let yoe = y - era * 400; let mp = (m + 9) % 12; let doy = (153 * mp + 2) / 5 + d - 1; let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; era * 146_097 + doe - 719_468 }
fn with_recap<R>(app: &AppHandle, f: impl FnOnce(&mut Recap) -> R) -> Option<R> {
    let mut g = RECAP.lock().ok()?;
    if g.is_none() { *g = Some(recap_file(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()); }
    let r = g.as_mut()?;
    let out = f(r);
    if let Some(p) = recap_file(app) { let _ = std::fs::write(p, serde_json::to_vec(r).unwrap_or_default()); }
    Some(out)
}
/// Counts one agent event into this week (12 weeks are kept). `turn_ms`: how long a finished turn took.
pub fn count(app: &AppHandle, agent: &str, project: &str, kind: &str, tool: &str, input: &Value, turn_ms: Option<i64>, now: i64) {
    let (week, day) = week_of(now, local_offset_min());
    let _ = with_recap(app, |r| {
        if r.on == Some(false) { return; }
        let w = r.weeks.entry(week).or_default();
        let bump = |m: &mut BTreeMap<String, u32>, k: &str| { if !k.is_empty() { *m.entry(k.to_string()).or_default() += 1; } };
        match kind {
            "SessionStart" => w.sessions += 1,
            "UserPromptSubmit" => { w.turns += 1; bump(&mut w.agents, agent); bump(&mut w.projects, project); bump(&mut w.days, day); }
            "PermissionRequest" => w.permissions += 1,
            "PreToolUse" if tool == "AskUserQuestion" => w.questions += 1,
            "PreToolUse" if matches!(tool, "Bash" | "PowerShell" | "run_shell_command" | "shell" | "run_command" | "bash") => w.commands += 1,
            "PostToolUse" => for c in crate::agents::changes_of(tool, input, now) { w.files += 1; w.added += c.added() as u64; w.removed += c.removed() as u64; },
            "Stop" | "StopFailure" => if let Some(ms) = turn_ms.filter(|m| *m > 0 && *m < 24 * 3_600_000) { let min = ms as f64 / 60000.0; w.minutes += min; w.longest_minutes = w.longest_minutes.max(min); },
            _ => {}
        }
        while r.weeks.len() > 12 { let first = r.weeks.keys().next().cloned(); if let Some(k) = first { r.weeks.remove(&k); } }
    });
}
/// Last week's recap (or this week's when asked from the tab), and whether it is new since you last saw one.
#[tauri::command]
pub fn agent_recap(app: AppHandle, current: bool) -> Value {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let (this, _) = week_of(now, local_offset_min());
    let (last, _) = week_of(now - 7 * 86_400_000, local_offset_min());
    let key = if current { this } else { last };
    with_recap(&app, |r| {
        let w = r.weeks.get(&key).cloned();
        let fresh = !current && r.on != Some(false) && w.as_ref().is_some_and(|w| w.turns > 0) && r.seen != key;
        json!({ "week": key, "data": w, "fresh": fresh, "on": r.on != Some(false) })
    }).unwrap_or(Value::Null)
}
#[tauri::command]
pub fn agent_recap_seen(app: AppHandle, week: String) { let _ = with_recap(&app, |r| r.seen = week); }
/// Turns the recap off (and forgets it) or back on.
#[tauri::command]
pub fn agent_recap_set(app: AppHandle, on: bool) { let _ = with_recap(&app, |r| { r.on = Some(on); if !on { r.weeks.clear(); } }); }
/// Saves the share image (a PNG made by the page) in Pictures, never over an existing file.
#[tauri::command]
pub fn agent_recap_save(week: String, png: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(png.trim_start_matches("data:image/png;base64,")).map_err(|e| e.to_string())?;
    if bytes.len() > 20 << 20 || !bytes.starts_with(b"\x89PNG") { return Err("Not a picture.".into()); }
    let home = PathBuf::from(std::env::var_os("USERPROFILE").ok_or("No user folder")?);
    let dir = [home.join("Pictures"), home.join("Downloads")].into_iter().find(|d| d.is_dir()).ok_or("No Pictures folder")?;
    let safe: String = week.chars().filter(|c| c.is_ascii_digit() || *c == '-').take(10).collect();
    let mut path = dir.join(format!("Sora weekly recap {safe}.png"));
    let mut n = 2;
    while path.exists() { path = dir.join(format!("Sora weekly recap {safe} ({n}).png")); n += 1; }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_prompt_is_one_clean_line() {
        assert_eq!(prompt_arg("fix\nthe  bug\t\u{1b}[31m"), "fix the bug [31m");
        assert_eq!(prompt_arg(&"a".repeat(5000)).len(), 4000);
        assert_eq!(dropped_path("C:\\a b\\c.png"), "\"C:\\a b\\c.png\" ");
        assert_eq!(dropped_path("C:\\x.txt"), "C:\\x.txt ");
    }
    #[test]
    fn weeks_start_on_monday_in_local_time() {
        // 2026-11-13 (a Friday) 10:00 UTC; at UTC+7 too.
        let ms = 1_794_564_000_000;
        assert_eq!(week_of(ms, 0), ("2026-11-09".to_string(), "Friday"));
        assert_eq!(week_of(ms, 420), ("2026-11-09".to_string(), "Friday"));
        // Monday 2026-11-09 00:30 at UTC+7 is still Sunday in UTC.
        let monday_early = 1_794_159_000_000;
        assert_eq!(week_of(monday_early, 420).0, "2026-11-09");
        assert_eq!(week_of(monday_early, 0).0, "2026-11-02");
    }
    #[test]
    fn dates_round_trip() { assert_eq!(civil(days_from(2026, 11, 13)), "2026-11-13"); assert_eq!(civil(days_from(2024, 2, 29)), "2024-02-29"); }
    #[test]
    fn sounds_are_wav() { for m in ["finish", "approval", "question"] { let w = tone(m); assert!(w.starts_with(b"RIFF") && w.len() > 44 + 20_000, "{m}"); } }
}
