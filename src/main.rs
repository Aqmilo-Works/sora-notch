#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::{Path, PathBuf}, sync::Mutex};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// The Microsoft Store edition (built with `--features store`): no AI agents, no chat apps, no self-update.
pub const STORE: bool = cfg!(feature = "store");

mod agent_extras;
mod agent_plugins;
mod agent_usage;
mod agents;
mod agy_rules;
mod meeting;
mod local_ai;
mod cloud_ai;
// Modules that are all one OS: the Windows file in win/, a macOS stand-in of the same functions in mac/ (cfg(win), build.rs).
#[cfg_attr(win, path = "win/chat_notices.rs")]
#[cfg_attr(not(win), path = "mac/chat_notices.rs")]
mod chat_notices;
mod convert;
mod board;
mod board_tools;
mod clip_kind;
mod history;
mod sealed;
mod whatsapp;
mod chat_updates;
mod telegram;
mod lan_send;
mod lan_receive;
mod quick_share;
mod share_cert;
mod web_share;
mod drag_out;
#[cfg_attr(win, path = "win/startup.rs")]
#[cfg_attr(not(win), path = "mac/startup.rs")]
mod startup;
mod uninstall;
mod desktop;
mod update;
mod focus;
#[cfg_attr(win, path = "win/draw.rs")]
#[cfg_attr(not(win), path = "mac/draw.rs")]
mod draw;
mod marks;
#[cfg_attr(win, path = "win/notch_ui.rs")]
#[cfg_attr(not(win), path = "mac/notch_ui.rs")]
mod notch_ui;
mod notices;
mod panes;
mod health;
mod sysmon;
mod steam;
mod place;
mod platform;
pub(crate) use platform::{media_read, shell_image};


type AppResult<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String { e.to_string() }

#[derive(Clone, Serialize, Deserialize)]
struct ShelfFile { path: String, name: String, size: u64, directory: bool, available: bool }
struct Shelf(Mutex<Vec<ShelfFile>>);
/// Files waiting in Send. Kept apart from the Tray (and only for this session): adding to one never shows up in the other.
struct SendQueue(Mutex<Vec<ShelfFile>>);
/// The Send view is on screen: files dropped on the panel go to Send instead of the Tray.
static SEND_DROP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[derive(Clone, Deserialize)]
struct Placement { expanded: bool, top: u32, monitor: usize }
struct WindowState(Mutex<Placement>);

/// The idle edge handle lives in a window that is exactly the compact notch window, so revealing never resizes or moves it.
pub(crate) fn place_edge(window: &tauri::WebviewWindow, p: &Placement) -> AppResult<()> { place(window, &Placement { expanded: false, ..p.clone() }) }
fn place(window: &tauri::WebviewWindow, p: &Placement) -> AppResult<()> {
    let monitors = window.available_monitors().map_err(err)?;
    let monitor = monitors.get(p.monitor).cloned().or(window.primary_monitor().map_err(err)?).ok_or("Monitor not found")?;
    let dpi = monitor.scale_factor();
    let scale: f64 = 0.85;
    // The window always has the panel's size. Resizing it makes the page draw one stale frame (the notch jumps sideways for a frame), so
    // closing only clips the window to the mini notch: everything around it stays clickable, and nothing is resized.
    let (w, h) = (744.0, 250.0);
    let fit = scale.min((monitor.size().width as f64 / dpi - 24.0) / w);
    let width = (w * fit * dpi).round() as u32;
    let height = (h * fit * dpi).round() as u32;
    let x = monitor.position().x + (monitor.size().width as i32 - width as i32) / 2;
    let y = monitor.position().y + platform::top_inset(&monitor) + (p.top.min(80) as f64 * dpi).round() as i32;
    platform::place_window(window, x, y, width, height, p.expanded, fit * dpi)?;
    window.emit("view-scale", fit).map_err(err)?;
    Ok(())
}

#[tauri::command]
fn set_panel(window: tauri::WebviewWindow, state: tauri::State<WindowState>, expanded: bool, scale: f64, top: u32, monitor: usize, home: Option<bool>) -> AppResult<()> {
    // scale and home are still sent by the web notch page; nothing reads them any more.
    let _ = (scale, home);
    let p = Placement { expanded, top, monitor };
    let app = window.app_handle().clone();
    if app.state::<desktop::Desktop>().suppressed.load(std::sync::atomic::Ordering::SeqCst) && desktop::edge_enabled(&app) {
        // Hidden with the edge handle on: keep the window as the thin strip instead of resizing it.
        *state.0.lock().map_err(err)? = p;
        if !expanded { desktop::collapsed(&app); }
        return desktop::enter_edge(&app);
    }
    desktop::leave_edge(&app);
    place(&window, &p)?;
    *state.0.lock().map_err(err)? = p;
    if !expanded { desktop::collapsed(&window.app_handle()); }
    if window.app_handle().state::<desktop::Desktop>().suppressed.load(std::sync::atomic::Ordering::SeqCst) { Ok(()) } else { window.show().map_err(err) }
}
#[tauri::command]
fn monitors(window: tauri::WebviewWindow) -> AppResult<Vec<String>> {
    Ok(window.available_monitors().map_err(err)?.iter().enumerate().map(|(i, m)| format!("Display {}  /  {} × {}", i+1, m.size().width, m.size().height)).collect())
}
#[tauri::command]
fn window_visible(window: tauri::WebviewWindow) -> bool { window.is_visible().unwrap_or(false) }
#[tauri::command]
fn quit(app: tauri::AppHandle) { focus::stop_owned(&app.state::<focus::FocusState>()); app.exit(0); }

