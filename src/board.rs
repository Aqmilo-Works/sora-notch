//! Board: what is copied (text, links, screenshots and pictures) is kept on a small board, newest first, on this PC only
//! (board.json and board\ in Sora's data folder). Windows' own "do not keep this" marks (password managers) are honoured.
//! Items fade out after a week unless kept.
#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use super::{err, AppResult};
use crate::clip_kind::{classify, code_language, link_url};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::{fs, io::Cursor, path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Mutex, OnceLock}, thread, time::{SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Emitter, Manager};
// What differs per OS lives in win/board.rs and mac/board.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/board.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/board.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const KEEP_DAYS: u64 = 7;
const MAX_ITEMS: usize = 60;
const MAX_TEXT: usize = 20_000;

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Item { id: String, kind: String, tag: String, text: String, w: u32, h: u32, at: u64, keep: bool,
    /// The words read from a picture (OCR): None until it was tried, empty when the picture has none.
    ocr: Option<String>,
    /// Where it was copied from: the program's own name ("Google Chrome") and its file, for its icon. Empty when unknown.
    app: String, src: String }
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store { capture: Option<bool>, items: Vec<Item> }
static STORE: Mutex<Option<Store>> = Mutex::new(None);
static DIR: OnceLock<PathBuf> = OnceLock::new();
static APP: OnceLock<AppHandle> = OnceLock::new();
/// Until when our own write to the clipboard (a click on a card) is not taken as a new copy.
static SKIP_UNTIL: AtomicU64 = AtomicU64::new(0);
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
fn with<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> { STORE.lock().ok().and_then(|mut s| s.as_mut().map(f)) }
fn save() {
    let Some(dir) = DIR.get() else { return };
    let Some(bytes) = with(|s| serde_json::to_vec(s).unwrap_or_default()) else { return };
    let tmp = dir.join("board.tmp");
    if fs::write(&tmp, bytes).is_ok() { let _ = fs::rename(&tmp, dir.join("board.json")); }
}
fn changed() { save(); if let Some(app) = APP.get() { let _ = app.emit("board-changed", ()); } }
fn files(id: &str) -> Option<(PathBuf, PathBuf)> { DIR.get().map(|d| (d.join("board").join(format!("{id}.png")), d.join("board").join(format!("{id}.jpg")))) }
fn drop_files(id: &str) { if let Some((a, b)) = files(id) { let _ = fs::remove_file(a); let _ = fs::remove_file(b); } }

/// Old items go, kept ones stay; never more than MAX_ITEMS unkept.
fn prune() {
    let cutoff = now().saturating_sub(KEEP_DAYS * 86_400);
    let gone: Vec<String> = with(|s| {
        let mut gone = Vec::new();
        let mut unkept = 0;
        s.items.retain(|i| {
            if i.keep { return true; }
            unkept += 1;
            let ok = i.at >= cutoff && unkept <= MAX_ITEMS;
            if !ok { gone.push(i.id.clone()); }
            ok
        });
        gone
    }).unwrap_or_default();
    for id in gone { drop_files(&id); }
}

