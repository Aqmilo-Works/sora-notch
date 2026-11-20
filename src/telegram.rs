//! Telegram Quick Reply (Settings > Chats, off by default) through TDLib, Telegram's own client library: Sora signs in as
//! the user like any Telegram app (official API, not a bot). TDLib (`tdjson.dll`) runs in a small helper of ours,
//! `modules\sora-tg.exe` beside Sora.exe (source in `telegram/`), started only when the switch is on: Sora stays small, and the helper hands
//! back the memory TDLib only needs while it syncs. The helper just carries TDLib's JSON over its stdin/stdout (no port);
//! all Telegram logic is here. It ends with Sora (job object). The app key (api_id and api_hash from my.telegram.org,
//! one for Sora, not one per user) is never in the source nor in plain text in Sora.exe: Sora carries a sealed,
//! signed key pack that only the helper opens (see `key_pack`, telegram/key.go). TDLib's database is encrypted with a
//! random key that Windows keeps for this user (DPAPI). Message text is never logged (TDLib's log: fatal errors only).
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, io::{BufRead, BufReader, Write}, path::PathBuf, process::{Child, ChildStdin, Command, Stdio}, sync::{atomic::{AtomicBool, AtomicU64, Ordering}, mpsc, Mutex}, thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
// What differs per OS lives in win/telegram.rs and mac/telegram.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/telegram.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/telegram.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const MAX_TEXT: usize = 4096;

#[derive(Clone, Serialize, Default, PartialEq, Debug)]
pub struct Status {
    /// off, missing (no helper or tdjson.dll), nokey (no app key), starting, phone, code, password, ready, closing, error
    state: String,
    /// What the code or password step should say ("Code sent to your Telegram app", the password hint).
    hint: String,
    /// The signed-in name and number.
    me: String,
    error: String,
}

use crate::whatsapp::module;
fn installed() -> bool { module(TG_HELPER).is_some() && module(TDLIB).is_some() }

struct Link { stdin: ChildStdin, child: Child, _job: crate::convert::engines::Job, id: u64 }
// The job is a plain kernel handle, only closed on drop: any thread may own it.
unsafe impl Send for Link {}
static LINK: Mutex<Option<Link>> = Mutex::new(None);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
/// Sign out was asked: when TDLib has closed, its folder is deleted.
static SIGNING_OUT: AtomicBool = AtomicBool::new(false);
/// Restarts since TDLib last reached "ready" (the wait grows, as for WhatsApp).
static RESTARTS: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);
static WAIT: Mutex<Option<HashMap<String, mpsc::Sender<Result<Value, String>>>>> = Mutex::new(None);
/// Chats as TDLib announced them: id -> (title, is a group, is a channel).
static CHAT_INFO: Mutex<Option<HashMap<i64, (String, bool, bool)>>> = Mutex::new(None);
static USERS: Mutex<Option<HashMap<i64, String>>> = Mutex::new(None);
/// Chats that wrote to this account (id -> title): only these can be answered.
static KNOWN: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static SEEN: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// Messages Sora showed, per chat, until a reply marks them viewed (nothing is marked read just by arriving).
static UNREAD: Mutex<Option<HashMap<i64, Vec<i64>>>> = Mutex::new(None);

fn status() -> Status { STATUS.lock().ok().and_then(|s| s.clone()).unwrap_or_else(|| Status { state: "off".into(), ..Default::default() }) }
fn set_status(app: &AppHandle, f: impl FnOnce(&mut Status)) {
    let now = { let Ok(mut s) = STATUS.lock() else { return }; let st = s.get_or_insert_with(|| Status { state: "off".into(), ..Default::default() }); f(st); st.clone() };
    let _ = app.emit("tg-state", now);
}
pub fn ready() -> bool { status().state == "ready" }
fn running() -> bool { LINK.lock().map_or(false, |l| l.is_some()) }
fn folder(app: &AppHandle) -> Result<PathBuf, String> { Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("telegram")) }
/// The serial of the pack the helper got last, and whether it came from the saved file.
static USED: Mutex<Option<(u64, bool)>> = Mutex::new(None);

