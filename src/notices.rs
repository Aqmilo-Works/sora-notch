#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use std::{collections::HashMap, fs, path::Path, sync::mpsc, thread, time::{Duration, SystemTime, UNIX_EPOCH}};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
// What differs per OS lives in win/notices.rs and mac/notices.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/notices.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/notices.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const LATE_LIMIT: i64 = 5 * 60 * 1000;
/// The reminder window (dip); ui/notice.css draws the capsule 6 px narrower and 6 px shorter, 2 px from the top.
/// The reminder's window: Small (a 248 x 52 card) or Large (300 x 64), Settings > Notch > Notification size.
const NOTICE: (f64, f64) = (248.0, 58.0);
const NOTICE_LARGE: (f64, f64) = (300.0, 70.0);
fn notice_dims(app: &AppHandle) -> (f64, f64) { if crate::desktop::preferences(app).notice_size == "large" { NOTICE_LARGE } else { NOTICE } }
const NOTICE_TOP: f64 = 2.0;
/// Under the mini notch or the panel the reminder's pill sits this far (dip) below the notch.
const NOTICE_GAP: f64 = 6.0;
/// With the native notch the window starts this far (dip) above the pill's usual place: room for the pill to
/// reach up over the floating capsule and become one shape with it, without the window ever moving for that.
const NOTICE_REACH: f64 = 20.0;
fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
/// `kind`: "alarm" for the Alarm widget (rings), else a calendar event.
pub struct CalendarEntry { id: String, title: String, at: i64, time: String, #[serde(default)] kind: String, #[serde(default)] note: String }

#[derive(Default, Deserialize, Serialize)]
struct Agenda {
    entries: Vec<CalendarEntry>,
    delivered: HashMap<String, i64>,
}

/// `label`: the app's name for a chat notice; `target`: the app (AUMID) a click on it opens.
#[derive(Serialize, Default)]
/// `mark`: the app's own mark on a chat notice (a small badge on the sender's picture); `group`: a group's message;
/// `large`: Settings > Notch > Notification size.
struct Notice { kind: &'static str, title: String, detail: String, #[serde(skip)] label: String, #[serde(skip)] target: String, #[serde(skip)] image: Option<crate::draw::Picture>,
    #[serde(skip_serializing_if = "Option::is_none")] mark: Option<crate::marks::MarkView>, group: bool, large: bool,
    /// A LocalSend notice's step: "ask:<id>" (Accept / Deny), "busy" (the progress bar), "done".
    #[serde(skip_serializing_if = "String::is_empty")] receive: String }
pub struct Notices {
    calendar: mpsc::Sender<Vec<CalendarEntry>>,
    display: mpsc::Sender<Notice>,
}

pub fn timer_finished(app: &AppHandle) {
    let _ = app.state::<Notices>().display.send(Notice {
        kind: "timer", title: "Focus complete".into(), detail: "Time for a break.".into(), ..Default::default()
    });
}

/// The water reminder (the Water widget on Home).
pub fn water(app: &AppHandle) {
    let _ = app.state::<Notices>().display.send(Notice {
        kind: "water", title: "Time for water".into(), detail: "A glass now keeps you going.".into(), ..Default::default()
    });
}

/// An AI agent finished, stopped or waits for you (agents.rs): click the card to open the Agents tab.
pub fn agent(app: &AppHandle, title: &str, detail: &str) {
    let _ = app.state::<Notices>().display.send(Notice { kind: "agent", title: title.into(), detail: detail.into(), ..Default::default() });
}
/// An agent asks for permission: the notice carries Allow / Deny for that request (agents.rs answers it).
pub fn agent_ask(app: &AppHandle, title: &str, detail: &str, id: &str) {
    let _ = app.state::<Notices>().display.send(Notice { kind: "agent", title: title.into(), detail: detail.into(), target: format!("agent-ask:{id}"), ..Default::default() });
}
/// Receiving files with LocalSend (lan_receive.rs): `step` "ask" (with Accept / Deny for request `id`), "busy" (a bar
/// that follows the transfer until it ends) or "done".
pub fn receive(app: &AppHandle, step: &str, id: &str, title: &str, detail: &str) {
    let target = if step == "ask" { format!("ls-ask:{id}") } else { format!("ls-{step}") };
    let receive = if step == "ask" { format!("ask:{id}") } else { step.to_string() };
    let _ = app.state::<Notices>().display.send(Notice { kind: "receive", title: title.into(), detail: detail.into(), target, receive, ..Default::default() });
}
/// The meeting recorder started, stopped or failed (meeting.rs): click the card to open the Meeting tab.
pub fn meeting(app: &AppHandle, title: &str, detail: &str) {
    let _ = app.state::<Notices>().display.send(Notice { kind: "meeting", title: title.into(), detail: detail.into(), ..Default::default() });
}

/// A chat app's notification (see chat_notices): sender and message, opened in that app on click.
/// Straight to the notch, not through the queue of `present` (which waits for each card to leave): the notch has its own
/// queue, and a message from the sender already on show joins that card there. A chat notice never has a window of its own.
/// `sender` names who wrote in a group ("" otherwise), `body` is the message, `kind` what it is ("text", "photo", ...; ""
/// when unknown, as for other apps' toasts). With Settings > Notch > Hide message text on, the card names only the kind
/// ("Sent a photo"); the chat history keeps the text.
#[allow(clippy::too_many_arguments)]
pub fn chat(app: &AppHandle, label: &str, title: &str, sender: &str, body: &str, kind: &str, target: &str, image: Option<crate::draw::Picture>) {
    let hide = crate::desktop::preferences(app).hide_chat_text;
    let line = chat_line(sender, body, kind, hide);
    // Windows draws chat cards inside its notch; macOS shows them as the floating reminders do, with the app's mark.
    if cfg!(win) { let _ = crate::notch_ui::notify("chat", label, title, &line, target, image); return; }
    let mark = crate::marks::mark_index(label).map(|i| crate::marks::marks().swap_remove(i));
    let _ = app.state::<Notices>().display.send(Notice { kind: "chat", label: label.into(), title: title.into(), detail: line, target: target.into(), image, mark, group: !sender.is_empty(), ..Default::default() });
}
/// The line a chat card shows.
fn chat_line(sender: &str, body: &str, kind: &str, hide: bool) -> String {
    let shown = if hide { kind_line(kind) } else { body };
    if sender.is_empty() { shown.to_string() } else { format!("{sender}: {shown}") }
}
/// What a hidden message says instead of its text.
fn kind_line(kind: &str) -> &'static str {
    match kind {
        "photo" => "Sent a photo", "video" => "Sent a video", "gif" => "Sent a GIF", "sticker" => "Sent a sticker",
        "voice" => "Sent a voice message", "video_note" => "Sent a video message", "audio" => "Sent an audio file",
        "document" => "Sent a document", "location" => "Sent a location", "contact" => "Sent a contact", "poll" => "Sent a poll",
        _ => "Sent a message",
    }
}

#[cfg(test)]
mod chat_line_tests {
    #[test]
    fn a_hidden_message_names_only_its_kind() {
        assert_eq!(super::chat_line("", "Photo: liburan", "photo", false), "Photo: liburan");
        assert_eq!(super::chat_line("", "Photo: liburan", "photo", true), "Sent a photo");
        assert_eq!(super::chat_line("Sam", "rahasia", "text", true), "Sam: Sent a message", "a group keeps who wrote");
        assert_eq!(super::chat_line("", "Sticker", "sticker", true), "Sent a sticker");
        assert_eq!(super::chat_line("", "anything", "", true), "Sent a message", "other apps' toasts");
    }
}

#[tauri::command]
pub fn notify_focus_complete(app: AppHandle) { timer_finished(&app); }


#[tauri::command]
pub fn sync_calendar_notices(app: AppHandle, entries: Vec<CalendarEntry>) -> Result<(), String> {
    app.state::<Notices>().calendar.send(entries).map_err(|e| e.to_string())
}

fn save(path: &Path, agenda: &Agenda) -> std::io::Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_vec(agenda)?)?;
    fs::rename(temp, path)
}