pub fn start(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let _ = fs::create_dir_all(dir.join("board"));
    let store: Store = fs::read(dir.join("board.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Ok(mut s) = STORE.lock() { *s = Some(store); }
    let _ = DIR.set(dir);
    let _ = APP.set(app.clone());
    prune();
    save();
    thread::spawn(listen);
    // Pictures kept from before OCR existed are read too, one by one.
    thread::spawn(|| for id in with(|s| s.items.iter().filter(|i| i.kind == "image" && i.ocr.is_none()).map(|i| i.id.clone()).collect::<Vec<_>>()).unwrap_or_default() {
        let img = files(&id).and_then(|(png, _)| image::open(png).ok());
        set_ocr(&id, img.and_then(|i| ocr(&i)).unwrap_or_default());
    });
}

// ---- reading the clipboard


// ---- where a copy came from
fn file_of(path: &str) -> String { path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase() }
/// A program that takes screenshots: its copy shows another window, so that window is the source.
/// shortcut: a fixed list of known tools, add one when a new tool shows up as the source.
pub fn is_screenshot_tool(path: &str) -> bool {
    ["screenclippinghost.exe", "snippingtool.exe", "screensketch.exe", "sharex.exe", "greenshot.exe", "lightshot.exe", "flameshot.exe", "snagit32.exe", "snagiteditor.exe"].contains(&file_of(path).as_str())
}
/// Windows' own hosts that only pass a copy on (the clipboard service and Win+V, remote desktop, COM and Store brokers):
/// never the app it came from.
pub fn is_relay(path: &str) -> bool {
    ["svchost.exe", "textinputhost.exe", "rdpclip.exe", "dllhost.exe", "runtimebroker.exe", "applicationframehost.exe", "ctfmon.exe"].contains(&file_of(path).as_str())
}

fn new_id() -> String { uuid::Uuid::new_v4().simple().to_string() }
/// What looks like wallet keys is never kept (docs/sora/03, S-04): a 12 to 24 word recovery phrase (lowercase letters
/// only), a 64-character hex private key, or a base58 Solana secret key. Pure; tested.
pub fn looks_like_secret_key(t: &str) -> bool {
    let t = t.trim();
    let words: Vec<&str> = t.split_whitespace().collect();
    let phrase = [12, 15, 18, 21, 24].contains(&words.len()) && words.iter().all(|w| (3..=8).contains(&w.len()) && w.chars().all(|c| c.is_ascii_lowercase()));
    let hex = t.trim_start_matches("0x");
    let hex_key = hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit());
    let b58_key = (86..=90).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric() && !"0OIl".contains(c));
    phrase || hex_key || b58_key
}
fn add_text(text: String, (app, src): (String, String)) {
    let t = text.trim_end_matches(['\r', '\n']).to_string();
    if t.trim().is_empty() || t.len() > MAX_TEXT * 4 || looks_like_secret_key(&t) { return; }
    let t: String = t.chars().take(MAX_TEXT).collect();
    let tag = classify(&t);
    let kind = if tag == "link" { "link" } else { "text" };
    with(|s| {
        // The same thing copied again moves to the top instead of repeating.
        if let Some(p) = s.items.iter().position(|i| i.kind == kind && i.text == t) {
            let mut item = s.items.remove(p);
            item.at = now();
            if !src.is_empty() { item.app = app; item.src = src; }
            s.items.insert(0, item);
        } else {
            s.items.insert(0, Item { id: new_id(), kind: kind.into(), tag: tag.into(), text: t.clone(), at: now(), app, src, ..Default::default() });
        }
    });
    prune();
    changed();
}
fn add_image(img: image::DynamicImage, (app, src): (String, String)) {
    let (w, h) = (image::GenericImageView::width(&img), image::GenericImageView::height(&img));
    if w == 0 || h == 0 || w > 12_000 || h > 12_000 { return; }
    let id = new_id();
    let Some((png, jpg)) = files(&id) else { return };
    if img.save_with_format(&png, image::ImageFormat::Png).is_err() { return; }
    let thumb = img.thumbnail(360, 360).to_rgb8();
    let mut buf = Cursor::new(Vec::new());
    if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80).encode_image(&thumb).is_ok() { let _ = fs::write(&jpg, buf.into_inner()); }
    with(|s| s.items.insert(0, Item { id: id.clone(), kind: "image".into(), tag: "image".into(), w, h, at: now(), app, src, ..Default::default() }));
    prune();
    changed();
    thread::spawn(move || set_ocr(&id, ocr(&img).unwrap_or_default()));
}

