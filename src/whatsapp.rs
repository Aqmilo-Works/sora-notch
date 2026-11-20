//! WhatsApp Quick Reply (Labs, off by default). A small helper of our own (`whatsapp/`, Go, built on whatsmeow) joins the
//! account as a linked device, so a message can be answered from the notch. It is NOT an official WhatsApp link: WhatsApp
//! may limit the account, and Settings says so before the QR code shows.
//! The helper is `modules\sora-wa.exe` beside Sora.exe. Sora talks to it in JSON lines over its stdin/stdout (no network port);
//! it ends with Sora (job object) and as soon as its stdin closes. Its session lives in Sora's data folder (`whatsapp\`),
//! which Unlink deletes. Message text is never written to a log.
#![cfg_attr(not(win), allow(dead_code))]
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, io::{BufRead, BufReader, Write}, path::PathBuf, process::{Child, ChildStdin, Command, Stdio}, sync::{atomic::{AtomicU64, Ordering}, mpsc, Mutex}, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
// What differs per OS lives in win/whatsapp.rs and mac/whatsapp.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/whatsapp.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/whatsapp.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const MAX_TEXT: usize = 4096;

#[derive(Clone, Serialize, Default, PartialEq, Debug)]
pub struct Status {
    /// off, missing, starting, unlinked, qr, linked, offline, error
    state: String,
    /// The pairing code as QR modules ('1' dark), row by row.
    qr: Option<Qr>,
    /// The linked phone number.
    me: String,
    error: String,
}
#[derive(Clone, Serialize, PartialEq, Debug)]
struct Qr { width: usize, modules: String }

struct Link { stdin: ChildStdin, child: Child, _job: crate::convert::engines::Job, id: u64 }
// The job is a plain kernel handle, only closed on drop: any thread may own it.
unsafe impl Send for Link {}
static LINK: Mutex<Option<Link>> = Mutex::new(None);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
/// Sends waiting for the helper's answer, by request number.
static WAIT: Mutex<Option<HashMap<u64, mpsc::Sender<Result<(), String>>>>> = Mutex::new(None);
/// Chats that wrote to this account (id -> name): only these can be answered, so the page cannot message anyone else.
static CHATS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// The chat a click on a notice asked to answer, taken by the panel page.
static PENDING: Mutex<Option<String>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);
/// Message ids already shown (a reconnect can bring a message again).
static SEEN: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// Restarts since the helper last reached "linked": the wait grows (5 s, 30 s, 2 min, then 5 min).
static RESTARTS: AtomicU64 = AtomicU64::new(0);
fn seen_before(key: String) -> bool {
    let Ok(mut s) = SEEN.lock() else { return false };
    if s.contains(&key) { return true; }
    s.push_back(key); if s.len() > 500 { s.pop_front(); }
    false
}
/// Replies sent in the last minute (chat key, when), for `pace`.
static SENT: Mutex<VecDeque<(String, std::time::Instant)>> = Mutex::new(VecDeque::new());
/// Sends at a person's pace, for WhatsApp and Telegram alike: one per chat every 1.5 s, at most 20 a minute in all.
/// Bursts are what gets accounts flagged as bots.
pub fn pace(key: &str) -> Result<(), String> {
    let now = std::time::Instant::now();
    let mut sent = SENT.lock().map_err(|e| e.to_string())?;
    while sent.front().is_some_and(|(_, at)| now.duration_since(*at) >= Duration::from_secs(60)) { sent.pop_front(); }
    if sent.iter().any(|(k, at)| k == key && now.duration_since(*at) < Duration::from_millis(1500)) { return Err("Wait a moment before sending again.".into()); }
    if sent.len() >= 20 { return Err("That is a lot of messages in a minute. Wait a little.".into()); }
    sent.push_back((key.to_string(), now));
    Ok(())
}
/// The reply box is being typed in (`on`) or left: the chat sees "typing…" only while the user really types.
#[tauri::command]
pub fn chat_typing(key: String, on: bool) {
    if let Some(chat) = key.strip_prefix("wa:") {
        if CHATS.lock().ok().is_some_and(|c| c.as_ref().is_some_and(|c| c.contains_key(chat))) && linked() { let _ = write(&json!({ "op": "typing", "chat": chat, "on": on })); }
    } else if let Some(chat) = key.strip_prefix("tg:") { crate::telegram::typing(chat, on); }
}
pub fn backoff(n: u64) -> Duration { Duration::from_secs(match n { 0 => 5, 1 => 30, 2 => 120, _ => 300 }) }

