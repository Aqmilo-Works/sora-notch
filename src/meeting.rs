//! Meeting recorder (Labs, docs/sora/02-architecture-proposal.md ADR-0002).
//!
//! One click records the meeting app's own sound: Windows process loopback (build 20348 or later) captures the audio
//! of one process tree (Zoom, Teams, Webex and the others in PROVIDERS), so music or a game playing on the same PC
//! stays out. A browser tab cannot be told apart from the browser's other tabs this way: a meeting in Chrome or Edge is
//! offered only as "all of this browser's sound", and only after you say so. Sora never falls back to recording all
//! of the PC's sound by itself: that is a separate choice you make. The microphone is off unless you switch it on,
//! and goes to a file of its own.
//!
//! Recordings stream to disk (WAV, 48 kHz, 16-bit stereo) as `<name>.wav.part` and become `<name>.wav` when they stop;
//! a `.part` left by a crash is repaired and kept the next time the Meeting tab opens. A finished recording is then
//! compressed to Opus (32 kbit/s mono, about 14 MB an hour) with the bundled FFmpeg, and the WAV removed; if that fails
//! the WAV stays. Nothing is uploaded.
#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use std::{collections::{HashMap, HashSet}, fs::{File, OpenOptions}, io::{Seek, SeekFrom, Write}, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering}, mpsc, Arc, Mutex}, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
// What differs per OS lives in win/meeting.rs and mac/meeting.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/meeting.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/meeting.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

/// Process loopback needs this Windows build (Microsoft's ApplicationLoopback sample).
pub const LOOPBACK_BUILD: u32 = 20348;
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BITS: u16 = 16;
const BLOCK: u32 = (CHANNELS as u32) * (BITS as u32) / 8;

// ---------------------------------------------------------------- meeting apps
/// (exe, name, words in a window title that mean a call is on screen, is a browser)
const PROVIDERS: [(&str, &str, &[&str], bool); 14] = [
    ("zoom.exe", "Zoom", &["zoom meeting", "zoom webinar", "meeting controls"], false),
    ("ms-teams.exe", "Microsoft Teams", &["meeting", "call with", "| call", "huddle"], false),
    ("teams.exe", "Microsoft Teams", &["meeting", "call with", "| call"], false),
    ("webexhost.exe", "Webex", &["meeting", "webex"], false),
    ("ciscocollabhost.exe", "Webex", &["meeting"], false),
    ("slack.exe", "Slack", &["huddle"], false),
    ("lark.exe", "Lark", &["meeting", "video call"], false),
    ("feishu.exe", "Lark", &["meeting", "video call"], false),
    ("chrome.exe", "Chrome", &["meet.google.com", "google meet", "meet -", "| microsoft teams", "zoom meeting", "webex"], true),
    ("msedge.exe", "Edge", &["meet.google.com", "google meet", "meet -", "| microsoft teams", "zoom meeting", "webex"], true),
    ("firefox.exe", "Firefox", &["meet.google.com", "google meet", "meet -", "| microsoft teams", "zoom meeting", "webex"], true),
    ("brave.exe", "Brave", &["meet.google.com", "google meet", "meet -", "| microsoft teams", "zoom meeting"], true),
    ("opera.exe", "Opera", &["meet.google.com", "google meet", "meet -", "| microsoft teams"], true),
    ("vivaldi.exe", "Vivaldi", &["meet.google.com", "google meet", "meet -", "| microsoft teams"], true),
];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Candidate {
    pub pid: u32, pub provider: String, pub title: String, pub exe: String,
    /// "high", "medium" or "low", from `score` (see `score`).
    pub confidence: String, pub score: u32, pub reasons: Vec<String>,
    /// True only for a desktop meeting app captured by its own process tree. A browser is never isolated per tab.
    pub isolated: bool, pub browser: bool,
}
/// What the scan saw for one app's process tree (pure input to `score`).
#[derive(Default, Clone, Debug)]
pub struct Signals { pub running: bool, pub call_window: Option<String>, pub any_window: Option<String>, pub audio_active: bool }
/// Weights: a known app running is weak (1); a window that shows a call is medium (2); the app playing sound right now
/// is medium (2). Microphone use is not counted: people stay muted for whole meetings. Pure; tested.
pub fn score(s: &Signals) -> (u32, &'static str, Vec<String>) {
    let mut n = 0; let mut why = vec![];
    if s.running { n += 1; why.push("App is running".to_string()); }
    if let Some(t) = &s.call_window { n += 2; why.push(format!("Call window: {}", short(t, 60))); }
    if s.audio_active { n += 2; why.push("Playing sound now".to_string()); }
    let c = if n >= 5 { "high" } else if n >= 3 { "medium" } else { "low" };
    (n, c, why)
}
/// The candidate to record without asking: exactly one high-confidence one (pure; tested). Otherwise the page shows a
/// short list to pick from.
pub fn auto_pick(list: &[Candidate]) -> Option<&Candidate> {
    let mut high = list.iter().filter(|c| c.confidence == "high");
    match (high.next(), high.next()) { (Some(c), None) => Some(c), _ => None }
}
fn short(s: &str, n: usize) -> String { if s.chars().count() > n { s.chars().take(n - 1).collect::<String>() + "…" } else { s.to_string() } }

pub fn loopback_supported() -> bool { windows_build() >= LOOPBACK_BUILD }