/// A key pack's serial, when it looks like one. Only sora-tg.exe can check its signature and open it.
fn pack_serial(v: &Value) -> Option<u64> {
    (v["v"] == 1 && v["box"].is_string() && v["sig"].is_string()).then(|| v["n"].as_u64()).flatten().filter(|n| *n > 0)
}
/// The newest of these packs: (serial, pack, came from the saved file).
fn best_pack(texts: impl IntoIterator<Item = (String, bool)>) -> Option<(u64, Value, bool)> {
    texts.into_iter().filter_map(|(t, saved)| { let v: Value = serde_json::from_str(&t).ok()?; Some((pack_serial(&v)?, v, saved)) }).max_by_key(|p| p.0)
}
/// Sora's Telegram app key as a sealed pack (never the key itself): the newest of the one built in (build.rs), one
/// saved from the website, and modules\tg-key.json beside Sora.exe (development).
fn key_pack() -> Option<(u64, Value, bool)> {
    let read = |p: Option<PathBuf>| p.and_then(|p| std::fs::read_to_string(p).ok());
    let saved = read(saved_pack()).map(|t| (t, true));
    let beside = read(module("tg-key.json")).map(|t| (t, false));
    best_pack(saved.into_iter().chain(beside).chain(Some((crate::whatsapp::TG_PACK.to_string(), false))))
}

/// The helper could not open the pack (`broken`), or Telegram refused the key in it (API_ID_INVALID,
/// API_ID_PUBLISHED_FLOOD): a bad saved pack is dropped, and the signed manifest is asked for a newer one (chat_updates).
fn key_refused(app: &AppHandle, broken: bool) {
    let used = USED.lock().ok().and_then(|u| *u);
    if broken {
        if used.is_some_and(|u| u.1) {
            // The saved pack is bad: forget it; the restart takes the built-in one.
            if let Some(p) = saved_pack() { let _ = std::fs::remove_file(p); }
            set_status(app, |s| s.state = "closing".into());
        } else {
            set_status(app, |s| { s.state = "error".into(); s.error = "This copy of Sora carries a damaged Telegram key. Update Sora.".into(); });
        }
        send(&json!({ "@type": "close" }));
    }
    crate::chat_updates::check(app);
}
/// Saves a downloaded pack (write then rename, so no half file) when it is one and its serial beats `used`.
fn keep_newer(text: &str, used: u64, file: &std::path::Path) -> bool {
    let Some((n, _, _)) = best_pack([(text.to_string(), true)]) else { return false };
    if n <= used { return false; }
    let tmp = file.with_extension("tmp");
    file.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, file).is_ok()
}
/// A pack from the signed manifest (chat_updates, checked daily): saved when newer than the one in use, then Telegram
/// starts again with it. Passed on unopened; sora-tg.exe checks its own signature (a bad one is dropped by `key_refused`).
pub fn offer_pack(app: &AppHandle, text: &str) {
    let used = USED.lock().ok().and_then(|u| *u).map_or(0, |u| u.0);
    let Some(file) = saved_pack() else { return };
    if !keep_newer(text, used, &file) { return; }
    if running() { set_status(app, |s| s.state = "closing".into()); send(&json!({ "@type": "close" })); }
    else if crate::desktop::preferences(app).feature_telegram { set_status(app, |s| *s = Status { state: "off".into(), ..Default::default() }); start(app); }
}