fn status() -> Status { STATUS.lock().ok().and_then(|s| s.clone()).unwrap_or_else(|| Status { state: "off".into(), ..Default::default() }) }
fn set_status(app: &AppHandle, f: impl FnOnce(&mut Status)) {
    let now = { let Ok(mut s) = STATUS.lock() else { return }; let st = s.get_or_insert_with(|| Status { state: "off".into(), ..Default::default() }); f(st); st.clone() };
    let _ = app.emit("wa-state", now);
}
pub fn linked() -> bool { status().state == "linked" }
include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// A file of the chat modules: `modules\` beside Sora.exe wins (development); otherwise the copies packed inside
/// Sora.exe, unpacked to `%APPDATA%\com.nook.desktop\modules` once per run (rewritten when the bytes differ: an update,
/// or a copy someone changed).
pub fn module(name: &str) -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join("modules").join(name);
    if beside.is_file() { return Some(beside); }
    Some(unpacked()?.join(name)).filter(|p| p.is_file())
}
fn unpacked() -> Option<&'static PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if EMBEDDED.is_empty() { return None; }
        let dir = modules_dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        // An earlier build unpacked the Telegram app key here as plain text; it is now only inside Sora.exe.
        let _ = std::fs::remove_file(dir.join("tg-api.txt"));
        for (name, bytes) in EMBEDDED {
            let p = dir.join(name);
            // Full compare, not size: a same-size update or swapped file must not survive. A failed write (file in use
            // by a running helper) keeps the old copy.
            if std::fs::read(&p).map_or(true, |old| old != *bytes) { let _ = std::fs::write(&p, bytes); }
            make_runnable(&p);
        }
        Some(dir)
    }).as_ref()
}
/// The helper to run: `modules` beside Sora.exe (development), else a newer one from the signed manifest
/// (chat_updates), else the one built into Sora.exe.
fn helper() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join("modules").join(HELPER);
    if beside.is_file() { return Some(beside); }
    updated_helper().or_else(|| module(HELPER))
}
fn folder(app: &AppHandle) -> Result<PathBuf, String> { Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("whatsapp")) }

/// Follows the Labs switch: off stops the helper; on starts it when an account is linked (it says "unlinked" and quits if not).
pub fn start(app: &AppHandle) {
    // Unpack (~54 MB compare/write) off the UI thread, before Settings first asks whether the helpers are installed.
    thread::spawn(|| { unpacked(); });
    if !crate::desktop::preferences(app).feature_whatsapp { stop(); set_status(app, |s| *s = Status { state: "off".into(), ..Default::default() }); return; }
    if crate::chat_updates::wa_paused() {
        stop();
        set_status(app, |s| *s = Status { state: "error".into(), error: "Sora paused WhatsApp for now: WhatsApp is limiting unofficial apps. It comes back by itself.".into(), ..Default::default() });
        return;
    }
    if LINK.lock().map_or(true, |l| l.is_some()) { return; }
    if let Err(e) = spawn(app, false) { set_status(app, |s| { s.state = "error".into(); s.error = e; }); }
}

/// Stops the helper and starts it again as the switch, the pause and the newest helper say (chat_updates).
pub fn restart(app: &AppHandle) { stop(); RESTARTS.store(0, Ordering::SeqCst); start(app); }