/// The top process of an app's tree: the furthest parent with the same exe name (Teams and browsers run many).
pub fn tree_root(procs: &HashMap<u32, (u32, String)>, pid: u32) -> u32 {
    let Some((_, exe)) = procs.get(&pid) else { return pid };
    let (mut root, mut seen) = (pid, HashSet::new());
    while let Some((parent, _)) = procs.get(&root) {
        if !seen.insert(root) { break; }
        match procs.get(parent) { Some((_, e)) if e == exe && *parent != root => root = *parent, _ => break }
    }
    root
}
fn in_tree(procs: &HashMap<u32, (u32, String)>, pid: u32, root: u32) -> bool {
    let (mut p, mut n) = (pid, 0);
    while n < 64 { if p == root { return true; } match procs.get(&p) { Some((parent, _)) if *parent != p => { p = *parent; n += 1; } _ => return false } }
    false
}
/// The meeting apps that look like they are in a call now, best first.
pub fn scan() -> Vec<Candidate> {
    let procs = processes();
    let titles = window_titles();
    let audio = active_audio_pids();
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (&pid, (_, exe)) in &procs {
        let Some(&(_, name, words, browser)) = PROVIDERS.iter().find(|p| p.0 == exe) else { continue };
        let root = tree_root(&procs, pid);
        if !seen.insert(root) { continue; }
        let tree: Vec<u32> = procs.keys().copied().filter(|p| in_tree(&procs, *p, root)).collect();
        let all_titles: Vec<&String> = tree.iter().filter_map(|p| titles.get(p)).flatten().collect();
        let call = all_titles.iter().find(|t| { let l = t.to_lowercase(); words.iter().any(|w| l.contains(w)) }).map(|t| t.to_string());
        let sig = Signals { running: true, any_window: all_titles.first().map(|t| t.to_string()), call_window: call.clone(), audio_active: tree.iter().any(|p| audio.contains(p)) };
        let (n, c, why) = score(&sig);
        // A browser only counts with a meeting page open; an app only once it shows a call or plays sound.
        if n < 3 || (browser && call.is_none()) { continue; }
        let title = call.or(sig.any_window).unwrap_or_else(|| name.to_string());
        out.push(Candidate { pid: root, provider: name.into(), title: short(&title, 80), exe: exe.clone(), confidence: c.into(), score: n, reasons: why, isolated: !browser, browser });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.provider.cmp(&b.provider)));
    out
}

// ---------------------------------------------------------------- WAV files
/// A 44-byte PCM WAV header for `data_bytes` of audio (pure; tested).
pub fn wav_header(data_bytes: u32, rate: u32, channels: u16, bits: u16) -> [u8; 44] {
    let block = channels as u32 * bits as u32 / 8;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF"); h[4..8].copy_from_slice(&(36u32.saturating_add(data_bytes)).to_le_bytes()); h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt "); h[16..20].copy_from_slice(&16u32.to_le_bytes()); h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes()); h[24..28].copy_from_slice(&rate.to_le_bytes()); h[28..32].copy_from_slice(&(rate * block).to_le_bytes());
    h[32..34].copy_from_slice(&(block as u16).to_le_bytes()); h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data"); h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}
/// Streams PCM to `<path>.part` and makes it `<path>` when finished; never holds the recording in memory.
pub struct WavWriter { file: File, part: PathBuf, path: PathBuf, pub bytes: u64, last_fix: Instant }
impl WavWriter {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let part = part_path(path);
        let mut file = OpenOptions::new().write(true).create_new(true).open(&part)?;
        file.write_all(&wav_header(0, RATE, CHANNELS, BITS))?;
        Ok(Self { file, part, path: path.to_path_buf(), bytes: 0, last_fix: Instant::now() })
    }
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        // A WAV file holds at most 4 GiB: past that (about 6 h at this rate) the recording stops with what it has.
        if self.bytes + data.len() as u64 > u32::MAX as u64 - 64 { return Err(std::io::Error::new(std::io::ErrorKind::Other, "The recording reached the 4 GB WAV limit.")); }
        self.file.write_all(data)?;
        self.bytes += data.len() as u64;
        // Every few seconds the header is brought up to date, so even a crash leaves a playable file.
        if self.last_fix.elapsed() > Duration::from_secs(5) { self.fix_header()?; self.last_fix = Instant::now(); }
        Ok(())
    }
    fn fix_header(&mut self) -> std::io::Result<()> {
        let h = wav_header(self.bytes as u32, RATE, CHANNELS, BITS);
        self.file.seek(SeekFrom::Start(0))?; self.file.write_all(&h)?; self.file.seek(SeekFrom::End(0))?; self.file.flush()
    }
    pub fn finish(mut self) -> std::io::Result<PathBuf> {
        self.fix_header()?;
        self.file.sync_all()?;
        drop(self.file);
        std::fs::rename(&self.part, &self.path)?;
        Ok(self.path)
    }
}
pub fn part_path(p: &Path) -> PathBuf { let mut s = p.as_os_str().to_owned(); s.push(".part"); PathBuf::from(s) }
/// Makes a `.part` left by a crash a proper WAV next to it (header from its length, no audio dropped) and returns the
/// new path. Never overwrites a file (pure file work; tested).
pub fn recover(part: &Path) -> std::io::Result<PathBuf> {
    let len = std::fs::metadata(part)?.len();
    if len < 44 { std::fs::remove_file(part)?; return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "empty")); }
    let data = ((len - 44) / BLOCK as u64 * BLOCK as u64).min(u32::MAX as u64 - 64) as u32;
    let mut f = OpenOptions::new().write(true).open(part)?;
    f.write_all(&wav_header(data, RATE, CHANNELS, BITS))?;
    f.set_len(44 + data as u64)?;
    drop(f);
    let wav = PathBuf::from(part.to_string_lossy().trim_end_matches(".part"));
    let stem = wav.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let target = unique(wav.parent().unwrap_or(Path::new(".")), &format!("{stem} (recovered)"), "wav");
    std::fs::rename(part, &target)?;
    Ok(target)
}
/// `<dir>/<stem>.<ext>`, or `<stem> (2).<ext>` and so on when taken (by a file or a .part).
pub fn unique(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    for i in 1..1000 {
        let name = if i == 1 { format!("{stem}.{ext}") } else { format!("{stem} ({i}).{ext}") };
        let p = dir.join(&name);
        if !p.exists() && !part_path(&p).exists() { return p; }
    }
    dir.join(format!("{stem} {}.{ext}", uuid::Uuid::new_v4().simple()))
}
/// A file name from a meeting title: no characters Windows refuses, no dots at the end, at most 60 characters. Pure; tested.
pub fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { ' ' } else { c }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(60).collect();
    let t = t.trim_end_matches(['.', ' ']).to_string();
    let reserved = ["con", "prn", "aux", "nul", "com1", "lpt1"];
    if t.is_empty() || reserved.contains(&t.to_lowercase().as_str()) { "Meeting".into() } else { t }
}