/// One request to TDLib, through the helper.
fn send(request: &Value) {
    let Ok(mut l) = LINK.lock() else { return };
    let Some(link) = l.as_mut() else { return };
    let mut line = request.to_string(); line.push('\n');
    let _ = link.stdin.write_all(line.as_bytes()).and_then(|_| link.stdin.flush());
}
/// Sends and waits for TDLib's answer to this request.
fn ask(mut request: Value, wait: Duration) -> Result<Value, String> {
    let extra = format!("r{}", NEXT.fetch_add(1, Ordering::SeqCst));
    request["@extra"] = json!(extra);
    let (tx, rx) = mpsc::channel();
    WAIT.lock().map_err(|e| e.to_string())?.get_or_insert_with(HashMap::new).insert(extra.clone(), tx);
    send(&request);
    let answer = rx.recv_timeout(wait);
    if let Ok(mut w) = WAIT.lock() { w.as_mut().map(|w| w.remove(&extra)); }
    answer.unwrap_or_else(|_| Err("No answer from Telegram.".into()))
}

/// Follows the switch: on starts TDLib (it asks for the phone number when not signed in), off closes it.
pub fn start(app: &AppHandle) {
    if !crate::desktop::preferences(app).feature_telegram {
        // TDLib closes its database cleanly, then the helper quits by itself.
        if running() { send(&json!({ "@type": "close" })); }
        set_status(app, |s| *s = Status { state: "off".into(), ..Default::default() });
        return;
    }
    if running() { return; }
    if !installed() { set_status(app, |s| *s = Status { state: "missing".into(), ..Default::default() }); return; }
    if key_pack().is_none() { set_status(app, |s| *s = Status { state: "nokey".into(), ..Default::default() }); return; }
    if let Ok(mut k) = KNOWN.lock() { let map = k.get_or_insert_with(HashMap::new); for (chat, name) in crate::history::reply_chats("tg:") { map.entry(chat).or_insert(name); } }
    if let Err(e) = spawn(app) { set_status(app, |s| { s.state = "error".into(); s.error = e; }); }
}

fn spawn(app: &AppHandle) -> Result<(), String> {
    let exe = module(TG_HELPER).ok_or("The Telegram helper is not installed.")?;
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let (mut child, job) = crate::convert::engines::spawn_jobbed(&mut cmd)?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { let _ = child.kill(); return Err("The Telegram helper did not start.".into()) };
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    *LINK.lock().map_err(|e| e.to_string())? = Some(Link { stdin, child, _job: job, id });
    set_status(app, |s| *s = Status { state: "starting".into(), ..Default::default() });
    // Any request makes TDLib start; it then asks for its parameters.
    send(&json!({ "@type": "getOption", "name": "version" }));
    let app = app.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) { handle(&app, &v); }
        }
        // The helper ended: TDLib closed (switch off, sign out), or it quit on its own.
        let was = status().state;
        if let Ok(mut l) = LINK.lock() { if l.as_ref().is_some_and(|l| l.id == id) { if let Some(mut l) = l.take() { let _ = l.child.wait(); } } }
        if let Ok(mut w) = WAIT.lock() { if let Some(w) = w.as_mut() { for (_, tx) in w.drain() { let _ = tx.send(Err("Telegram is not connected.".into())); } } }
        if SIGNING_OUT.swap(false, Ordering::SeqCst) {
            if let Ok(dir) = folder(&app) { let _ = std::fs::remove_dir_all(dir); }
            if let Ok(mut k) = KNOWN.lock() { *k = None; }
        }
        let on = crate::desktop::preferences(&app).feature_telegram;
        if was != "error" { set_status(&app, |s| *s = Status { state: "off".into(), ..Default::default() }); }
        if !on { return; }
        // Signed out, or a crash while the switch is on: start again (after a growing wait if it was not asked for).
        if !matches!(was.as_str(), "closing" | "error") { thread::sleep(crate::whatsapp::backoff(RESTARTS.fetch_add(1, Ordering::SeqCst))); }
        else { thread::sleep(Duration::from_millis(800)); }
        if was != "error" && !running() { start(&app); }
    });
    Ok(())
}