// ---- the words in a picture (Windows' own OCR: on this PC, no network)
/// One word found in a picture and where it is, in the picture's own pixels.
#[derive(Serialize)]
pub struct OcrWord { text: String, x: f32, y: f32, w: f32, h: f32 }
/// One line of text found in a picture: its words, in reading order.
#[derive(Serialize)]
pub struct OcrLine { text: String, words: Vec<OcrWord> }
/// The text in a picture, line by line (for the search and the board file).
fn ocr(img: &image::DynamicImage) -> Option<String> {
    let text = ocr_lines(img)?.into_iter().map(|l| l.text).collect::<Vec<_>>().join("\n");
    Some(text.trim().chars().take(MAX_TEXT).collect())
}
/// Scan text: reads the picture as copied again and gives each line with its box, so the preview can let the user pick lines.
#[tauri::command]
pub async fn board_ocr(id: String) -> AppResult<Vec<OcrLine>> {
    let (png, _) = files(&id).ok_or("No picture.")?;
    tauri::async_runtime::spawn_blocking(move || {
        let img = image::open(png).map_err(err)?;
        ocr_lines(&img).ok_or_else(|| "Windows has no text recognition for your language. Add one in Settings > Time & language > Language.".into())
    }).await.map_err(err)?
}
fn set_ocr(id: &str, text: String) {
    if with(|s| s.items.iter_mut().find(|i| i.id == id).map(|i| i.ocr = Some(text)).is_some()) == Some(true) { changed(); }
}

// ---- listening