// ---------------------------------------------------------------- capture
// Marker only: the completion may come on any thread.

/// What is captured: one app's tree, the whole PC's output (the default device), or the default microphone.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source { Process(u32), System, Mic }

/// Shared between the capture threads and the commands.
struct Live {
    stop: AtomicBool, paused: AtomicBool, level: AtomicU32, mic_level: AtomicU32,
    /// Milliseconds recorded (pauses excluded).
    recorded_ms: AtomicU64,
    error: Mutex<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Meta {
    pub id: String, pub title: String, pub provider: String, pub started: i64, pub duration_s: f64,
    /// "app" (one process tree), "browser" (all of a browser's tabs) or "system" (everything the PC plays).
    pub mode: String, pub isolated: bool, pub mic: bool, #[serde(default)] pub recovered: bool,
    #[serde(default)] pub note: String,
}
struct Recording { meta: Meta, live: Arc<Live>, threads: Vec<thread::JoinHandle<Result<PathBuf, String>>>, started: Instant, wav: PathBuf }
static CURRENT: Mutex<Option<Recording>> = Mutex::new(None);
/// Whether a recording runs (AI jobs wait for it: recording comes first).
pub fn is_recording() -> bool { CURRENT.lock().map_or(false, |c| c.is_some()) }

fn write_zeros(wav: &mut WavWriter, mut bytes: usize, zeros: &[u8]) -> std::io::Result<()> { while bytes > 0 { let n = bytes.min(zeros.len()); wav.write(&zeros[..n])?; bytes -= n; } Ok(()) }
fn write_error(e: &std::io::Error) -> String {
    if e.raw_os_error() == Some(112) { "The disk is full. The recording stopped and keeps what was saved.".into() } else { format!("Could not write the recording: {e}") }
}

// ---------------------------------------------------------------- settings and history
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct Settings { folder: String, consent_seen: bool }
fn settings_path(app: &AppHandle) -> Option<PathBuf> { app.path().app_data_dir().ok().map(|d| d.join("meeting.json")) }
fn settings(app: &AppHandle) -> Settings { settings_path(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }
fn save_settings(app: &AppHandle, s: &Settings) { if let Some(p) = settings_path(app) { if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); } let _ = std::fs::write(p, serde_json::to_vec_pretty(s).unwrap_or_default()); } }
pub fn folder(app: &AppHandle) -> PathBuf {
    let s = settings(app);
    let f = if s.folder.is_empty() { documents().join("Sora Recordings") } else { PathBuf::from(s.folder) };
    let _ = std::fs::create_dir_all(&f);
    f
}
/// The sound file of a recording: `<stem>.opus` once compressed, else `<stem>.wav`.
pub fn audio_of(dir: &Path, stem: &str) -> Option<PathBuf> {
    ["opus", "wav"].iter().map(|x| dir.join(format!("{stem}.{x}"))).find(|p| p.is_file())
}
/// The files of one recording, from its id (the file name without extension): its sound and its notes. None for an
/// id that is not a plain name in the recordings folder (no paths from the page).
pub fn files_of(app: &AppHandle, id: &str) -> Option<(PathBuf, PathBuf)> {
    if id.is_empty() || id.contains(['\\', '/', ':']) || id.contains("..") { return None; }
    let dir = folder(app);
    audio_of(&dir, id).map(|a| (a, dir.join(format!("{id}.json"))))
}

// ---------------------------------------------------------------- compression
/// Speech at 32 kbit/s Opus, mono: about 14 MB an hour instead of about 690 MB of WAV.
const OPUS_RATE: &str = "32k";
pub fn compress_file(ffmpeg: &Path, wav: &Path) -> Result<PathBuf, String> {
    let out = wav.with_extension("opus");
    let part = wav.with_extension("opus.part");
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-protocol_whitelist", "file", "-i"]).arg(wav)
        .args(["-vn", "-ac", "1", "-c:a", "libopus", "-b:a", OPUS_RATE, "-application", "voip", "-f", "opus"]).arg(&part)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let (mut child, _job) = crate::convert::engines::spawn_jobbed(&mut cmd)?;
    below_normal(&child);
    let ok = child.wait().map_err(|e| e.to_string())?.success();
    let size = std::fs::metadata(&part).map_or(0, |m| m.len());
    if !ok || size < 200 { let _ = std::fs::remove_file(&part); return Err("Compression failed; the WAV is kept.".into()); }
    std::fs::rename(&part, &out).map_err(|e| { let _ = std::fs::remove_file(&part); e.to_string() })?;
    // A WAV still open elsewhere (a player) stays; the Opus copy is used from now on.
    let _ = std::fs::remove_file(wav);
    Ok(out)
}
/// Compresses finished recordings in the background, one at a time, then (for a recording just made) writes its
/// transcript when a transcript package is installed.
fn compress_later(app: &AppHandle, wavs: Vec<PathBuf>, transcribe: Option<String>) {
    let app = app.clone();
    thread::spawn(move || {
        static ONE: Mutex<()> = Mutex::new(());
        let _one = ONE.lock();
        // Without FFmpeg the WAV stays (and is transcribed as it is).
        let ffmpeg = app.path().app_cache_dir().ok().and_then(|d| crate::convert::engines::Engine::Ffmpeg.prepare(&d.join("converter")).ok());
        if let Some((ffmpeg, _locks)) = &ffmpeg { for w in wavs { if w.is_file() { let _ = compress_file(ffmpeg, &w); } } }
        let _ = app.emit("meeting-history", ());
        drop(_one);
        if let Some(id) = transcribe { crate::local_ai::transcribe_after_recording(&app, &id); }
    });
}
fn now_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64) }
fn feature_on(app: &AppHandle) -> Result<(), String> { if crate::desktop::preferences(app).feature_meeting { Ok(()) } else { Err("Turn on Meeting recorder in Settings > Labs first.".into()) } }