fn shelf_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(err)?;
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.join("shelf.json"))
}
fn save_shelf(app: &tauri::AppHandle, files: &[ShelfFile]) -> AppResult<()> {
    let path = shelf_path(app)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(files).map_err(err)?).map_err(err)?;
    std::fs::rename(tmp, path).map_err(err)
}
/// Appends new paths (at most 80 entries, no duplicates); `files_only` skips folders. Returns how many were added.
fn push_files(files: &mut Vec<ShelfFile>, paths: Vec<PathBuf>, files_only: bool) -> usize {
    let before = files.len();
    for path in paths {
        if files.len() >= 80 { break; }
        if let Ok(meta) = std::fs::metadata(&path) {
            if files_only && meta.is_dir() { continue; }
            let path_string = path.to_string_lossy().to_string();
            if files.iter().any(|f| f.path == path_string) { continue; }
            files.push(ShelfFile { name: path.file_name().unwrap_or_default().to_string_lossy().to_string(), path: path_string, size: meta.len(), directory: meta.is_dir(), available: true });
        }
    }
    files.len() - before
}
/// Files received with LocalSend (lan_receive.rs) join the Tray; the panel shows them without switching tabs.
pub(crate) fn add_received(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
    if let Ok(files) = add_paths(app, paths) { let _ = app.emit("shelf-refresh", files); }
}
fn add_paths(app: &tauri::AppHandle, paths: Vec<PathBuf>) -> AppResult<Vec<ShelfFile>> {
    let state = app.state::<Shelf>();
    let mut files = state.0.lock().map_err(err)?;
    if push_files(&mut files, paths, false) > 0 { desktop::mark_tray_added(); }
    save_shelf(app, &files)?;
    Ok(files.clone())
}
#[tauri::command]
async fn shelf_list(app: tauri::AppHandle) -> AppResult<Vec<ShelfFile>> {
    tauri::async_runtime::spawn_blocking(move || {
        let shelf = app.state::<Shelf>();
        let mut snapshot = shelf.0.lock().map_err(err)?.clone();
        // Disconnected drives can take seconds. Keep the window thread and the shelf lock free.
        for file in &mut snapshot { file.available = Path::new(&file.path).exists(); }
        let mut current = shelf.0.lock().map_err(err)?;
        for file in current.iter_mut() {
            if let Some(checked) = snapshot.iter().find(|checked| checked.path == file.path) {
                file.available = checked.available;
            }
        }
        Ok(current.clone())
    }).await.map_err(err)?
}
fn send_add_paths(app: &tauri::AppHandle, paths: Vec<PathBuf>) -> AppResult<Vec<ShelfFile>> {
    let queue = app.state::<SendQueue>();
    let mut files = queue.0.lock().map_err(err)?;
    push_files(&mut files, paths, true);
    Ok(files.clone())
}
#[tauri::command]
fn send_list(state: tauri::State<SendQueue>) -> AppResult<Vec<ShelfFile>> {
    let mut files = state.0.lock().map_err(err)?;
    files.retain(|f| Path::new(&f.path).is_file());
    Ok(files.clone())
}
/// "Send to…" from the Tray: the file is copied into Send (it stays in the Tray).
#[tauri::command]
fn send_add(app: tauri::AppHandle, paths: Vec<String>) -> AppResult<Vec<ShelfFile>> { send_add_paths(&app, paths.into_iter().map(PathBuf::from).collect()) }
#[tauri::command]
fn send_remove(state: tauri::State<SendQueue>, paths: Option<Vec<String>>) -> AppResult<Vec<ShelfFile>> {
    let mut files = state.0.lock().map_err(err)?;
    if let Some(paths) = paths { files.retain(|f| !paths.contains(&f.path)); } else { files.clear(); }
    Ok(files.clone())
}
#[tauri::command]
fn send_focus(on: bool) { SEND_DROP.store(on, std::sync::atomic::Ordering::SeqCst); }
/// A file dialog of Sora's is up while this lives: the notch goes under it (notch_ui::dialog) and back over afterwards.
pub(crate) struct Aside;
impl Aside { pub(crate) fn new() -> Self { notch_ui::dialog(true); Aside } }
impl Drop for Aside { fn drop(&mut self) { notch_ui::dialog(false); } }
#[tauri::command]
async fn send_pick(app:tauri::AppHandle)->AppResult<Vec<String>>{
    tauri::async_runtime::spawn_blocking(move||{
        let aside=Aside::new();
        let paths=rfd::FileDialog::new().set_title("Add files to send").pick_files().unwrap_or_default();
        drop(aside);
        let selected:Vec<String>=paths.iter().map(|p|p.to_string_lossy().into()).collect();
        let files=send_add_paths(&app,paths)?;
        Ok(selected.into_iter().filter(|p|files.iter().any(|f|f.path==*p)).collect())
    }).await.map_err(err)?
}
#[tauri::command]
async fn shelf_pick(app: tauri::AppHandle) -> AppResult<Vec<ShelfFile>> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = { let _aside = Aside::new(); rfd::FileDialog::new().set_title("Add files to Sora").pick_files().unwrap_or_default() };
        add_paths(&app, paths)
    }).await.map_err(err)?
}
#[tauri::command]
fn shelf_remove(app: tauri::AppHandle, state: tauri::State<Shelf>, path: Option<String>) -> AppResult<Vec<ShelfFile>> {
    let mut files = state.0.lock().map_err(err)?;
    if let Some(path) = path { files.retain(|f| f.path != path); } else { files.clear(); }
    save_shelf(&app, &files)?;
    Ok(files.clone())
}
#[tauri::command]
fn shelf_open(state: tauri::State<Shelf>, path: String, reveal: bool) -> AppResult<()> {
    if !state.0.lock().map_err(err)?.iter().any(|f| f.path == path) { return Err("This file is not in the tray".into()); }
    if !Path::new(&path).exists() { return Err("The file has been moved or deleted".into()); }
    let target = if reveal { Path::new(&path).parent().ok_or("Folder not found")?.to_string_lossy().to_string() } else { path };
    platform::open_path(&target)
}