fn handle(app: &AppHandle, v: &Value) {
    // TDLib could not be loaded: say why and stay stopped (no restart loop between Starting and Off).
    if v["@type"] == "soraMissing" {
        let why = v["error"].as_str().unwrap_or("").to_string();
        set_status(app, |s| *s = Status { state: "error".into(), error: if why.is_empty() { "Telegram's library (TDLib) could not be loaded.".into() } else { format!("Telegram's library (TDLib) could not be loaded: {why}") }, ..Default::default() });
        return;
    }
    if v["@type"] == "soraKeyInvalid" { key_refused(app, true); return; }
    // Telegram refused the app key itself (any request may answer so); the answer still goes to its asker below.
    if v["@type"] == "error" && v["message"].as_str().is_some_and(|m| m.starts_with("API_ID_")) { key_refused(app, false); }
    if let Some(extra) = v["@extra"].as_str() {
        let result = if v["@type"] == "error" { Err(friendly(v["message"].as_str().unwrap_or(""))) } else { Ok(v.clone()) };
        if let Some(tx) = WAIT.lock().ok().and_then(|mut w| w.as_mut().and_then(|w| w.remove(extra))) { let _ = tx.send(result); }
        return;
    }
    match v["@type"].as_str().unwrap_or("") {
        "updateAuthorizationState" => auth(app, &v["authorization_state"]),
        "updateNewChat" => {
            let c = &v["chat"];
            let kind = c["type"]["@type"].as_str().unwrap_or("");
            let info = (c["title"].as_str().unwrap_or("").chars().take(80).collect(), kind == "chatTypeBasicGroup" || kind == "chatTypeSupergroup", c["type"]["is_channel"].as_bool() == Some(true));
            if let (Some(id), Ok(mut m)) = (c["id"].as_i64(), CHAT_INFO.lock()) { m.get_or_insert_with(HashMap::new).insert(id, info); }
        }
        "updateChatTitle" => if let (Some(id), Ok(mut m)) = (v["chat_id"].as_i64(), CHAT_INFO.lock()) {
            if let Some(e) = m.get_or_insert_with(HashMap::new).get_mut(&id) { e.0 = v["title"].as_str().unwrap_or("").chars().take(80).collect(); }
        },
        "updateUser" => {
            let u = &v["user"];
            let name = format!("{} {}", u["first_name"].as_str().unwrap_or(""), u["last_name"].as_str().unwrap_or("")).trim().chars().take(60).collect::<String>();
            if let (Some(id), Ok(mut m)) = (u["id"].as_i64(), USERS.lock()) { m.get_or_insert_with(HashMap::new).insert(id, name); }
        }
        "updateNewMessage" => message(app, &v["message"]),
        _ => {}
    }
}