fn spawn(app: &AppHandle, pair: bool) -> Result<(), String> {
    let Some(exe) = helper() else { set_status(app, |s| *s = Status { state: "missing".into(), ..Default::default() }); return Err("The WhatsApp helper is not installed.".into()) };
    let dir = folder(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("--dir").arg(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if pair { cmd.arg("--pair"); }
    let (mut child, job) = crate::convert::engines::spawn_jobbed(&mut cmd)?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { let _ = child.kill(); return Err("The WhatsApp helper did not start.".into()) };
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    *LINK.lock().map_err(|e| e.to_string())? = Some(Link { stdin, child, _job: job, id });
    if let Ok(mut c) = CHATS.lock() { let map = c.get_or_insert_with(HashMap::new); for (chat, name) in crate::history::reply_chats("wa:") { map.entry(chat).or_insert(name); } }
    set_status(app, |s| { s.state = "starting".into(); s.qr = None; s.error.clear(); });
    let app = app.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) { handle(&app, &v); }
        }
        // The helper ended: by Unlink / switch off (LINK already taken), or on its own.
        let mine = LINK.lock().ok().map_or(false, |mut l| { let m = l.as_ref().is_some_and(|l| l.id == id); if m { if let Some(mut l) = l.take() { let _ = l.child.wait(); } } m });
        if let Ok(mut w) = WAIT.lock() { if let Some(w) = w.as_mut() { for (_, tx) in w.drain() { let _ = tx.send(Err("WhatsApp is not connected.".into())); } } }
        if mine {
            let was = status().state;
            // Ended on its own before it ever ran fine: counts toward dropping a downloaded helper (chat_updates).
            if was == "starting" { crate::chat_updates::helper_failed(); }
            set_status(&app, |s| { s.qr = None; if !matches!(s.state.as_str(), "unlinked" | "error") { s.state = "offline".into(); } });
            // It quit on its own while linked (crash, lost pipe): start it again after a wait, if the switch is still on.
            // Not after "unlinked" or "error" (logged out, outdated, opened elsewhere): a restart cannot fix those.
            if matches!(was.as_str(), "linked" | "offline" | "starting") {
                let wait = backoff(RESTARTS.fetch_add(1, Ordering::SeqCst));
                thread::sleep(wait);
                if crate::desktop::preferences(&app).feature_whatsapp && LINK.lock().map_or(false, |l| l.is_none()) { let _ = spawn(&app, false); }
            }
        }
    });
    Ok(())
}

fn qr_of(code: &str) -> Option<Qr> {
    let qr = qrcode::QrCode::with_error_correction_level(code.as_bytes(), qrcode::EcLevel::L).ok()?;
    Some(Qr { width: qr.width(), modules: qr.to_colors().iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect() })
}
fn text(v: &Value, key: &str, max: usize) -> String { v[key].as_str().unwrap_or("").chars().take(max).collect() }

/// One event from the helper (pure apart from emitting; see the tests for the message line).
fn handle(app: &AppHandle, v: &Value) {
    match v["t"].as_str().unwrap_or("") {
        "qr" => { crate::chat_updates::helper_ok(); let qr = qr_of(v["code"].as_str().unwrap_or("")); set_status(app, |s| { s.state = "qr".into(); s.qr = qr; }); }
        "state" => {
            let state = text(v, "state", 20);
            let me = text(v, "me", 32);
            let error = text(v, "error", 200);
            if state == "linked" { RESTARTS.store(0, Ordering::SeqCst); }
            if matches!(state.as_str(), "linked" | "unlinked") { crate::chat_updates::helper_ok(); }
            set_status(app, |s| { s.state = state; s.qr = None; if !me.is_empty() { s.me = me; } s.error = error; });
        }
        "msg" => {
            let Some((chat, name, detail)) = message(v) else { return };
            if seen_before(format!("{chat}{}", text(v, "id", 128))) { return; }
            if let Ok(mut c) = CHATS.lock() { c.get_or_insert_with(HashMap::new).insert(chat.clone(), name.clone()); }
            let target = format!("wa:{chat}");
            crate::history::add_reply("WhatsApp", &name, &detail, "", &target);
            // Messages that waited while Sora was off arrive at once on connect: history only, no burst of notices.
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let from = if v["group"].as_bool() == Some(true) { text(v, "sender", 60) } else { String::new() };
            if v["ts"].as_u64().map_or(true, |ts| now.saturating_sub(ts) < 120) { crate::notices::chat(app, "WhatsApp", &name, &from, &text(v, "text", 2000), &text(v, "kind", 20), &target, None); }
        }
        "sent" | "error" => {
            let Some(req) = v["req"].as_u64() else { return };
            let result = if v["t"] == "sent" { Ok(()) } else { Err(text(v, "error", 200)) };
            if let Some(tx) = WAIT.lock().ok().and_then(|mut w| w.as_mut().and_then(|w| w.remove(&req))) { let _ = tx.send(result); }
        }
        _ => {}
    }
}
/// A message line as the notch and the history show it: (chat id, chat name, text). A group's text names the sender
/// ("Sam: on my way"), the way Windows' own group notices do.
fn message(v: &Value) -> Option<(String, String, String)> {
    let chat = text(v, "chat", 120);
    if chat.is_empty() || !chat.contains('@') { return None; }
    let sender = text(v, "sender", 60);
    let name = Some(text(v, "name", 80)).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| if sender.is_empty() { chat.split('@').next().unwrap_or("").to_string() } else { sender.clone() });
    let body = text(v, "text", 2000);
    let detail = if v["group"].as_bool() == Some(true) && !sender.is_empty() { format!("{sender}: {body}") } else { body };
    Some((chat, name, detail))
}