#[derive(Serialize)]
struct Media { title: String, artist: String, album: String, source: String, playing: bool, position: f64, duration: f64, playback_rate: f64, artwork: Option<String>, can_seek: bool, can_play: bool, can_next: bool, can_previous: bool }
#[tauri::command]
async fn media_status(with_art: bool) -> AppResult<Option<Media>> {
    tauri::async_runtime::spawn_blocking(move || media_read(with_art)).await.map_err(err)?
}
#[tauri::command]
async fn media_action(action: String, position: Option<f64>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || platform::media_command(&action, position)).await.map_err(err)?
}
#[tauri::command]
async fn lyrics_pick() -> AppResult<Option<String>> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = ({ let _aside = Aside::new(); rfd::FileDialog::new().add_filter("Synced lyrics", &["lrc"]).pick_file() }) else { return Ok(None); };
        if std::fs::metadata(&path).map_err(err)?.len() > 1_048_576 { return Err("LRC file is too large (maximum 1 MB)".into()); }
        std::fs::read_to_string(path).map(Some).map_err(err)
    }).await.map_err(err)?
}
fn http() -> AppResult<reqwest::Client> { reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).user_agent("Sora/1.0").build().map_err(err) }
#[tauri::command]
async fn weather(latitude: f64, longitude: f64) -> AppResult<serde_json::Value> {
    if !latitude.is_finite() || !longitude.is_finite() || latitude.abs() > 90.0 || longitude.abs() > 180.0 { return Err("Invalid coordinates".into()); }
    http()?.get("https://api.open-meteo.com/v1/forecast").query(&[("latitude",latitude.to_string()),("longitude",longitude.to_string()),("current","temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code".into()),("timezone","auto".into())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)
}
/// Prices for the Markets widget (Yahoo Finance's public chart data): price, change since the previous close, and the
/// day's points for a sparkline. Symbols are checked here; at most eight per call.
#[tauri::command]
async fn quotes(symbols: Vec<String>) -> AppResult<Vec<serde_json::Value>> {
    if symbols.len() > 8 || symbols.iter().any(|s| s.is_empty() || s.len() > 16 || !s.chars().all(|c| c.is_ascii_alphanumeric() || "^=.-".contains(c))) { return Err("Use up to 8 symbols like AAPL, BTC-USD or GC=F".into()); }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).user_agent("Mozilla/5.0 Sora").build().map_err(err)?;
    let mut out = Vec::new();
    for symbol in symbols {
        let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}", urlencoding(&symbol));
        let data: serde_json::Value = match client.get(url).query(&[("range", "1d"), ("interval", "15m")]).send().await.and_then(|r| r.error_for_status()) { Ok(r) => r.json().await.map_err(err)?, Err(_) => { out.push(serde_json::json!({ "symbol": symbol, "error": true })); continue } };
        let r = &data["chart"]["result"][0];
        let m = &r["meta"];
        let mut closes: Vec<f64> = r["indicators"]["quote"][0]["close"].as_array().map(|a| a.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default();
        let mut previous = m["chartPreviousClose"].clone();
        // A closed market (gold, stocks on a weekend or holiday) has no points today: show its last trading day instead.
        if closes.len() < 2 {
            if let Ok(r5) = client.get(format!("https://query1.finance.yahoo.com/v8/finance/chart/{}", urlencoding(&symbol))).query(&[("range", "5d"), ("interval", "15m")]).send().await.and_then(|r| r.error_for_status()) {
                if let Ok(d5) = r5.json::<serde_json::Value>().await {
                    if let Some((day, prev)) = last_session(&d5["chart"]["result"][0]) { closes = day; if let Some(p) = prev { previous = serde_json::json!(p); } }
                }
            }
        }
        out.push(serde_json::json!({ "symbol": symbol, "name": m["shortName"], "currency": m["currency"], "price": m["regularMarketPrice"], "previous": previous, "points": closes }));
    }
    Ok(out)
}
/// The closes of the last trading day in a multi-day chart, and the close of the day before it.
fn last_session(r: &serde_json::Value) -> Option<(Vec<f64>, Option<f64>)> {
    let off = r["meta"]["gmtoffset"].as_i64().unwrap_or(0);
    let times = r["timestamp"].as_array()?;
    let closes = r["indicators"]["quote"][0]["close"].as_array()?;
    let pts: Vec<(i64, f64)> = times.iter().zip(closes).filter_map(|(t, c)| Some(((t.as_i64()? + off).div_euclid(86_400), c.as_f64()?))).collect();
    let last = pts.last()?.0;
    let day: Vec<f64> = pts.iter().filter(|p| p.0 == last).map(|p| p.1).collect();
    let prev = pts.iter().rev().find(|p| p.0 < last).map(|p| p.1);
    (day.len() >= 2).then_some((day, prev))
}
/// Finds market symbols by name for the Markets widget's search (Yahoo Finance search): stocks, crypto, futures, indexes.
#[tauri::command]
async fn market_search(query: String) -> AppResult<Vec<serde_json::Value>> {
    let query = query.trim().to_string();
    if query.is_empty() || query.chars().count() > 40 { return Ok(vec![]); }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).user_agent("Mozilla/5.0 Sora").build().map_err(err)?;
    let data: serde_json::Value = client.get("https://query1.finance.yahoo.com/v1/finance/search").query(&[("q", query.as_str()), ("quotesCount", "8"), ("newsCount", "0")]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    Ok(data["quotes"].as_array().into_iter().flatten().filter_map(|q| {
        let symbol = q["symbol"].as_str()?;
        if symbol.len() > 16 || !symbol.chars().all(|c| c.is_ascii_alphanumeric() || "^=.-".contains(c)) { return None; }
        Some(serde_json::json!({ "symbol": symbol, "name": q["shortname"].as_str().or(q["longname"].as_str()).unwrap_or(symbol), "kind": q["typeDisp"], "exchange": q["exchDisp"] }))
    }).collect())
}
fn urlencoding(s: &str) -> String { s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect() }
#[tauri::command]
async fn city_search(query: String) -> AppResult<serde_json::Value> {
    let query=query.trim().to_string();
    if query.chars().count()<2 || query.chars().count()>150 {return Err("Enter a location name between 2 and 150 characters".into())}
    static CITIES: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
    let cities = CITIES.get_or_init(|| serde_json::from_str(include_str!("../data/cities.json")).expect("bundled city catalogue"));
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).collect();
    let mut matches: Vec<_> = cities.iter().filter(|c| words.iter().all(|w| c["search"].as_str().unwrap_or("").contains(w))).collect();
    matches.sort_by_key(|c| {
        let name=c["name"].as_str().unwrap_or("").to_lowercase();
        let exact=c["search"].as_str().unwrap_or("").split('|').any(|a| a == query);
        (if exact {0} else if name.starts_with(&query) {1} else {2}, std::cmp::Reverse(c["population"].as_u64().unwrap_or(0)))
    });
    let results: Vec<_> = matches.into_iter().take(100).map(|c| {let mut c=c.clone();c.as_object_mut().unwrap().remove("search");c}).collect();
    Ok(serde_json::json!({"results":results}))
}