fn auth(app: &AppHandle, a: &Value) {
    let kind = a["@type"].as_str().unwrap_or("");
    match kind {
        "authorizationStateWaitTdlibParameters" => {
            let (Some((serial, pack, saved)), Ok(dir)) = (key_pack(), folder(app)) else { send(&json!({ "@type": "close" })); return };
            if let Ok(mut u) = USED.lock() { *u = Some((serial, saved)); }
            let key = match db_key(&dir) { Ok(k) => k, Err(e) => { set_status(app, |s| { s.state = "error".into(); s.error = e; }); send(&json!({ "@type": "close" })); return } };
            send(&json!({ "@type": "setTdlibParameters", "use_test_dc": false,
                "database_directory": dir.join("db"), "files_directory": dir.join("files"), "database_encryption_key": key,
                "use_file_database": false, "use_chat_info_database": true, "use_message_database": false, "use_secret_chats": false,
                // The helper swaps the sealed pack for api_id/api_hash; the key in plain text never crosses the pipe.
                "@sora_key": pack, "system_language_code": "en", "device_model": "Sora (Windows)",
                "system_version": "", "application_version": env!("CARGO_PKG_VERSION") }));
        }
        "authorizationStateWaitPhoneNumber" => set_status(app, |s| { s.state = "phone".into(); s.hint.clear(); }),
        "authorizationStateWaitCode" => {
            let hint = match a["code_info"]["type"]["@type"].as_str().unwrap_or("") {
                "authenticationCodeTypeTelegramMessage" => "Code sent to your Telegram app",
                "authenticationCodeTypeSms" | "authenticationCodeTypeSmsWord" | "authenticationCodeTypeSmsPhrase" => "Code sent by SMS",
                "authenticationCodeTypeCall" => "Code sent by a call",
                _ => "Enter the code Telegram sent you",
            };
            set_status(app, |s| { s.state = "code".into(); s.hint = hint.into(); s.error.clear(); });
        }
        "authorizationStateWaitPassword" => {
            let hint = a["password_hint"].as_str().unwrap_or("").chars().take(60).collect::<String>();
            set_status(app, |s| { s.state = "password".into(); s.hint = if hint.is_empty() { "Your two-step password".into() } else { format!("Hint: {hint}") }; s.error.clear(); });
        }
        "authorizationStateReady" => {
            RESTARTS.store(0, Ordering::SeqCst);
            set_status(app, |s| { s.state = "ready".into(); s.hint.clear(); s.error.clear(); });
            let app = app.clone();
            thread::spawn(move || if let Ok(me) = ask(json!({ "@type": "getMe" }), Duration::from_secs(15)) {
                let name = format!("{} {}", me["first_name"].as_str().unwrap_or(""), me["last_name"].as_str().unwrap_or("")).trim().to_string();
                let phone = me["phone_number"].as_str().map(|p| format!("+{p}")).unwrap_or_default();
                set_status(&app, |s| s.me = if name.is_empty() { phone } else { format!("{name} ({phone})") });
            });
        }
        "authorizationStateLoggingOut" | "authorizationStateClosing" => set_status(app, |s| s.state = "closing".into()),
        // The helper quits right after this; its end (in `spawn`) deletes the folder on sign out and starts again.
        "authorizationStateClosed" => {}
        // Email login, sign-up and "confirm on another device" are not done here.
        _ => set_status(app, |s| { s.state = "error".into(); s.error = "Finish signing in with the Telegram app on your phone first, then try again.".into(); }),
    }
}