fn write(v: &Value) -> Result<(), String> {
    let mut l = LINK.lock().map_err(|e| e.to_string())?;
    let link = l.as_mut().ok_or("WhatsApp is not connected.")?;
    let mut line = v.to_string(); line.push('\n');
    link.stdin.write_all(line.as_bytes()).and_then(|_| link.stdin.flush()).map_err(|_| "WhatsApp is not connected.".to_string())
}
/// Closes the helper's stdin (it quits by itself), then makes sure it is gone.
fn stop() {
    let Some(mut link) = LINK.lock().ok().and_then(|mut l| l.take()) else { return };
    drop(link.stdin);
    thread::spawn(move || {
        for _ in 0..30 { if matches!(link.child.try_wait(), Ok(Some(_))) { return; } thread::sleep(Duration::from_millis(100)); }
        let _ = link.child.kill(); let _ = link.child.wait();
    });
}

#[tauri::command]
pub fn wa_state() -> Status { let mut s = status(); if s.state == "off" || s.state == "offline" || s.state == "unlinked" { if helper().is_none() { s.state = "missing".into(); } } s }
/// Shows a QR code to link this PC (Settings > WhatsApp). Only when the switch is on.
#[tauri::command]
pub fn wa_link(app: AppHandle) -> Result<(), String> {
    if !crate::desktop::preferences(&app).feature_whatsapp { return Err("Turn on WhatsApp replies first.".into()); }
    if LINK.lock().map_or(false, |l| l.is_some()) {
        if matches!(status().state.as_str(), "qr" | "starting" | "linked") { return Ok(()); }
        stop(); thread::sleep(Duration::from_millis(400));
    }
    spawn(&app, true)
}
/// Logs this PC out of the account (it leaves the phone's Linked devices) and deletes the session folder.
#[tauri::command]
pub async fn wa_unlink(app: AppHandle) -> Result<(), String> {
    let running = LINK.lock().map_or(false, |l| l.is_some());
    if running && write(&json!({ "op": "logout" })).is_ok() {
        // The helper logs out, then quits; give it a moment.
        for _ in 0..50 { if LINK.lock().map_or(true, |l| l.is_none()) { break; } tokio::time::sleep(Duration::from_millis(200)).await; }
    }
    stop();
    tokio::time::sleep(Duration::from_millis(500)).await;
    if let Ok(dir) = folder(&app) { if dir.exists() { std::fs::remove_dir_all(&dir).map_err(|_| "Could not delete the WhatsApp session folder.".to_string())?; } }
    if let Ok(mut c) = CHATS.lock() { *c = None; }
    set_status(&app, |s| *s = Status { state: "unlinked".into(), ..Default::default() });
    Ok(())
}
/// Answers a chat that wrote to this account. Plain text only; the sent line goes into the history.
#[tauri::command]
pub async fn wa_send(chat: String, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() { return Err("Type a message.".into()); }
    if text.chars().count() > MAX_TEXT { return Err("The message is too long.".into()); }
    let name = CHATS.lock().ok().and_then(|c| c.as_ref().and_then(|c| c.get(&chat).cloned())).ok_or("Sora can only answer a chat that wrote to you.")?;
    if !linked() { return Err("WhatsApp is not connected.".into()); }
    pace(&format!("wa:{chat}"))?;
    let req = NEXT.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = mpsc::channel();
    WAIT.lock().map_err(|e| e.to_string())?.get_or_insert_with(HashMap::new).insert(req, tx);
    if let Err(e) = write(&json!({ "op": "send", "req": req, "chat": chat, "text": text })) { if let Ok(mut w) = WAIT.lock() { w.as_mut().map(|w| w.remove(&req)); } return Err(e); }
    let answer = tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(20))).await.map_err(|e| e.to_string())?;
    if let Ok(mut w) = WAIT.lock() { w.as_mut().map(|w| w.remove(&req)); }
    match answer {
        Ok(Ok(())) => { crate::history::add_reply("WhatsApp", &name, &format!("You: {text}"), "", &format!("wa:{chat}")); Ok(()) }
        Ok(Err(e)) => Err(if e.is_empty() { "WhatsApp did not take the message.".into() } else { e }),
        Err(_) => Err("No answer from WhatsApp. The message may not have been sent.".into()),
    }
}
/// A notice target that can be answered from Sora: a linked WhatsApp chat ("wa:") or a Telegram one ("tg:").
pub fn is_reply_target(target: &str) -> bool { target.starts_with("wa:") || target.starts_with("tg:") }
/// A notice was clicked: the panel opens the chat history with this chat's reply box (`key` "wa:<id>" or "tg:<id>"),
/// or on one side of it ("cat:ai" / "cat:social").
pub fn want_reply(app: &AppHandle, key: &str) {
    if let Ok(mut p) = PENDING.lock() { *p = Some(key.to_string()); }
    let _ = app.emit("chat-reply", key);
}
#[tauri::command]
pub fn chat_take_reply() -> Option<String> { PENDING.lock().ok().and_then(|mut p| p.take()) }

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn a_message_line_names_the_chat_and_a_group_sender() {
        let dm = super::message(&json!({ "chat": "628123@s.whatsapp.net", "name": "Budi", "sender": "Budi", "text": "See you at 7" })).unwrap();
        assert_eq!(dm, ("628123@s.whatsapp.net".into(), "Budi".into(), "See you at 7".into()));
        let group = super::message(&json!({ "chat": "1203@g.us", "name": "Family", "sender": "Sam", "group": true, "text": "on my way" })).unwrap();
        assert_eq!(group.1, "Family");
        assert_eq!(group.2, "Sam: on my way");
        let unnamed = super::message(&json!({ "chat": "628999@s.whatsapp.net", "text": "hi" })).unwrap();
        assert_eq!(unnamed.1, "628999", "no name: the number");
        assert!(super::message(&json!({ "chat": "", "text": "x" })).is_none());
        assert!(super::message(&json!({ "chat": "not-a-jid", "text": "x" })).is_none());
    }
    #[test]
    fn a_message_brought_again_is_seen_once_and_waits_grow() {
        assert!(!super::seen_before("cm1".into()));
        assert!(super::seen_before("cm1".into()));
        assert!(!super::seen_before("cm2".into()));
        let waits: Vec<u64> = (0..5).map(|n| super::backoff(n).as_secs()).collect();
        assert_eq!(waits, [5, 30, 120, 300, 300]);
    }
    #[test]
    fn replies_go_at_a_persons_pace() {
        assert!(super::pace("t:a").is_ok());
        assert!(super::pace("t:a").unwrap_err().contains("Wait a moment"), "the same chat twice at once");
        for i in 0..19 { assert!(super::pace(&format!("t:{i}")).is_ok()); }
        assert!(super::pace("t:x").unwrap_err().contains("a lot of messages"), "20 a minute in all");
    }
    #[test]
    fn the_pairing_code_becomes_a_square_qr() {
        let qr = super::qr_of("2@abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJ,abcdefghijklmnopqrstuvwxyz0123456789+/=,abcdefghijklmnopqrstuvwxyz0123456789+/=,abcdefghijklmnopqrstuvwxyz0123=").unwrap();
        assert_eq!(qr.modules.len(), qr.width * qr.width);
        assert!(qr.modules.contains('1') && qr.modules.contains('0'));
    }
    #[test]
    fn only_a_chat_that_wrote_can_be_answered() {
        let e = tauri::async_runtime::block_on(super::wa_send("62000@s.whatsapp.net".into(), "hi".into())).unwrap_err();
        assert!(e.contains("only answer"), "{e}");
        assert_eq!(tauri::async_runtime::block_on(super::wa_send("x".into(), "   ".into())).unwrap_err(), "Type a message.");
    }
}