async fn lrclib_lyrics(title: String, artist: String, album: String, duration: f64) -> AppResult<serde_json::Value> {
    let client=http()?;
    let response=client.get("https://lrclib.net/api/get").query(&[("track_name",title.clone()),("artist_name",artist.clone()),("album_name",album),("duration",duration.round().to_string())]).send().await.map_err(err)?;
    if response.status().is_success() { let data: serde_json::Value=response.json().await.map_err(err)?;if data["syncedLyrics"].is_string() || data["instrumental"].as_bool()==Some(true) {return Ok(data);} }
    let results: Vec<serde_json::Value>=client.get("https://lrclib.net/api/search").query(&[("track_name",title.clone()),("artist_name",artist.clone())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    results.into_iter().find(|v| v["syncedLyrics"].is_string() && v["trackName"].as_str().unwrap_or("").to_lowercase()==title.to_lowercase() && v["artistName"].as_str().unwrap_or("").to_lowercase().contains(&artist.to_lowercase()) && (v["duration"].as_f64().unwrap_or(0.0)-duration).abs()<5.0).ok_or("Synced lyrics are not available for this version".into())
}
fn norm(s: &str) -> String { s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect() }
fn strip_tags(mut s: &str) -> &str { s = s.trim(); while s.starts_with('[') { match s.find(']') { Some(i) => s = s[i+1..].trim_start(), None => break } } s }
/// Drops the credit block ("词：", "Chorus by:", "Title - Artist") that many providers put before the first sung line.
fn clean_lrc(raw: &str) -> String {
    let mut lead = true; let mut first = true; let mut out: Vec<&str> = Vec::new();
    for line in raw.lines() {
        let timed = line.starts_with('[') && line.chars().nth(1).map_or(false, |c| c.is_ascii_digit());
        if !timed { out.push(line); continue; }
        let body = strip_tags(line);
        if lead {
            let head: String = body.chars().take(40).collect();
            let looks_like_credit = head.contains(':') || head.contains('：') || (first && body.contains(" - "));
            first = false;
            if looks_like_credit { continue; }
            lead = false;
        }
        out.push(line);
    }
    out.join("\n")
}
fn lyric_penalty(candidate: &str, wanted: &str) -> u32 { let (c, w) = (candidate.to_lowercase(), wanted.to_lowercase()); ["live", "ver.", "version", "remix", "inst", "karaoke"].iter().filter(|k| c.contains(*k) && !w.contains(*k)).count() as u32 }
async fn qq_lyrics(title: String, artist: String, duration: f64) -> AppResult<serde_json::Value> {
    let client=http()?;
    let found: serde_json::Value=client.get("https://c.y.qq.com/soso/fcgi-bin/client_search_cp").header("Referer","https://y.qq.com/").query(&[("w",format!("{} {}",title,artist)),("format","json".into()),("n","10".into()),("p","1".into())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let (t,a)=(norm(&title),norm(&artist));
    let mut best: Option<(u32,u32,String)>=None;
    for s in found["data"]["song"]["list"].as_array().into_iter().flatten() {
        let name=s["songname"].as_str().unwrap_or("");
        let diff=if duration>0.0 { (s["interval"].as_f64().unwrap_or(0.0)-duration).abs() } else { 0.0 };
        if diff>=4.0 { continue; }
        let by=s["singer"].as_array().into_iter().flatten().filter_map(|x| x["name"].as_str()).map(norm).any(|n| !n.is_empty() && (a.contains(&n) || n.contains(&a)));
        let n=norm(name);
        if !(by && (n.contains(&t) || t.contains(&n))) { continue; }
        let Some(mid)=s["songmid"].as_str() else { continue };
        let score=(lyric_penalty(name,&title), diff.round() as u32, mid.to_string());
        if best.as_ref().map_or(true,|b| (score.0,score.1)<(b.0,b.1)) { best=Some(score); }
    }
    let mid=best.ok_or("No match")?.2;
    let data: serde_json::Value=client.get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg").header("Referer","https://y.qq.com/").query(&[("songmid",mid),("format","json".into()),("nobase64","1".into()),("g_tk","5381".into())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let lrc=clean_lrc(data["lyric"].as_str().unwrap_or(""));
    if lrc.contains("[0") || lrc.contains("[1") { Ok(serde_json::json!({"syncedLyrics":lrc})) } else { Err("No synced lyrics".into()) }
}
async fn tokio_sleep() { tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_millis(700))).await.ok(); }
async fn netease_lyrics(title: String, artist: String, duration: f64) -> AppResult<serde_json::Value> {
    let client=http()?;
    let headers=|b: reqwest::RequestBuilder| b.header("Referer","https://music.163.com/").header("Cookie","appver=2.0.2");
    let found: serde_json::Value=headers(client.get("https://music.163.com/api/search/get")).query(&[("s",format!("{} {}",title,artist)),("type","1".into()),("limit","10".into())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let (t,a)=(title.to_lowercase(),artist.to_lowercase());
    let id=found["result"]["songs"].as_array().into_iter().flatten().find(|s| {
        let name=s["name"].as_str().unwrap_or("").to_lowercase();
        let near=duration<=0.0 || (s["duration"].as_f64().unwrap_or(0.0)/1000.0-duration).abs()<5.0;
        let by=s["artists"].as_array().into_iter().flatten().filter_map(|x| x["name"].as_str()).map(str::to_lowercase).any(|n| !n.is_empty() && (a.contains(&n) || n.contains(&a)));
        near && (by || name==t) && (name.contains(&t) || t.contains(&name) || by)
    }).and_then(|s| s["id"].as_u64()).ok_or("No match")?;
    let data: serde_json::Value=headers(client.get("https://music.163.com/api/song/lyric")).query(&[("id",id.to_string()),("lv","1".into())]).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let lrc=clean_lrc(data["lrc"]["lyric"].as_str().unwrap_or(""));
    if lrc.contains("[0") || lrc.contains("[1") { Ok(serde_json::json!({"syncedLyrics":lrc})) } else { Err("No synced lyrics".into()) }
}
#[tauri::command]
async fn online_lyrics(title: String, artist: String, album: String, duration: f64) -> AppResult<serde_json::Value> {
    let short: String = { let mut s = title.clone(); for open in ['(', '[', '（'] { if let Some(i) = s.find(open) { s.truncate(i); } } if let Some(i) = s.find(" - ") { s.truncate(i); } s.trim().to_string() };
    let short = if short.is_empty() { title.clone() } else { short };
    for attempt in 0..2 {
        if let Ok(data) = lrclib_lyrics(title.clone(), artist.clone(), album.clone(), duration).await { return Ok(data); }
        if short != title { if let Ok(data) = lrclib_lyrics(short.clone(), artist.clone(), album.clone(), duration).await { return Ok(data); } }
        if attempt == 0 { tokio_sleep().await; }
    }
    if let Ok(data) = qq_lyrics(short.clone(), artist.clone(), duration).await { return Ok(data); }
    if let Ok(data) = netease_lyrics(short.clone(), artist.clone(), duration).await { return Ok(data); }
    let mut url = reqwest::Url::parse("https://api.lyrics.ovh/v1/").map_err(err)?;
    url.path_segments_mut().map_err(|_| "Invalid lyrics URL")?.pop_if_empty().push(&artist).push(&title);
    let data: serde_json::Value = http()?.get(url).send().await.map_err(err)?.error_for_status().map_err(err)?.json().await.map_err(err)?;
    let text = data["lyrics"].as_str().filter(|s| !s.trim().is_empty()).ok_or("Lyrics unavailable")?;
    Ok(serde_json::json!({"plainLyrics":text,"provider":"Lyrics.ovh"}))
}
#[tauri::command]
async fn file_thumbnail(app: tauri::AppHandle, path: String) -> AppResult<String> {
    let listed = app.state::<Shelf>().0.lock().map_err(err)?.iter().any(|f| f.path == path) || app.state::<SendQueue>().0.lock().map_err(err)?.iter().any(|f| f.path == path);
    if !listed { return Err("This file is not in the tray or Send".into()); }
    tauri::async_runtime::spawn_blocking(move || shell_image(path, false)).await.map_err(err)?
}

#[tauri::command]
async fn media_source_icon(source: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || platform::media_source_icon(source)).await.map_err(err)?
}


// Observe button-down edges without consuming clicks intended for other apps.
fn start_outside_click_monitor(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut previous = [false; 3];
        loop {
            let down = platform::mouse_down();
            let clicked = down.iter().zip(previous).any(|(now, before)| *now && !before);
            previous = down;
            let expanded = app.state::<WindowState>().0.lock().map(|p| p.expanded).unwrap_or(false);
            if clicked {
                if let Some(window) = app.get_webview_window("main") {
                    if expanded && !desktop::DRAG_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) && !desktop::tray_held() {
                        if let Some((cx, cy)) = platform::cursor_pos() {
                            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                                if cx < pos.x || cy < pos.y || cx >= pos.x + size.width as i32 || cy >= pos.y + size.height as i32 {
                                    let _ = window.emit("panel-request", false);
                                }
                            }
                        }
                    }
                }
            }
            // Outside clicks only matter while the panel is open: poll fast then, lazily otherwise (a press lasts well over 60 ms).
            std::thread::sleep(std::time::Duration::from_millis(if expanded { 10 } else { 60 }));
        }
    });
}