/// A message as the notch and the history show it.
fn message(app: &AppHandle, m: &Value) {
    if m["is_outgoing"].as_bool() == Some(true) { return; }
    let (Some(chat_id), Some(id)) = (m["chat_id"].as_i64(), m["id"].as_i64()) else { return };
    let (title, group, channel) = CHAT_INFO.lock().ok().and_then(|c| c.as_ref().and_then(|c| c.get(&chat_id).cloned())).unwrap_or_default();
    if channel { return; }
    let body = content_text(&m["content"]);
    if body.is_empty() { return; }
    {
        let Ok(mut s) = SEEN.lock() else { return };
        let key = format!("{chat_id}:{id}");
        if s.contains(&key) { return; }
        s.push_back(key); if s.len() > 500 { s.pop_front(); }
    }
    if let Ok(mut u) = UNREAD.lock() { let list = u.get_or_insert_with(HashMap::new).entry(chat_id).or_default(); list.push(id); if list.len() > 50 { list.remove(0); } }
    let sender = m["sender_id"]["user_id"].as_i64().and_then(|u| USERS.lock().ok().and_then(|m| m.as_ref().and_then(|m| m.get(&u).cloned()))).unwrap_or_default();
    let name = if title.is_empty() { if sender.is_empty() { "Telegram".to_string() } else { sender.clone() } } else { title };
    let detail = if group && !sender.is_empty() { format!("{sender}: {body}") } else { body.clone() };
    let chat = chat_id.to_string();
    if let Ok(mut k) = KNOWN.lock() { k.get_or_insert_with(HashMap::new).insert(chat.clone(), name.clone()); }
    let target = format!("tg:{chat}");
    crate::history::add_reply("Telegram", &name, &detail, "", &target);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let from = if group { sender.as_str() } else { "" };
    if m["date"].as_u64().map_or(true, |d| now.saturating_sub(d) < 120) { crate::notices::chat(app, "Telegram", &name, from, &body, content_kind(&m["content"]), &target, None); }
}
/// What a message is, for a card that hides its text (notices::chat).
fn content_kind(c: &Value) -> &'static str {
    match c["@type"].as_str().unwrap_or("") {
        "messagePhoto" => "photo", "messageVideo" => "video", "messageAnimation" => "gif", "messageDocument" => "document",
        "messageVoiceNote" => "voice", "messageVideoNote" => "video_note", "messageAudio" => "audio", "messageSticker" => "sticker",
        "messageLocation" | "messageVenue" => "location", "messageContact" => "contact", "messagePoll" => "poll",
        _ => "text",
    }
}
/// The text, a caption, or what kind of message it is; "" for service messages.
fn content_text(c: &Value) -> String {
    let caption = |kind: &str| { let t = c["caption"]["text"].as_str().unwrap_or("").trim(); if t.is_empty() { kind.to_string() } else { format!("{kind}: {t}") } };
    let s = match c["@type"].as_str().unwrap_or("") {
        "messageText" => c["text"]["text"].as_str().unwrap_or("").to_string(),
        "messagePhoto" => caption("Photo"),
        "messageVideo" => caption("Video"),
        "messageAnimation" => caption("GIF"),
        "messageDocument" => { let n = c["document"]["file_name"].as_str().unwrap_or(""); if n.is_empty() { caption("Document") } else { format!("Document: {n}") } }
        "messageVoiceNote" => "Voice message".into(),
        "messageVideoNote" => "Video message".into(),
        "messageAudio" => "Audio".into(),
        "messageSticker" => format!("Sticker {}", c["sticker"]["emoji"].as_str().unwrap_or("")).trim().to_string(),
        "messageLocation" | "messageVenue" => "Location".into(),
        "messageContact" => "Contact".into(),
        "messagePoll" => { let q = c["poll"]["question"]["text"].as_str().or(c["poll"]["question"].as_str()).unwrap_or(""); if q.is_empty() { "Poll".into() } else { format!("Poll: {q}") } }
        _ => String::new(),
    };
    s.chars().take(2000).collect()
}
fn friendly(e: &str) -> String {
    match e {
        "PHONE_NUMBER_INVALID" => "That phone number is not valid. Use the full number with the country code, like +62 812 3456 7890.".into(),
        "PHONE_CODE_INVALID" => "Wrong code.".into(),
        "PHONE_CODE_EXPIRED" => "The code expired. Sign in again.".into(),
        "PASSWORD_HASH_INVALID" => "Wrong password.".into(),
        e if e.starts_with("API_ID_") => "Telegram refused Sora's app key. Sora is looking for a new one: try again later.".into(),
        e if e.starts_with("FLOOD_WAIT") || e.starts_with("Too Many Requests") => "Too many tries. Wait a while and try again.".into(),
        "" => "Telegram did not accept it.".into(),
        e => e.chars().take(160).collect(),
    }
}

/// TDLib's database key: 32 random bytes kept sealed for this Windows account with Sora's entropy (sealed.rs; a key
/// an older Sora sealed without the entropy is moved over), as base64 for TDLib.
fn db_key(dir: &std::path::Path) -> Result<String, String> {
    use base64::Engine;
    std::fs::create_dir_all(dir).map_err(|_| "Could not open the Telegram folder.".to_string())?;
    let key = crate::sealed::key(&dir.join("key.bin")).map_err(|_| "Could not unlock the Telegram session. Sign out and sign in again.".to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(key))
}