// ---- commands
#[derive(Serialize)]
pub struct View { #[serde(flatten)] item: Item, thumb: Option<String>, lang: String }
#[derive(Serialize)]
pub struct State { capture: bool, items: Vec<View> }

#[tauri::command]
pub fn board_list() -> State {
    let (capture, items) = with(|s| (s.capture.unwrap_or(true), s.items.clone())).unwrap_or((true, Vec::new()));
    let items = items.into_iter().map(|mut item| {
        // What an item is follows the current rules, so older copies get the newer ones (a link without https://, more languages).
        if item.kind == "text" { item.tag = classify(&item.text).into(); if item.tag == "link" { item.kind = "link".into(); } }
        let lang = if item.kind == "text" && item.tag == "code" { code_language(&item.text).unwrap_or("").to_string() } else { String::new() };
        let thumb = (item.kind == "image").then(|| files(&item.id).and_then(|(_, j)| fs::read(j).ok())).flatten()
            .map(|b| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)));
        View { item, thumb, lang }
    }).collect();
    State { capture, items }
}
/// The picture as it was copied (not the small one on the card), for the preview. Nothing for a missing or very large file.
#[tauri::command]
pub fn board_image(id: String) -> Option<String> {
    let (png, _) = files(&id)?;
    if fs::metadata(&png).ok()?.len() > 16 * 1024 * 1024 { return None; }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(fs::read(png).ok()?)))
}
/// The icon of a program items were copied from, as a picture. Asked by the program's file (any of its items may be gone by
/// now), but only for one the board names.
#[tauri::command]
pub async fn board_app_icon(src: String) -> AppResult<String> {
    if src.is_empty() || with(|s| s.items.iter().any(|i| i.src == src)) != Some(true) { return Err("Not known where this came from.".into()); }
    tauri::async_runtime::spawn_blocking(move || crate::shell_image(src, true)).await.map_err(err)?
}
/// Writes an item to `dir` as a file (a picture as PNG, anything else as .txt) and returns its path.
fn export(item: &Item, dir: &std::path::Path) -> AppResult<PathBuf> {
    fs::create_dir_all(dir).map_err(err)?;
    let what = if item.kind == "image" { "picture" } else if item.tag.is_empty() { "text" } else { item.tag.as_str() };
    if item.kind == "image" {
        let (png, _) = files(&item.id).ok_or("No picture.")?;
        let to = dir.join(format!("Sora {what} {}.png", item.id));
        fs::copy(png, &to).map_err(err)?;
        Ok(to)
    } else {
        let to = dir.join(format!("Sora {what} {}.txt", item.id));
        fs::write(&to, item.text.as_bytes()).map_err(err)?;
        Ok(to)
    }
}
fn item_of(id: &str) -> AppResult<Item> { Ok(with(|s| s.items.iter().find(|i| i.id == id).cloned()).flatten().ok_or("That item is gone.")?) }
/// Open a copied link in the default browser. Only web addresses: the "open" verb on anything else could start a program.
#[tauri::command]
pub fn board_open_link(id: String) -> AppResult<()> {
    let item = item_of(&id)?;
    let url = link_url(&item.text).ok_or("That is not a link.")?;
    if !(url.starts_with("http://") || url.starts_with("https://")) { return Err("Only web links open.".into()); }
    crate::platform::open_path(&url).map_err(|_| "No browser answered.".to_string())?;
    Ok(())
}
/// Save: the item becomes a file in Downloads.
#[tauri::command]
pub fn board_save(id: String) -> AppResult<String> {
    let item = item_of(&id)?;
    let dir = APP.get().ok_or("Not ready.")?.path().download_dir().map_err(err)?;
    Ok(export(&item, &dir)?.file_name().unwrap_or_default().to_string_lossy().into_owned())
}
/// Share: the item becomes a file the Send tab can send. It lives in Sora's own folder; the ones from earlier days are cleared first.
#[tauri::command]
pub fn board_share(id: String) -> AppResult<String> {
    let item = item_of(&id)?;
    let dir = DIR.get().ok_or("Not ready.")?.join("share");
    if let Ok(list) = fs::read_dir(&dir) {
        for f in list.flatten() { if f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 86_400) { let _ = fs::remove_file(f.path()); } }
    }
    Ok(export(&item, &dir)?.to_string_lossy().into_owned())
}
#[tauri::command]
pub fn board_set_capture(on: bool) { with(|s| s.capture = Some(on)); changed(); }
#[tauri::command]
pub fn board_keep(id: String, keep: bool) { with(|s| if let Some(i) = s.items.iter_mut().find(|i| i.id == id) { i.keep = keep; }); changed(); }
#[tauri::command]
pub fn board_remove(id: String) { with(|s| s.items.retain(|i| i.id != id)); drop_files(&id); changed(); }
#[tauri::command]
pub fn board_clear() {
    let gone: Vec<String> = with(|s| { let g = s.items.iter().filter(|i| !i.keep).map(|i| i.id.clone()).collect(); s.items.retain(|i| i.keep); g }).unwrap_or_default();
    for id in gone { drop_files(&id); }
    changed();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshot_tools_are_known_by_file_name() {
        assert!(is_screenshot_tool(r"C:\Program Files\WindowsApps\Microsoft.ScreenSketch_11.0\SnippingTool\SnippingTool.exe"));
        assert!(is_screenshot_tool("ScreenClippingHost.exe"));
        assert!(!is_screenshot_tool(r"C:\Program Files\Google\Chrome\Application\chrome.exe"));
    }
    #[test]
    fn windows_relays_are_never_the_source() {
        assert!(is_relay(r"C:\Windows\System32\svchost.exe") && is_relay("TextInputHost.exe") && is_relay(r"C:\Windows\System32\ApplicationFrameHost.exe"));
        assert!(!is_relay(r"C:\Users\x\AppData\Local\Lark\app\Lark.exe") && !is_relay(r"C:\Windows\explorer.exe"));
    }
    #[cfg(win)]
    /// On a real PC: the window in front is named, and never as a relay.
    #[test]
    fn the_app_in_front_is_named() {
        let front = unsafe { exe_of(windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()) };
        assert!(front.as_deref().is_none_or(|p| file_of(p) != "applicationframehost.exe"), "{front:?}");
    }
    #[cfg(win)]
    #[test]
    fn app_name_is_the_file_description_else_the_file_name() {
        assert_eq!(app_name(r"C:\Windows\explorer.exe"), "Windows Explorer");
        assert_eq!(app_name(r"C:\nowhere\my-tool.exe"), "My-tool");
    }
}