fn main() {
    // An AI agent's hook: forward its event to the running Sora and leave at once (agents.rs).
    if agents::relay() { return; }
    // After a self-update the previous process may still be closing: wait, then tidy up its renamed file.
    if std::env::args().any(|a| a == "--updated") { std::thread::sleep(std::time::Duration::from_millis(1800)); }
    platform::remove_old_update();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            // "sora.exe --settings music" (started while Sora runs) opens that Settings section; anything else just shows the notch.
            if let Some(i) = args.iter().position(|a| a == "--settings") {
                let section = args.get(i + 1).filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric())).cloned();
                let app = app.clone();
                tauri::async_runtime::spawn(async move { let _ = panes::open_settings(app, section).await; });
            } else { desktop::reveal(app); }
        }))
        .manage(focus::FocusState::default())
        .manage(drag_out::Outgoing::default())
        .manage(Shelf(Mutex::new(Vec::new())))
        .manage(SendQueue(Mutex::new(Vec::new())))
        .manage(WindowState(Mutex::new(Placement { expanded: false, top: 0, monitor: 0 })))
        .invoke_handler(tauri::generate_handler![notices::notify_focus_complete, notices::sync_calendar_notices, notices::notice_ready, notices::notice_finished, notices::notice_clickable, lan_receive::receive_decide, desktop::finish_panel_hide, desktop::tray_hold, desktop::timer_alert, update::app_version, update::check_update, update::apply_update, uninstall::uninstall_kind, uninstall::uninstall_sora, desktop::set_shortcut_capture, focus::sync_windows_focus, desktop::desktop_preferences, desktop::set_desktop_preferences, desktop::notch_displays, web_share::web_share_start, web_share::web_share_reset, web_share::web_share_stop, web_share::web_share_prepare, web_share::web_share_state, web_share::web_share_decide, desktop::preview_notch_size, startup::startup_enabled, startup::set_startup, lan_send::send_scan, lan_send::send_lookup, lan_send::send_files, lan_send::send_file, send_list, send_add, send_remove, send_focus, convert::tray_convert, convert::tray_conversion_options, set_panel, monitors, window_visible, quit, shelf_list, send_pick, shelf_pick, shelf_remove, shelf_open, drag_out::shelf_drag, media_status, media_action, lyrics_pick, weather, city_search, place::auto_location, place::place_lookup, place::place_search,online_lyrics, file_thumbnail, media_source_icon, panes::open_settings, panes::window_close, panes::window_minimize, notch_ui::panel_ready, notch_ui::panel_close, notch_ui::panel_view, notch_ui::panel_tall, notch_ui::timer_deadline, history::app_marks, history::app_icon, history::history_list, whatsapp::wa_state, whatsapp::wa_link, whatsapp::wa_unlink, whatsapp::wa_send, whatsapp::chat_take_reply, whatsapp::chat_typing, telegram::tg_state, telegram::tg_submit, telegram::tg_sign_out, telegram::tg_send, chat_notices::notification_access_settings, history::history_set_enabled, history::history_remove, history::history_clear, board::board_list, board::board_clip_text, board::board_put_text, board::board_set_capture, board::board_keep, board::board_remove, board::board_clear, board::board_copy, board::board_image, board::board_save, board::board_share, board::board_open_link, board::board_ocr, board::board_app_icon, board_tools::board_link_preview, board_tools::board_open_terminal, desktop::take_open_tab, health::health_get, health::health_set, health::health_drank, health::health_undrank, quotes, market_search, sysmon::system_stats, steam::steam_games, steam::steam_cover, steam::steam_launch, steam::steam_running, desktop::labs_state, agents::agent_pending, agents::agent_ack, agents::agent_decide, agents::agent_open_terminal, agents::agents_state, agents::sora_exe, agents::agent_hooks, agents::agent_hooks_preview, agents::agent_hooks_set, agents::agent_capabilities, agents::agent_pipe_status, agents::agent_open_file, agent_usage::agent_usage, agent_extras::agent_folders, agent_extras::agent_pick_folder, agent_extras::agent_new_session, agent_extras::agent_launchers, agent_extras::agent_drop, agent_extras::agent_sounds_folder, agent_extras::agent_recap, agent_extras::agent_recap_seen, agent_extras::agent_recap_set, agent_extras::agent_recap_save, meeting::meeting_scan, meeting::meeting_start, meeting::meeting_stop, meeting::meeting_pause, meeting::meeting_state, meeting::meeting_list, meeting::meeting_delete, meeting::meeting_open, meeting::meeting_audio, meeting::meeting_rename, meeting::meeting_pick_folder, meeting::meeting_consent_seen, local_ai::ai_status, local_ai::ai_download, local_ai::ai_cancel_download, local_ai::ai_remove, local_ai::ai_transcribe, local_ai::ai_summarize, local_ai::ai_cancel, local_ai::ai_transcript, local_ai::ai_transcript_save, local_ai::ai_speaker_name, local_ai::ai_export, local_ai::ai_export_summary, local_ai::ai_copy_summary, local_ai::ai_copy, cloud_ai::cloud_ai_get, cloud_ai::cloud_ai_set, cloud_ai::cloud_ai_key, cloud_ai::cloud_ai_models, cloud_ai::cloud_ai_summarize])
        .setup(|app| {
            notices::start(app.handle())?;
            board::start(app.handle());
            if !STORE { history::start(app.handle()); chat_notices::start(app.handle()); }
            health::start(app.handle())?;

            // Use one WebView for the mascot, animated shell, and live content by default.
            if !notch_ui::enabled() {
                let main = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into())).title("Sora").inner_size(336.0, 80.0)
                    .decorations(false).transparent(true).shadow(false).resizable(false).always_on_top(true).skip_taskbar(true).visible(false).build()?;
                platform::hide_frame(&main);
            }
            app.manage(lan_send::Network::new()?);
            lan_receive::start(app.handle());
            desktop::start(app.handle())?;
            agents::start(app.handle());
            if !STORE { whatsapp::start(app.handle()); telegram::start(app.handle()); chat_updates::start(app.handle()); }
            if notch_ui::enabled() { notch_ui::start(app.handle()); }
            let _ = startup::migrate_legacy(); startup::adopt_installed();
            start_outside_click_monitor(app.handle().clone());
            if let Ok(path) = shelf_path(app.handle()) { if let Ok(data) = std::fs::read(path) { if let Ok(files) = serde_json::from_slice::<Vec<ShelfFile>>(&data) { *app.state::<Shelf>().0.lock().unwrap() = files; } } }
            let menu = tauri::menu::Menu::with_items(app, &[
                &tauri::menu::MenuItem::with_id(app, "open", "Open Sora", true, None::<&str>)?,
                &tauri::menu::MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?,
                &tauri::menu::MenuItem::with_id(app, "collapse", "Collapse", cfg!(win), None::<&str>)?,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
            ])?;
            // macOS: the face's eyes as a template image, which the menu bar tints for light, dark and selected.
            let (tray_icon, template): (&[u8], bool) = if cfg!(win) { (include_bytes!("../icons/icon.png"), false) } else { (include_bytes!("../icons/tray-mac.png"), true) };
            let tray = tauri::tray::TrayIconBuilder::new().icon(tauri::image::Image::from_bytes(tray_icon)?).icon_as_template(template).menu(&menu)
                .on_menu_event(|app, event| { if event.id.as_ref() == "quit" { quit(app.clone()); } else if event.id.as_ref() == "open" { desktop::reveal(app); } else if event.id.as_ref() == "settings" { let app = app.clone(); tauri::async_runtime::spawn(async move { let _ = panes::open_settings(app, None).await; }); } else if let Some(w) = app.get_webview_window("main") { let _ = w.emit("panel-request", false); } }).build(app)?;
            tray.set_visible(platform::TRAY_VISIBLE)?;
            // macOS: Sora lives in the menu bar (its icon there), not in the Dock. tao starts every app as a regular one,
            // which undoes Info.plist's LSUIElement, so it is set here.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" && window.label() != "panel" { return; }
            if let tauri::WindowEvent::DragDrop(event) = event {
                if window.app_handle().state::<drag_out::Outgoing>().0.load(std::sync::atomic::Ordering::SeqCst) { return; }
                match event {
                    tauri::DragDropEvent::Enter { .. } => { let _ = window.emit("shelf-hover", true); },
                    tauri::DragDropEvent::Over { position } => { let _ = window.emit("drag-position", (position.x, position.y)); },
                    tauri::DragDropEvent::Leave => { let _ = window.emit("shelf-hover", false); },
                    // On the Agents tab a file goes to the session it is dropped on (the page knows which one is under it).
                    tauri::DragDropEvent::Drop { paths, position } if notch_ui::tab_visible("agents") => { let _ = window.emit("agents-drop", serde_json::json!({ "paths": paths, "x": position.x, "y": position.y })); },
                    tauri::DragDropEvent::Drop { paths, .. } if SEND_DROP.load(std::sync::atomic::Ordering::SeqCst) => { match send_add_paths(window.app_handle(), paths.clone()) { Ok(files) => { let _ = window.emit("send-updated", files); }, Err(message) => { let _ = window.emit("app-error", message); } } },
                    tauri::DragDropEvent::Drop { paths, .. } => { match add_paths(window.app_handle(), paths.clone()) { Ok(files) => { let _ = window.emit("shelf-updated", files); }, Err(message) => { let _ = window.emit("app-error", message); } } },
                    _ => {}
                }
            }
        })
        .build(tauri::generate_context!()).expect("Sora could not start")
        // Closing the last web window (the panel page, Settings) must not end Sora; only Quit does (it exits with a code).
        .run(|_, event| match event {
            tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            // Chat apps get their own Windows popups back while Sora is closed.
            tauri::RunEvent::Exit => chat_notices::restore_banners(),
            _ => {}
        });
}