#[tauri::command]
pub fn tg_state() -> Status {
    let mut s = status();
    if s.state == "off" { if !installed() { s.state = "missing".into(); } else if key_pack().is_none() { s.state = "nokey".into(); } }
    s
}
/// The sign-in step: the phone number, the code, or the two-step password, whichever TDLib waits for.
#[tauri::command]
pub async fn tg_submit(value: String) -> Result<(), String> {
    let value = value.trim().to_string();
    if value.is_empty() || value.len() > 256 { return Err("Type it first.".into()); }
    let request = match status().state.as_str() {
        "phone" => json!({ "@type": "setAuthenticationPhoneNumber", "phone_number": value }),
        "code" => json!({ "@type": "checkAuthenticationCode", "code": value }),
        "password" => json!({ "@type": "checkAuthenticationPassword", "password": value }),
        _ => return Err("Telegram is not waiting for that.".into()),
    };
    tauri::async_runtime::spawn_blocking(move || ask(request, Duration::from_secs(30))).await.map_err(|e| e.to_string())?.map(|_| ())
}
/// Signs this PC out of Telegram (it leaves the account's Devices list) and deletes its data.
#[tauri::command]
pub fn tg_sign_out() -> Result<(), String> {
    if !running() { return Err("Telegram is not running.".into()); }
    SIGNING_OUT.store(true, Ordering::SeqCst);
    send(&json!({ "@type": "logOut" }));
    Ok(())
}
/// Answers a chat that wrote to this account. Plain text only; the sent line goes into the history.
#[tauri::command]
pub async fn tg_send(chat: String, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() { return Err("Type a message.".into()); }
    if text.chars().count() > MAX_TEXT { return Err("The message is too long.".into()); }
    let name = KNOWN.lock().ok().and_then(|k| k.as_ref().and_then(|k| k.get(&chat).cloned())).ok_or("Sora can only answer a chat that wrote to you.")?;
    let chat_id: i64 = chat.parse().map_err(|_| "Unknown chat.")?;
    if !ready() { return Err("Telegram is not connected.".into()); }
    crate::whatsapp::pace(&format!("tg:{chat}"))?;
    let unread = UNREAD.lock().ok().and_then(|mut u| u.as_mut().and_then(|u| u.remove(&chat_id))).unwrap_or_default();
    let request = json!({ "@type": "sendMessage", "chat_id": chat_id, "input_message_content": { "@type": "inputMessageText", "text": { "@type": "formattedText", "text": text } } });
    // As TDLib asks of apps (and as Telegram's own apps do): the chat is opened, its shown messages viewed, the reply
    // sent, the chat closed again.
    tauri::async_runtime::spawn_blocking(move || {
        send(&json!({ "@type": "openChat", "chat_id": chat_id }));
        if !unread.is_empty() { send(&json!({ "@type": "viewMessages", "chat_id": chat_id, "message_ids": unread, "source": null, "force_read": true })); }
        let sent = ask(request, Duration::from_secs(20));
        send(&json!({ "@type": "closeChat", "chat_id": chat_id }));
        sent
    }).await.map_err(|e| e.to_string())??;
    crate::history::add_reply("Telegram", &name, &format!("You: {text}"), "", &format!("tg:{chat}"));
    Ok(())
}