/// Repairs `.part` files a crash left behind (only while nothing records).
fn recover_all(app: &AppHandle) {
    if CURRENT.lock().map_or(true, |c| c.is_some()) { return; }
    let dir = folder(app);
    let Ok(list) = std::fs::read_dir(&dir) else { return };
    for e in list.flatten() {
        let p = e.path();
        if !p.to_string_lossy().ends_with(".wav.part") { continue; }
        let old_meta = PathBuf::from(p.to_string_lossy().trim_end_matches(".wav.part").to_string() + ".json");
        if let Ok(wav) = recover(&p) {
            compress_later(app, vec![wav.clone()], None);
            let mut meta: Meta = std::fs::read(&old_meta).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            meta.id = wav.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            meta.recovered = true;
            meta.duration_s = std::fs::metadata(&wav).map_or(0.0, |m| (m.len().saturating_sub(44)) as f64 / (RATE * BLOCK) as f64);
            if meta.title.is_empty() { meta.title = "Recovered recording".into(); }
            let _ = std::fs::write(wav.with_extension("json"), serde_json::to_vec_pretty(&meta).unwrap_or_default());
            if old_meta != wav.with_extension("json") { let _ = std::fs::remove_file(&old_meta); }
        }
    }
}

// ---------------------------------------------------------------- commands
#[derive(Serialize)]
pub struct ScanResult { candidates: Vec<Candidate>, pick: Option<u32>, build: u32, isolation: bool, recording: bool }
#[tauri::command]
pub async fn meeting_scan(app: AppHandle) -> Result<ScanResult, String> {
    feature_on(&app)?;
    let list = tauri::async_runtime::spawn_blocking(scan).await.map_err(|e| e.to_string())?;
    let build = windows_build();
    let pick = auto_pick(&list).filter(|c| c.isolated).map(|c| c.pid);
    Ok(ScanResult { pick, candidates: list, build, isolation: build >= LOOPBACK_BUILD, recording: CURRENT.lock().map_or(false, |c| c.is_some()) })
}
#[derive(Deserialize)]
pub struct StartRequest {
    /// The process (tree root) to record; 0 with mode "system".
    pid: u32,
    /// "app", "browser" or "system".
    mode: String,
    #[serde(default)] mic: bool,
    /// Required for "browser" and "system": you were told this records more than the meeting.
    #[serde(default)] broad_ok: bool,
    #[serde(default)] title: String,
    #[serde(default)] provider: String,
}
/// The meeting service a browser window shows, from its title ("Meet - Weekly sync", "… | Microsoft Teams") (pure; tested).
pub fn service_in(title: &str) -> Option<&'static str> {
    let t = title.to_lowercase();
    if t.contains("meet.google.com") || t.contains("google meet") || t.starts_with("meet -") || t.starts_with("meet –") { Some("Google Meet") }
    else if t.contains("microsoft teams") { Some("Microsoft Teams") }
    else if t.contains("zoom") { Some("Zoom") }
    else if t.contains("webex") { Some("Webex") }
    else { None }
}
/// Starts recording. Refuses (never widens on its own): process capture on an old Windows build, a browser or the
/// whole PC without `broad_ok`, a pid that is not a meeting app found by the scan.
#[tauri::command]
pub async fn meeting_start(app: AppHandle, req: StartRequest) -> Result<Value, String> {
    feature_on(&app)?;
    if CURRENT.lock().map_err(|e| e.to_string())?.is_some() { return Err("Already recording.".into()); }
    let mut service: Option<&'static str> = None;
    let source = match req.mode.as_str() {
        "app" | "browser" => {
            if !loopback_supported() { return Err(format!("Isolation unavailable: recording one app needs Windows build {LOOPBACK_BUILD} or later (this PC has {}). Sora will not record all PC audio instead unless you choose that.", windows_build())); }
            let found = tauri::async_runtime::spawn_blocking(scan).await.map_err(|e| e.to_string())?;
            let c = found.iter().find(|c| c.pid == req.pid).ok_or("That meeting is gone. Scan again.")?;
            if c.browser != (req.mode == "browser") { return Err("That app changed. Scan again.".into()); }
            if c.browser && !req.broad_ok { return Err("A browser meeting records all of the browser's tabs. Confirm first.".into()); }
            if c.browser { service = service_in(&c.title); }
            Source::Process(req.pid)
        }
        "system" => { if !req.broad_ok { return Err("Recording all PC audio needs your confirmation.".into()); } Source::System }
        _ => return Err("Unknown recording mode.".into()),
    };
    let dir = folder(&app);
    let started = now_ms();
    let stamp = crate::meeting::local_stamp();
    let title = if req.title.trim().is_empty() { "Meeting".to_string() } else { req.title.trim().to_string() };
    // A meeting in a browser is named by its service (Google Meet), so it gets that logo; else the browser's name.
    let provider = if req.mode == "system" { "All PC audio".to_string() } else { service.map_or_else(|| req.provider.clone(), str::to_string) };
    let wav = unique(&dir, &safe_name(&format!("{stamp} {provider}")), "wav");
    let id = wav.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let meta = Meta { id: id.clone(), title: short(&title, 120), provider, started, duration_s: 0.0, mode: req.mode.clone(), isolated: req.mode == "app", mic: req.mic, recovered: false, note: String::new() };
    let _ = std::fs::write(wav.with_extension("json"), serde_json::to_vec_pretty(&meta).unwrap_or_default());
    let live = Arc::new(Live { stop: AtomicBool::new(false), paused: AtomicBool::new(false), level: AtomicU32::new(0), mic_level: AtomicU32::new(0), recorded_ms: AtomicU64::new(0), error: Mutex::new(String::new()) });
    let mut threads = vec![];
    let (tx, rx) = mpsc::channel();
    { let (live, path, tx) = (live.clone(), wav.clone(), tx.clone()); threads.push(thread::spawn(move || run_capture(source, path, live, tx, false))); }
    let first = rx.recv_timeout(Duration::from_secs(8)).unwrap_or(Err("The audio engine did not start.".into()));
    if let Err(e) = first {
        live.stop.store(true, Ordering::SeqCst);
        for t in threads { let _ = t.join(); }
        let _ = std::fs::remove_file(&wav); let _ = std::fs::remove_file(part_path(&wav)); let _ = std::fs::remove_file(wav.with_extension("json"));
        return Err(e);
    }
    if req.mic {
        let mic_path = unique(&dir, &format!("{id} (mic)"), "wav");
        let (live2, tx2) = (live.clone(), tx.clone());
        threads.push(thread::spawn(move || run_capture(Source::Mic, mic_path, live2, tx2, true)));
        if let Ok(Err(e)) = rx.recv_timeout(Duration::from_secs(8)) { if let Ok(mut m) = live.error.lock() { *m = format!("Recording without the microphone: {e}"); } }
    }
    *CURRENT.lock().map_err(|e| e.to_string())? = Some(Recording { meta: meta.clone(), live: live.clone(), threads, started: Instant::now(), wav });
    crate::notch_ui::set_recording(true);
    crate::notices::meeting(&app, "Recording", &format!("{} · {}", meta.provider, if meta.isolated { "only this app's sound" } else { "includes other sound" }));
    watch(app.clone(), live, if let Source::Process(pid) = source { Some(pid) } else { None });
    Ok(state_json())
}
/// Quiet this long with the call window gone means the meeting ended (a free Zoom or Webex call cut at 40 minutes
/// leaves the app open). Long enough for a quiet moment while someone shares a screen.
pub const ENDED_QUIET: Duration = Duration::from_secs(30);
/// Below this peak (16-bit) the meeting is silent.
const QUIET_PEAK: u32 = 300;
/// Whether the recording should stop because the meeting is over (pure; tested): the app showed a call window when it
/// started, shows none now, and has been silent for ENDED_QUIET. Without a call window seen at the start (a call found
/// only by its sound), only the app closing ends it.
pub fn meeting_ended(seen_call: bool, call_now: bool, quiet_for: Duration) -> bool { seen_call && !call_now && quiet_for >= ENDED_QUIET }
/// Whether the app tree rooted at `pid` shows a call window now; None when it is not a known meeting app.
fn call_window_open(pid: u32) -> Option<bool> {
    let procs = processes();
    let exe = procs.get(&pid)?.1.clone();
    let words = PROVIDERS.iter().find(|p| p.0 == exe)?.2;
    let titles = window_titles();
    Some(procs.keys().filter(|p| in_tree(&procs, **p, pid)).filter_map(|p| titles.get(p)).flatten().any(|t| { let l = t.to_lowercase(); words.iter().any(|w| l.contains(w)) }))
}
/// Ends the recording the moment a capture thread stops by itself (app closed, disk full, device lost), or when the
/// meeting it records has ended (see `meeting_ended`).
fn watch(app: AppHandle, live: Arc<Live>, target: Option<u32>) {
    thread::spawn(move || {
        let seen_call = target.and_then(call_window_open).unwrap_or(false);
        let (mut last_sound, mut last_check) = (Instant::now(), Instant::now());
        loop {
            thread::sleep(Duration::from_millis(500));
            let still = CURRENT.lock().map_or(false, |c| c.as_ref().is_some_and(|r| Arc::ptr_eq(&r.live, &live)));
            if !still { return; }
            let _ = app.emit("meeting-state", state_json());
            if live.stop.load(Ordering::SeqCst) { let _ = finish(&app); return; }
            if live.level.load(Ordering::SeqCst) > QUIET_PEAK { last_sound = Instant::now(); }
            if let (Some(pid), true) = (target, seen_call && last_check.elapsed() >= Duration::from_secs(2)) {
                last_check = Instant::now();
                let call_now = call_window_open(pid).unwrap_or(false);
                if call_now { last_sound = last_sound.max(Instant::now() - Duration::from_secs(1)); }
                if meeting_ended(seen_call, call_now, last_sound.elapsed()) {
                    if let Ok(mut e) = live.error.lock() { if e.is_empty() { *e = "The meeting ended, so the recording stopped.".into(); } }
                    let _ = finish(&app);
                    return;
                }
            }
        }
    });
}
fn finish(app: &AppHandle) -> Result<Value, String> {
    let Some(rec) = CURRENT.lock().map_err(|e| e.to_string())?.take() else { return Err("Not recording.".into()) };
    rec.live.stop.store(true, Ordering::SeqCst);
    let mut errors = vec![];
    let mut saved = vec![];
    for t in rec.threads { match t.join() { Ok(Ok(p)) => saved.push(p), Ok(Err(e)) => errors.push(e), Err(_) => errors.push("A capture thread failed.".into()) } }
    crate::notch_ui::set_recording(false);
    let mut meta = rec.meta;
    meta.duration_s = rec.live.recorded_ms.load(Ordering::SeqCst) as f64 / 1000.0;
    let reason = rec.live.error.lock().map(|e| e.clone()).unwrap_or_default();
    if !reason.is_empty() { meta.note = reason.clone(); }
    let _ = std::fs::write(rec.wav.with_extension("json"), serde_json::to_vec_pretty(&meta).unwrap_or_default());
    let _ = rec.started;
    crate::notices::meeting(app, if reason.is_empty() { "Recording saved" } else { "Recording stopped" }, &if reason.is_empty() { format!("{} · {}", meta.provider, clock(meta.duration_s)) } else { short(&reason, 80) });
    let _ = app.emit("meeting-state", state_json());
    let _ = app.emit("meeting-history", ());
    let id = rec.wav.file_stem().map(|s| s.to_string_lossy().to_string());
    compress_later(app, saved, id);
    Ok(json!({ "saved": meta, "errors": errors }))
}
fn clock(s: f64) -> String { let s = s.round() as u64; if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) } }
#[tauri::command]
pub fn meeting_stop(app: AppHandle) -> Result<Value, String> { finish(&app) }
#[tauri::command]
pub fn meeting_pause(paused: bool) -> Result<Value, String> {
    let c = CURRENT.lock().map_err(|e| e.to_string())?;
    let r = c.as_ref().ok_or("Not recording.")?;
    r.live.paused.store(paused, Ordering::SeqCst);
    drop(c);
    Ok(state_json())
}
fn state_json() -> Value {
    let c = CURRENT.lock().ok();
    match c.as_ref().and_then(|c| c.as_ref()) {
        None => json!({ "state": "idle" }),
        Some(r) => json!({
            "state": if r.live.paused.load(Ordering::SeqCst) { "paused" } else { "recording" },
            "meta": r.meta, "seconds": r.live.recorded_ms.load(Ordering::SeqCst) as f64 / 1000.0,
            "level": r.live.level.load(Ordering::SeqCst) as f64 / 32768.0, "mic_level": r.live.mic_level.load(Ordering::SeqCst) as f64 / 32768.0,
            "warning": r.live.error.lock().map(|e| e.clone()).unwrap_or_default(),
        }),
    }
}
#[tauri::command]
pub fn meeting_state() -> Value { state_json() }
#[derive(Serialize)]
pub struct Item { meta: Meta, size: u64, mic: bool, transcript: bool, summary: bool }
#[tauri::command]
pub fn meeting_list(app: AppHandle) -> Result<Value, String> {
    feature_on(&app)?;
    recover_all(&app);
    let dir = folder(&app);
    let recording = CURRENT.lock().ok().and_then(|c| c.as_ref().map(|r| r.meta.id.clone())).unwrap_or_default();
    let mut items = vec![];
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext != "wav" && ext != "opus" { continue; }
        let id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if id.ends_with(" (mic)") || id == recording { continue; }
        // While a WAV is being compressed both may exist: list it once.
        if ext == "wav" && p.with_extension("opus").is_file() { continue; }
        let meta: Meta = std::fs::read(p.with_extension("json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| Meta { id: id.clone(), title: id.clone(), ..Default::default() });
        let size = e.metadata().map_or(0, |m| m.len());
        items.push(Item { meta: Meta { id: id.clone(), ..meta }, size, mic: audio_of(&dir, &format!("{id} (mic)")).is_some(), transcript: dir.join(format!("{id}.transcript.json")).is_file(), summary: dir.join(format!("{id}.summary.json")).is_file() });
    }
    items.sort_by(|a, b| b.meta.started.cmp(&a.meta.started));
    let s = settings(&app);
    Ok(json!({ "items": items, "folder": dir.display().to_string(), "consent_seen": s.consent_seen }))
}
/// Deletes a recording with everything made from it (mic track, transcript, summary, notes).
#[tauri::command]
pub fn meeting_delete(app: AppHandle, id: String) -> Result<(), String> {
    let (wav, meta) = files_of(&app, &id).ok_or("That recording is gone.")?;
    if CURRENT.lock().ok().and_then(|c| c.as_ref().map(|r| r.meta.id.clone())).as_deref() == Some(id.as_str()) { return Err("Stop the recording first.".into()); }
    let dir = folder(&app);
    for p in [wav, meta, dir.join(format!("{id}.wav")), dir.join(format!("{id}.opus")), dir.join(format!("{id} (mic).wav")), dir.join(format!("{id} (mic).opus")), dir.join(format!("{id}.transcript.json")), dir.join(format!("{id}.summary.json")), dir.join(format!("{id}.16k.wav"))] { let _ = std::fs::remove_file(p); }
    let _ = app.emit("meeting-history", ());
    Ok(())
}
/// Plays a recording in the default player, or shows it in Explorer.
#[tauri::command]
pub fn meeting_open(app: AppHandle, id: String, reveal: bool) -> Result<(), String> {
    let (wav, _) = files_of(&app, &id).ok_or("That recording is gone.")?;
    let r = if reveal { std::process::Command::new("explorer.exe").arg(format!("/select,{}", wav.display())).spawn() } else { std::process::Command::new("explorer.exe").arg(&wav).spawn() };
    r.map(|_| ()).map_err(|e| e.to_string())
}
/// A recording's sound for the panel's player: the meeting app's track, or with `mic` your microphone's. Bytes only, so
/// the page never gets a path; a WAV left uncompressed (over 200 MB) plays in the default player instead.
#[tauri::command]
pub async fn meeting_audio(app: AppHandle, id: String, mic: bool) -> Result<tauri::ipc::Response, String> {
    let (audio, _) = files_of(&app, &id).ok_or("That recording is gone.")?;
    let path = if mic { audio_of(audio.parent().unwrap_or(Path::new(".")), &format!("{id} (mic)")).ok_or("No microphone track.")? } else { audio };
    if std::fs::metadata(&path).map_or(0, |m| m.len()) > 200 << 20 { return Err("Too long to play here. Use Play.".into()); }
    tauri::async_runtime::spawn_blocking(move || std::fs::read(path)).await.map_err(|e| e.to_string())?.map(tauri::ipc::Response::new).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn meeting_rename(app: AppHandle, id: String, title: String) -> Result<(), String> {
    let (_, meta_path) = files_of(&app, &id).ok_or("That recording is gone.")?;
    let mut meta: Meta = std::fs::read(&meta_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| Meta { id: id.clone(), ..Default::default() });
    meta.title = short(title.trim(), 120);
    std::fs::write(meta_path, serde_json::to_vec_pretty(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn meeting_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let start = folder(&app);
    let aside = crate::Aside::new();
    let picked = rfd::AsyncFileDialog::new().set_title("Folder for recordings").set_directory(start).pick_folder().await;
    drop(aside);
    let Some(p) = picked else { return Ok(None) };
    let mut s = settings(&app);
    s.folder = p.path().display().to_string();
    save_settings(&app, &s);
    Ok(Some(s.folder))
}
#[tauri::command]
pub fn meeting_consent_seen(app: AppHandle) { let mut s = settings(&app); s.consent_seen = true; save_settings(&app, &s); }


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_browser_meeting_is_named_by_its_service() {
        assert_eq!(service_in("Meet - Weekly sync - Google Chrome"), Some("Google Meet"));
        assert_eq!(service_in("meet.google.com is sharing your screen"), Some("Google Meet"));
        assert_eq!(service_in("Chat | Design review | Microsoft Teams - Microsoft Edge"), Some("Microsoft Teams"));
        assert_eq!(service_in("Zoom Meeting - Google Chrome"), Some("Zoom"));
        assert_eq!(service_in("Inbox - Gmail - Google Chrome"), None);
    }
    fn cand(pid: u32, c: &str) -> Candidate { Candidate { pid, provider: "Zoom".into(), title: String::new(), exe: "zoom.exe".into(), confidence: c.into(), score: 0, reasons: vec![], isolated: true, browser: false } }
    #[test]
    fn signals_add_up_and_a_mute_mic_does_not_matter() {
        assert_eq!(score(&Signals { running: true, ..Default::default() }).1, "low", "an idle app is no meeting");
        assert_eq!(score(&Signals { running: true, audio_active: true, ..Default::default() }).1, "medium", "sound alone could be a video");
        assert_eq!(score(&Signals { running: true, call_window: Some("Zoom Meeting".into()), ..Default::default() }).1, "medium", "a silent call still shows");
        let (n, c, why) = score(&Signals { running: true, call_window: Some("Zoom Meeting".into()), audio_active: true, any_window: None });
        assert_eq!((n, c, why.len()), (5, "high", 3));
    }
    #[test]
    fn only_one_sure_meeting_is_picked_by_itself() {
        assert_eq!(auto_pick(&[cand(1, "high"), cand(2, "medium")]).map(|c| c.pid), Some(1));
        assert_eq!(auto_pick(&[cand(1, "high"), cand(2, "high")]), None, "two meetings: ask");
        assert_eq!(auto_pick(&[cand(1, "medium")]), None, "unsure: ask");
        assert_eq!(auto_pick(&[]), None);
    }
    #[test]
    fn the_tree_root_is_the_topmost_same_app() {
        let p: HashMap<u32, (u32, String)> = [(1, (0, "explorer.exe".to_string())), (10, (1, "ms-teams.exe".into())), (11, (10, "ms-teams.exe".into())), (12, (11, "msedgewebview2.exe".into()))].into_iter().collect();
        assert_eq!(tree_root(&p, 11), 10);
        assert_eq!(tree_root(&p, 10), 10);
        assert!(in_tree(&p, 12, 10) && !in_tree(&p, 1, 10));
        let looped: HashMap<u32, (u32, String)> = [(5, (6, "a.exe".to_string())), (6, (5, "a.exe".into()))].into_iter().collect();
        let _ = tree_root(&looped, 5); // must not hang
    }
    #[test]
    fn names_are_safe_for_windows() {
        assert_eq!(safe_name("2026-11-12 14-03 Zoom: Q4 <plan>?"), "2026-11-12 14-03 Zoom Q4 plan");
        assert_eq!(safe_name("con"), "Meeting");
        assert_eq!(safe_name("..."), "Meeting");
        assert_eq!(safe_name(&"x".repeat(200)).len(), 60);
    }
    #[test]
    fn a_wav_streams_to_disk_and_a_crash_leaves_it_playable() {
        let dir = tempfile::tempdir().unwrap();
        let path = unique(dir.path(), "Meeting", "wav");
        let mut w = WavWriter::create(&path).unwrap();
        w.write(&[1, 0, 2, 0, 3, 0, 4, 0]).unwrap();
        assert!(part_path(&path).exists() && !path.exists(), "a .part while it records");
        let done = w.finish().unwrap();
        let b = std::fs::read(&done).unwrap();
        assert_eq!((&b[0..4], &b[8..12], u32::from_le_bytes(b[40..44].try_into().unwrap()), b.len()), (&b"RIFF"[..], &b"WAVE"[..], 8, 52));
        assert_eq!(unique(dir.path(), "Meeting", "wav").file_name().unwrap(), "Meeting (2).wav", "never over an older file");
        // A crash: a .part with a stale header and half a frame at the end.
        let crashed = dir.path().join("Lost.wav.part");
        let mut data = wav_header(0, RATE, CHANNELS, BITS).to_vec(); data.extend_from_slice(&[9u8; 4 * 10 + 3]);
        std::fs::write(&crashed, &data).unwrap();
        let fixed = recover(&crashed).unwrap();
        assert_eq!(fixed.file_name().unwrap(), "Lost (recovered).wav");
        let b = std::fs::read(&fixed).unwrap();
        assert_eq!((u32::from_le_bytes(b[40..44].try_into().unwrap()), b.len()), (40, 84), "whole frames kept, the torn one dropped");
        assert!(!crashed.exists());
    }
    #[test]
    #[cfg_attr(not(win), ignore = "needs the bundled engines, which are Windows programs for now")]
    fn the_gpu_library_unpacks_from_sora_itself() {
        let dir = tempfile::tempdir().unwrap();
        let (marker, locks) = crate::convert::engines::Engine::WhisperGpu.prepare(dir.path()).unwrap();
        let folder = marker.parent().unwrap();
        assert!(folder.join("ggml-vulkan.dll").is_file() && folder.join("LICENSE").is_file());
        assert_eq!(locks.len(), 3, "every file is checked and held");
        drop(locks);
        std::fs::write(folder.join("ggml-vulkan.dll"), b"tampered").unwrap();
        let (_, again) = crate::convert::engines::Engine::WhisperGpu.prepare(dir.path()).unwrap();
        assert!(std::fs::metadata(folder.join("ggml-vulkan.dll")).unwrap().len() > 1_000_000, "a changed copy is replaced");
        drop(again);
    }
    #[test]
    #[cfg_attr(not(win), ignore = "needs the bundled engines, which are Windows programs for now")]
    fn a_finished_recording_shrinks_to_opus_and_the_wav_goes() {
        let dir = tempfile::tempdir().unwrap();
        let (ffmpeg, _locks) = crate::convert::engines::Engine::Ffmpeg.prepare(&dir.path().join("cache")).unwrap();
        // Three seconds of a 440 Hz tone, as the recorder writes it.
        let path = dir.path().join("Meeting.wav");
        let mut w = WavWriter::create(&path).unwrap();
        let mut pcm = vec![];
        for i in 0..RATE * 3 { let s = ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16; for _ in 0..CHANNELS { pcm.extend_from_slice(&s.to_le_bytes()); } }
        w.write(&pcm).unwrap();
        let wav = w.finish().unwrap();
        let wav_size = std::fs::metadata(&wav).unwrap().len();
        let opus = compress_file(&ffmpeg, &wav).unwrap();
        let opus_size = std::fs::metadata(&opus).unwrap().len();
        assert!(!wav.exists(), "the WAV is gone once the Opus file is there");
        assert!(opus_size * 20 < wav_size, "much smaller: {opus_size} vs {wav_size}");
        assert_eq!(audio_of(dir.path(), "Meeting"), Some(opus));
        // A broken input keeps what there is.
        let bad = dir.path().join("Bad.wav"); std::fs::write(&bad, b"not audio").unwrap();
        assert!(compress_file(&ffmpeg, &bad).is_err() && bad.exists() && !dir.path().join("Bad.opus").exists());
    }
    #[test]
    #[cfg(win)]
    fn this_pc_reports_its_build() { assert!(windows_build() > 10000); }
    #[test]
    fn a_meeting_ends_when_its_window_goes_and_it_falls_silent() {
        let s = Duration::from_secs;
        assert!(meeting_ended(true, false, s(31)), "Zoom cut the free call: window gone, silence");
        assert!(!meeting_ended(true, false, s(10)), "a short quiet moment (screen share) is not the end");
        assert!(!meeting_ended(true, true, s(600)), "a muted meeting with its window open goes on");
        assert!(!meeting_ended(false, false, s(600)), "no call window was ever seen: only the app closing ends it");
    }
    /// Real capture on this PC (needs sound): plays a Windows sound in a child process and records only that process.
    #[test]
    #[ignore]
    fn records_one_process_tree() {
        assert!(loopback_supported());
        let mut child = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "$p=New-Object Media.SoundPlayer 'C:\\Windows\\Media\\Alarm01.wav'; $p.PlaySync(); $p.PlaySync()"]).spawn().unwrap();
        thread::sleep(Duration::from_millis(1500));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let live = Arc::new(Live { stop: AtomicBool::new(false), paused: AtomicBool::new(false), level: AtomicU32::new(0), mic_level: AtomicU32::new(0), recorded_ms: AtomicU64::new(0), error: Mutex::new(String::new()) });
        let (tx, rx) = mpsc::channel();
        let (l2, p2) = (live.clone(), path.clone());
        let pid = child.id();
        let t = thread::spawn(move || run_capture(Source::Process(pid), p2, l2, tx, false));
        let t0 = Instant::now();
        rx.recv().unwrap().unwrap();
        println!("ready after {:?}", t0.elapsed());
        thread::sleep(Duration::from_secs(4));
        live.stop.store(true, Ordering::SeqCst);
        let t1 = Instant::now();
        let out = t.join().unwrap().unwrap();
        println!("stopped after {:?}, recorded_ms {}", t1.elapsed(), live.recorded_ms.load(Ordering::SeqCst));
        let _ = child.kill();
        let b = std::fs::read(out).unwrap();
        let peak = b[44..].chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs()).max().unwrap_or(0);
        println!("bytes {} peak {peak}", b.len());
        assert!(b.len() > 44 + 48_000 * 4 * 3, "about 4 s on disk");
        assert!(peak > 500, "the child's sound was captured");
    }
    /// Isolation on this PC: while one process plays a sound, recording a different (silent) process tree stays silent.
    #[test]
    #[ignore]
    fn another_process_stays_out() {
        let mut noisy = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "$p=New-Object Media.SoundPlayer 'C:\\Windows\\Media\\Ring01.wav'; $p.PlaySync(); $p.PlaySync()"]).spawn().unwrap();
        let mut quiet = std::process::Command::new("ping").args(["-n", "8", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
        thread::sleep(Duration::from_millis(1500));
        let dir = tempfile::tempdir().unwrap();
        let rec = |pid: u32, name: &str| {
            let live = Arc::new(Live { stop: AtomicBool::new(false), paused: AtomicBool::new(false), level: AtomicU32::new(0), mic_level: AtomicU32::new(0), recorded_ms: AtomicU64::new(0), error: Mutex::new(String::new()) });
            let (tx, rx) = mpsc::channel();
            let (l2, p2) = (live.clone(), dir.path().join(name));
            let t = thread::spawn(move || run_capture(Source::Process(pid), p2, l2, tx, false));
            rx.recv().unwrap().unwrap();
            (live, t)
        };
        let (a, ta) = rec(noisy.id(), "noisy.wav");
        let (b, tb) = rec(quiet.id(), "quiet.wav");
        thread::sleep(Duration::from_secs(3));
        a.stop.store(true, Ordering::SeqCst); b.stop.store(true, Ordering::SeqCst);
        let peak = |p: PathBuf| { let d = std::fs::read(p).unwrap(); d[44..].chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs()).max().unwrap_or(0) };
        let (pa, pb) = (peak(ta.join().unwrap().unwrap()), peak(tb.join().unwrap().unwrap()));
        let _ = noisy.kill(); let _ = quiet.kill();
        println!("noisy peak {pa}, quiet peak {pb}");
        assert!(pa > 500, "the playing process is heard");
        assert!(pb < 50, "the other process's recording holds none of that sound");
    }
}