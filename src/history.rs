//! Chat history: the notifications of chat apps that Sora shows (see chat_notices) are kept, split into AI apps and social
//! apps, so a message that went by can be read again. Stored on this PC only, encrypted (chat-history.bin in Sora's data
//! folder, its key sealed for this Windows account in chat-history.key: see sealed.rs); never written in plain text.
//! It can be switched off, cleared, and items go after 30 days.
#![cfg_attr(not(win), allow(dead_code))]
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::{Mutex, OnceLock}, time::{SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Emitter, Manager};

const KEEP_DAYS: u64 = 30;
const MAX_ITEMS: usize = 1500;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Entry { id: u64, cat: String, app: String, title: String, detail: String, at: u64, aumid: String,
    /// The chat a reply goes to ("wa:<chat id>"), for messages from a linked account (whatsapp.rs); empty otherwise.
    reply: String }
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store { enabled: Option<bool>, next: u64, items: Vec<Entry> }
static STORE: Mutex<Option<Store>> = Mutex::new(None);
/// The encrypted file and its data key; unset (nothing saved, the history lives in memory) when Windows cannot unlock the key.
static FILE: OnceLock<(PathBuf, Vec<u8>)> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();
const KIND: &[u8] = b"sora-chat-history";
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn with<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> { STORE.lock().ok().and_then(|mut s| s.as_mut().map(f)) }
fn save() {
    let Some((file, key)) = FILE.get() else { return };
    let Some(bytes) = with(|s| serde_json::to_vec(s).unwrap_or_default()) else { return };
    let Some(sealed) = crate::sealed::seal(key, &bytes, KIND) else { return };
    let tmp = file.with_extension("tmp");
    if fs::write(&tmp, sealed).is_ok() { let _ = fs::rename(&tmp, file); }
}
/// The history in `file` (encrypted), or else in the plain `old` file an older Sora wrote (moved over by `start`).
fn load(file: &std::path::Path, old: &std::path::Path, key: &[u8]) -> Store {
    let sealed = fs::read(file).ok().and_then(|b| crate::sealed::open(key, &b, KIND));
    sealed.or_else(|| fs::read(old).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn changed() { save(); if let Some(app) = APP.get() { let _ = app.emit("history-changed", ()); } }

pub fn start(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let (file, old) = (dir.join("chat-history.bin"), dir.join("chat-history.json"));
    let key = crate::sealed::key(&dir.join("chat-history.key")).ok();
    let mut store = key.as_ref().map(|k| load(&file, &old, k)).unwrap_or_default();
    let cutoff = now().saturating_sub(KEEP_DAYS * 86_400);
    store.items.retain(|i| i.at >= cutoff);
    if let Ok(mut s) = STORE.lock() { *s = Some(store); }
    let _ = APP.set(app.clone());
    let Some(key) = key else { return };
    let _ = FILE.set((file.clone(), key));
    save();
    // The plain file of an older Sora goes once its contents are safely in the encrypted one.
    if old.exists() && FILE.get().is_some_and(|(_, k)| fs::read(&file).ok().and_then(|b| crate::sealed::open(k, &b, KIND)).is_some()) { crate::sealed::shred(&old); }
}

/// A chat notification that was shown: its app (as the notice names it), the sender or group, and the message. `aumid` is the
/// installed app's id (empty for a website in a browser), for its own icon.
pub fn add(label: &str, title: &str, detail: &str, aumid: &str) { add_reply(label, title, detail, aumid, "") }
/// The same, for a message that can be answered from Sora: `reply` names the chat ("wa:<chat id>").
pub fn add_reply(label: &str, title: &str, detail: &str, aumid: &str, reply: &str) {
    let (app, cat) = crate::notch_ui::app_info(label);
    with(|s| {
        if s.enabled == Some(false) { return; }
        // The same line twice in a row (an app repeating its toast) is kept once.
        if s.items.first().is_some_and(|i| i.app == app && i.title == title && i.detail == detail && now().saturating_sub(i.at) < 20) { return; }
        s.next += 1;
        let id = s.next;
        s.items.insert(0, Entry { id, cat: cat.into(), app, title: title.chars().take(200).collect(), detail: detail.chars().take(2000).collect(), at: now(), aumid: aumid.chars().take(200).collect(), reply: reply.chars().take(160).collect() });
        s.items.truncate(MAX_ITEMS);
    });
    changed();
}

/// Chats kept in the history that can be answered (`prefix` "wa:"): (chat id, chat name), newest name first.
pub fn reply_chats(prefix: &str) -> Vec<(String, String)> {
    with(|s| {
        let mut seen = std::collections::HashSet::new();
        s.items.iter().filter_map(|i| i.reply.strip_prefix(prefix).map(|c| (c.to_string(), i.title.clone()))).filter(|(c, _)| seen.insert(c.clone())).collect()
    }).unwrap_or_default()
}

/// The installed app's own icon (what Windows shows for it), as a picture: only for an app that has sent a notification.
#[tauri::command]
pub async fn app_icon(aumid: String) -> Result<String, String> {
    if aumid.is_empty() || with(|s| !s.items.iter().any(|i| i.aumid == aumid)) == Some(true) { return Err("Unknown app.".into()); }
    tauri::async_runtime::spawn_blocking(move || crate::shell_image(format!("shell:AppsFolder\\{aumid}"), true)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn app_marks() -> Vec<crate::notch_ui::MarkView> { crate::notch_ui::marks() }

#[derive(Serialize)]
pub struct State { enabled: bool, items: Vec<Entry> }
#[tauri::command]
pub fn history_list() -> State {
    let (enabled, items) = with(|s| (s.enabled != Some(false), s.items.clone())).unwrap_or((true, Vec::new()));
    State { enabled, items }
}
#[tauri::command]
pub fn history_set_enabled(on: bool) { with(|s| s.enabled = Some(on)); changed(); }
#[tauri::command]
pub fn history_remove(id: u64) { with(|s| s.items.retain(|i| i.id != id)); changed(); }
/// Clears one side ("ai" or "social"), or everything when `cat` is empty.
#[tauri::command]
pub fn history_clear(cat: String) { with(|s| s.items.retain(|i| !cat.is_empty() && i.cat != cat)); changed(); }

#[cfg(test)]
mod tests {
    #[test]
    fn the_history_file_is_encrypted_and_an_old_plain_one_still_loads() {
        let dir = std::env::temp_dir().join(format!("sora-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (file, old, key) = (dir.join("chat-history.bin"), dir.join("chat-history.json"), [3u8; 32]);
        std::fs::write(&old, br#"{"enabled":true,"next":1,"items":[{"id":1,"app":"Telegram","title":"Budi","detail":"rahasia","at":5}]}"#).unwrap();
        assert_eq!(super::load(&file, &old, &key).items[0].detail, "rahasia", "an older Sora's plain file");
        let store = super::load(&file, &old, &key);
        std::fs::write(&file, crate::sealed::seal(&key, &serde_json::to_vec(&store).unwrap(), super::KIND).unwrap()).unwrap();
        let raw = std::fs::read(&file).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"rahasia") && !raw.windows(4).any(|w| w == b"Budi"), "text readable in the file");
        crate::sealed::shred(&old);
        assert!(!old.exists());
        assert_eq!(super::load(&file, &old, &key).items[0].title, "Budi", "reads back from the encrypted file");
        assert!(super::load(&file, &old, &[4u8; 32]).items.is_empty(), "another key reads nothing");
        let _ = std::fs::remove_dir_all(dir);
    }
    /// On a real PC after Sora ran: the history is only in the encrypted file, and it opens (prints how many items).
    #[test]
    #[ignore]
    fn reads_the_real_encrypted_history() {
        let dir = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("com.nook.desktop");
        assert!(!dir.join("chat-history.json").exists(), "plain file still there");
        let key = crate::sealed::key(&dir.join("chat-history.key")).unwrap();
        let store = super::load(&dir.join("chat-history.bin"), &dir.join("none"), &key);
        println!("encrypted history items: {}", store.items.len());
        assert!(!store.items.is_empty());
    }
    /// On a real PC: an installed chat app's own icon comes back as a picture.
    #[test]
    #[ignore]
    fn installed_app_has_its_own_icon() {
        let icon = crate::shell_image(r"shell:AppsFolder\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".into(), true).expect("icon");
        assert!(icon.starts_with("data:image/png;base64,") && icon.len() > 2000, "{}", icon.len());
    }
}