/// "typing…" in a chat that wrote, only while the user really types in Sora's reply box (whatsapp::chat_typing).
pub fn typing(chat: &str, on: bool) {
    let known = KNOWN.lock().ok().is_some_and(|k| k.as_ref().is_some_and(|k| k.contains_key(chat)));
    let Ok(chat_id) = chat.parse::<i64>() else { return };
    if !known || !ready() { return; }
    send(&json!({ "@type": "sendChatAction", "chat_id": chat_id, "action": { "@type": if on { "chatActionTyping" } else { "chatActionCancel" } } }));
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn the_newest_key_pack_wins_and_junk_is_skipped() {
        let p = |v: u64, n: u64| json!({ "v": v, "n": n, "box": "b", "sig": "s" }).to_string();
        let best = super::best_pack([(p(1, 1), false), (p(1, 3), true), ("not json".into(), true), (p(2, 9), false), (String::new(), false)]).unwrap();
        assert_eq!((best.0, best.2), (3, true));
        assert!(super::best_pack([(p(1, 0), false)]).is_none(), "serial 0 is not a pack");
        assert!(super::best_pack([(json!({ "v": 1, "n": 2, "box": "b" }).to_string(), false)]).is_none(), "unsigned");
    }
    #[test]
    fn a_newer_pack_from_the_website_is_kept_and_nothing_else_is() {
        // A local stand-in for the website, answering three requests in turn.
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/tg-key.json", server.server_addr().to_ip().unwrap());
        let pack2 = json!({ "v": 1, "n": 2, "box": "b", "sig": "s" }).to_string();
        let bodies = vec![pack2.clone(), "x".repeat(9000), "not a pack".to_string()];
        std::thread::spawn(move || for body in bodies { if let Ok(rq) = server.recv() { let _ = rq.respond(tiny_http::Response::from_string(body)); } });
        let dir = std::env::temp_dir().join(format!("sora-tg-fetch-{}", std::process::id()));
        let file = dir.join("tg-key.json");

        let got = tauri::async_runtime::block_on(crate::chat_updates::download_text(&url, 8192)).unwrap();
        assert!(!super::keep_newer(&got, 2, &file), "same serial as the one in use");
        assert!(!file.exists());
        assert!(super::keep_newer(&got, 1, &file), "higher serial");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), pack2);
        assert!(!file.with_extension("tmp").exists());
        assert!(tauri::async_runtime::block_on(crate::chat_updates::download_text(&url, 8192)).is_none(), "over 8 KB");
        let junk = tauri::async_runtime::block_on(crate::chat_updates::download_text(&url, 8192)).unwrap();
        assert!(!super::keep_newer(&junk, 0, &file), "not a pack");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), pack2, "the kept pack is untouched");
        assert!(tauri::async_runtime::block_on(crate::chat_updates::download_text("http://127.0.0.1:9/none", 8192)).is_none(), "unreachable");
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn this_build_carries_a_sealed_pack_not_the_key() {
        // Empty in editions built without a key (Public): nothing to check there.
        let pack = crate::whatsapp::TG_PACK;
        if pack.is_empty() { return; }
        let v: serde_json::Value = serde_json::from_str(pack).unwrap();
        assert!(super::pack_serial(&v).is_some());
        assert!(v.get("api_hash").is_none() && v.get("api_id").is_none());
    }
    #[test]
    fn message_content_reads_as_a_notice_line() {
        assert_eq!(super::content_text(&json!({ "@type": "messageText", "text": { "text": "halo" } })), "halo");
        assert_eq!(super::content_text(&json!({ "@type": "messagePhoto", "caption": { "text": "" } })), "Photo");
        assert_eq!(super::content_text(&json!({ "@type": "messagePhoto", "caption": { "text": "lihat" } })), "Photo: lihat");
        assert_eq!(super::content_text(&json!({ "@type": "messagePoll", "poll": { "question": { "text": "Makan?" } } })), "Poll: Makan?");
        assert_eq!(super::content_text(&json!({ "@type": "messageChatAddMembers" })), "", "service messages are not shown");
    }
    #[test]
    fn only_a_chat_that_wrote_can_be_answered() {
        let e = tauri::async_runtime::block_on(super::tg_send("777".into(), "hi".into())).unwrap_err();
        assert!(e.contains("only answer"), "{e}");
        assert_eq!(super::friendly("PHONE_CODE_INVALID"), "Wrong code.");
    }
    #[test]
    #[cfg_attr(not(win), ignore = "needs a macOS key store for sealed.rs (Keychain)")]
    fn the_database_key_is_kept_sealed_and_comes_back_the_same() {
        let dir = std::env::temp_dir().join(format!("sora-tg-key-{}", std::process::id()));
        let a = super::db_key(&dir).unwrap();
        let sealed = std::fs::read(dir.join("key.bin")).unwrap();
        assert!(sealed.len() > 32, "DPAPI blob, not the bare key");
        assert_eq!(super::db_key(&dir).unwrap(), a);
        assert_eq!(base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &a).unwrap().len(), 32);
        let _ = std::fs::remove_dir_all(dir);
    }
}