impl Agenda {
    fn collect_due(&mut self, now: i64) -> Vec<Notice> {
        let mut due = Vec::new();
        self.delivered.retain(|_, at| *at >= now - LATE_LIMIT);
        self.entries.retain(|entry| {
            if self.delivered.get(&entry.id) == Some(&entry.at) { return false; }
            if entry.at > now { return true; }
            if entry.at >= now - LATE_LIMIT {
                self.delivered.insert(entry.id.clone(), entry.at);
                if entry.kind == "alarm" {
                    ring();
                    due.push(Notice { kind: "alarm", title: if entry.title.is_empty() { "Alarm".into() } else { entry.title.clone() }, detail: format!("It's {}", entry.time), ..Default::default() });
                } else {
                    due.push(Notice { kind: "event", title: entry.title.clone(), detail: if entry.note.trim().is_empty() { format!("Starts at {}", entry.time) } else { format!("{} · {}", entry.time, entry.note.trim()) }, ..Default::default() });
                }
            }
            false
        });
        due
    }
}

pub fn start(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("calendar-notices.json");
    let (display, queue) = mpsc::channel::<Notice>();
    let (calendar, updates) = mpsc::channel::<Vec<CalendarEntry>>();
    app.manage(Notices { calendar, display: display.clone() });
    let host = app.clone();
    thread::spawn(move || {
        for (id, mut notice) in queue.into_iter().enumerate() {
            if let Err(e) = present(&host, id, &mut notice) { eprintln!("Sora notification: {e}"); }
        }
    });
    thread::spawn(move || {
        let mut agenda: Agenda = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        loop {
            let now = now_ms();
            let before = agenda.entries.len();
            let due = agenda.collect_due(now);
            if before != agenda.entries.len() {
                if let Err(e) = save(&path, &agenda) { eprintln!("Sora calendar: {e}"); }
            }
            for notice in due { let _ = display.send(notice); }
            let wait = agenda.entries.iter().map(|e| e.at - now).min().unwrap_or(30_000).clamp(1, 30_000);
            match updates.recv_timeout(Duration::from_millis(wait as u64)) {
                Ok(mut entries) => {
                    let cutoff = now_ms() - LATE_LIMIT;
                    entries.retain(|e| e.at >= cutoff && !e.id.is_empty() && !e.title.is_empty()
                        && agenda.delivered.get(&e.id) != Some(&e.at));
                    entries.sort_by_key(|e| e.at);
                    if entries != agenda.entries {
                        agenda.entries = entries;
                        if let Err(e) = save(&path, &agenda) { eprintln!("Sora calendar: {e}"); }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    Ok(())
}

// Each capsule owns its lifetime. The next item waits for the previous exit animation.
fn present(app: &AppHandle, id: usize, notice: &mut Notice) -> Result<(), Box<dyn std::error::Error>> {
    // The native notch draws it as part of itself (one shape with the capsule); it says when the reminder has gone.
    // Dropped at once means it cannot (web notch, or hidden away): then the separate window below.
    // LocalSend's progress notice: not when its transfer already ended (a small one is over before the notice's turn).
    let busy = notice.receive == "busy";
    if busy && !crate::lan_receive::active() { return Ok(()); }
    let wait = if notice.receive.starts_with("ask:") { 75 } else { 20 };
    if let Some(gone) = crate::notch_ui::notify(notice.kind, &notice.label, &notice.title, &notice.detail, &notice.target, notice.image.take()) {
        // The progress notice stays while the transfer runs; once it ends, the next notice (what came in) takes its place.
        if busy {
            loop {
                match gone.recv_timeout(Duration::from_millis(400)) {
                    Err(mpsc::RecvTimeoutError::Timeout) if crate::lan_receive::active() => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => return Ok(()),
                }
            }
        } else {
            match gone.recv_timeout(Duration::from_secs(wait)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => {}
                _ => return Ok(()),
            }
        }
    }
    // A chat notice belongs in the notch only; Windows still keeps the original in its Notification Center.
    if notice.kind == "chat" && cfg!(win) { return Ok(()); }
    let label = format!("notice-{id}");
    notice.large = crate::desktop::preferences(app).notice_size == "large";
    let script = format!("window.soraNotice={};", serde_json::to_string(notice)?);
    let size = notice_dims(app);
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("notice.html".into()))
        .title("Sora notification").inner_size(size.0, size.1).decorations(false).shadow(false)
        .transparent(true).skip_taskbar(true).always_on_top(true).resizable(false)
        .focused(false).focusable(false).accept_first_mouse(true).visible(false).initialization_script(&script).build()?;
    let (closed, wait_rx) = mpsc::channel();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) { let _ = closed.send(()); }
    });
    if busy {
        // Its page closes itself when the transfer ends; this makes sure it does even if that word was missed.
        loop {
            match wait_rx.recv_timeout(Duration::from_millis(400)) {
                Err(mpsc::RecvTimeoutError::Timeout) if crate::lan_receive::active() => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => { if wait_rx.recv_timeout(Duration::from_secs(2)).is_err() { let _ = window.destroy(); } break; }
                _ => break,
            }
        }
    } else if wait_rx.recv_timeout(Duration::from_secs(if notice.receive.starts_with("ask:") { 75 } else { 20 })).is_err() { let _ = window.destroy(); }
    Ok(())
}

/// Where the reminder window goes, in physical px, and how the page draws in it (css px).
/// With the native notch the window hangs under the notch as it is now (capsule, mini notch or panel), `reach` above
/// the pill's usual place; the window never moves for anything but the notch itself. `attached`: the notch is the
/// floating capsule, and `cap` is where it lies in the window, so the pill can reach up over it and be one shape with it.
fn notice_place(width: i32, dpi: f64, geom: (i32, i32), capsule: Option<(i32, i32, i32, i32)>) -> ((i32, i32), serde_json::Value) {
    let (cx, bottom) = geom;
    let at = (cx - width / 2, bottom + ((NOTICE_GAP - NOTICE_TOP - NOTICE_REACH) * dpi).round() as i32);
    let cap = capsule.map(|(_, cy, cw, ch)| serde_json::json!({ "top": (cy - at.1) as f64 / dpi, "w": cw as f64 / dpi, "h": ch as f64 / dpi }));
    (at, serde_json::json!({ "native": true, "attached": capsule.is_some(), "det": NOTICE_REACH + NOTICE_TOP, "nb": NOTICE_REACH + NOTICE_TOP - NOTICE_GAP, "cap": cap }))
}

#[tauri::command]
/// Places and shows the reminder; returns how the page draws it (see `notice_place`).
/// `extra`: room (dip) a notice needs under its words (LocalSend's buttons or bar); `wider`: more width (its sender's name).
pub fn notice_ready(window: WebviewWindow, extra: Option<f64>, wider: Option<f64>) -> Result<serde_json::Value, String> {
    if !window.label().starts_with("notice-") { return Err("Not a notification window".into()); }
    let app = window.app_handle();
    let placement = app.state::<crate::WindowState>().0.lock().map_err(crate::err)?.clone();
    let monitors = window.available_monitors().map_err(crate::err)?;
    let monitor = monitors.get(placement.monitor).cloned().or(window.primary_monitor().map_err(crate::err)?)
        .ok_or("Monitor not found")?;
    let dpi = monitor.scale_factor();
    let size = notice_dims(app);
    let width = ((size.0 + wider.unwrap_or(0.0).clamp(0.0, 120.0)) * dpi).round().min(monitor.size().width as f64).max(1.0) as u32;
    let native = crate::notch_ui::enabled().then(crate::notch_ui::notch_geometry).flatten();
    let height = ((size.1 + extra.unwrap_or(0.0).clamp(0.0, 80.0) + if native.is_some() { NOTICE_REACH + 14.0 } else { 0.0 }) * dpi).round() as u32;
    let ((x, y), place) = if let Some(geom) = native {
        notice_place(width as i32, dpi, geom, crate::notch_ui::capsule_rect())
    } else {
        let hidden = app.state::<crate::desktop::Desktop>().suppressed.load(std::sync::atomic::Ordering::SeqCst);
        let notch_height = if hidden { 18.0 } else if placement.expanded || app.get_webview_window("panel").is_some_and(|w| w.is_visible().unwrap_or(false)) { 234.0 * 0.85 } else { 64.0 * 0.85 };
        ((monitor.position().x + (monitor.size().width as i32 - width as i32) / 2,
          monitor.position().y + crate::platform::top_inset(&monitor) + ((placement.top.min(80) as f64 + notch_height + 4.0) * dpi).round() as i32),
         serde_json::json!({ "native": false }))
    };
    window.set_size(PhysicalSize::new(width, height)).map_err(crate::err)?;
    window.set_position(PhysicalPosition::new(x, y)).map_err(crate::err)?;
    window.set_ignore_cursor_events(true).map_err(crate::err)?;
    no_fade(&window);
    window.show().map_err(crate::err)?;
    crate::platform::show_on_top(&window);
    if native.is_some() { follow_notch(&window, (x, y), place["attached"] == true, width as i32, dpi); }
    Ok(place)
}


/// A notice with buttons (LocalSend's Accept / Deny) takes clicks; every other one lets them through.
#[tauri::command]
pub fn notice_clickable(window: WebviewWindow) -> Result<(), String> {
    if !window.label().starts_with("notice-") { return Err("Not a notification window".into()); }
    window.set_ignore_cursor_events(false).map_err(crate::err)
}

#[tauri::command]
pub fn notice_finished(window: WebviewWindow) -> Result<(), String> {
    if !window.label().starts_with("notice-") { return Err("Not a notification window".into()); }
    window.destroy().map_err(crate::err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: &str, at: i64) -> CalendarEntry {
        CalendarEntry { id: id.into(), at, title: "Review".into(), time: "14:00".into(), kind: String::new(), note: String::new() }
    }
    #[test]
    fn alerts_once_at_deadline_across_sync_and_restart() {
        let mut agenda = Agenda { entries: vec![entry("one", 100), entry("two", 100)], ..Default::default() };
        assert!(agenda.collect_due(99).is_empty());
        assert_eq!(agenda.collect_due(100).len(), 2);
        let mut restored: Agenda = serde_json::from_slice(&serde_json::to_vec(&agenda).unwrap()).unwrap();
        restored.entries.push(entry("one", 100));
        assert!(restored.collect_due(101).is_empty());
        restored.entries.push(entry("one", 200));
        assert_eq!(restored.collect_due(200).len(), 1);
    }
    #[test]
    fn stale_events_do_not_replay_after_sleep() {
        let mut agenda=Agenda { entries:vec![entry("old",100),entry("recent",LATE_LIMIT)], ..Default::default() };
        assert_eq!(agenda.collect_due(LATE_LIMIT+101).len(),1);
        assert!(agenda.entries.is_empty());
    }
}