#[cfg(test)]
mod widget_tests {
    /// Real network only: Yahoo answers for gold and bitcoin.
    #[test]
    #[ignore]
    fn finds_markets() { let r = tauri::async_runtime::block_on(super::market_search("gold".into())).unwrap(); println!("{r:?}"); assert!(r.iter().any(|x| x["symbol"] == "GC=F")); }
    #[test]
    #[ignore]
    fn fetches_quotes() { let q = tauri::async_runtime::block_on(super::quotes(vec!["GC=F".into(), "BTC-USD".into()])).unwrap(); println!("{q:?}"); assert!(q.iter().all(|x| x["price"].is_number())); }
}
#[cfg(test)]
mod lyric_tests {
    use super::*;
    #[test]
    fn cleans_credit_block() {
        let raw="[ti:x]
[00:00.00]Song - Artist
[00:00.11]词：A
[00:00.24]Chorus by：B
[00:12.00]First real line
[00:15.00][00:20.00]Hey: shared";
        let out=clean_lrc(raw);
        assert!(out.contains("First real line")&&out.contains("Hey: shared")&&out.contains("[ti:x]")&&!out.contains("词")&&!out.contains("Chorus")&&!out.contains("Song - Artist"));
    }
    #[test]
    #[ignore]
    fn online_chain_finds_boynextdoor() {
        let data=tauri::async_runtime::block_on(online_lyrics("IF I SAY, I LOVE YOU".into(),"BOYNEXTDOOR".into(),String::new(),161.0)).unwrap();
        let lrc=data["syncedLyrics"].as_str().unwrap_or("");
        println!("{}",lrc.lines().take(6).collect::<Vec<_>>().join("
"));
        assert!(lrc.contains("[00:"));
    }
}
