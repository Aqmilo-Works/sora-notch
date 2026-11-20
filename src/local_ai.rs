//! Local AI for meeting recordings (Labs): transcripts with whisper.cpp and summaries with llama.cpp, both run as child
//! programs on this PC (ADR-0003). No Ollama, no server, no cloud. Nothing is downloaded until you click Download for one
//! part; each part comes from a pinned HTTPS address and must match its pinned SHA-256 and size, or it is thrown away.
//! The recorder works without any of this, and a transcript works without the summary model.
//!
//! Workers get file paths only and never a network address; Sora itself makes no request while they run.
use std::{collections::HashMap, fs, io::{Read, Write}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use futures_util::StreamExt;
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
// What differs per OS lives in win/local_ai.rs and mac/local_ai.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/local_ai.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/local_ai.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

#[derive(Serialize, Clone, Copy)]
pub struct Part {
    pub id: &'static str,
    /// "transcript" or "summary"; "engine" or "model".
    pub group: &'static str, pub kind: &'static str,
    pub name: &'static str, pub detail: &'static str, pub license: &'static str,
    #[serde(skip)] pub url: &'static str, #[serde(skip)] pub sha256: &'static str,
    pub size: u64,
    /// Smallest free memory (GB) it is offered for without a warning.
    pub min_ram_gb: f64,
}
/// Pinned 2026-11-12 (engines, Whisper Base and Small) and 2026-11-13 (Whisper Large V3 Turbo, Qwen3.5) from the GitHub
/// release API (asset digests) and Hugging Face (LFS SHA-256). Qwen publishes no GGUF of Qwen3.5: the Q4_K_M files come
/// from Unsloth's conversions of the official weights (same Apache-2.0 licence). 2026-11-17: Whisper Base q5_1 and
/// Silero VAD v6.2 (ggml-org/whisper-vad), both from the Hugging Face LFS SHA-256. Measured on 8 min of FLEURS speech:
/// with VAD Whisper Base takes about half the time and Whisper Small gets more accurate (it no longer skips stretches);
/// q5_1 keeps Base's accuracy at 40% of its size but costs Small 7 points, so Small stays full.
pub const CATALOG: [Part; 12] = [
    Part { id: "whisper-engine", group: "transcript", kind: "engine", name: "Transcript engine", detail: "whisper.cpp b5454, CPU (x64)", license: "MIT", url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip", sha256: "6ba69e3482d7826214f90a6a9c84ca07782aec1e1d0c6a7c30c994fd5d816ccb", size: 8928640, min_ram_gb: 0.0 },
    Part { id: "whisper-base-q5", group: "transcript", kind: "model", name: "Whisper Base", detail: "Fast, 4 GB RAM", license: "MIT (OpenAI Whisper)", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin", sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898", size: 59707625, min_ram_gb: 0.5 },
    Part { id: "whisper-small", group: "transcript", kind: "model", name: "Whisper Small", detail: "Accurate, 8 GB RAM", license: "MIT (OpenAI Whisper)", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin", sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b", size: 487601967, min_ram_gb: 2.0 },
    Part { id: "whisper-turbo", group: "transcript", kind: "model", name: "Whisper Large V3 Turbo", detail: "Most accurate, 16 GB RAM", license: "MIT (OpenAI Whisper)", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin", sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2", size: 574041195, min_ram_gb: 2.0 },
    // A helper, not a model: Whisper skips the silence it marks (faster, and no made-up lines in quiet stretches).
    Part { id: "whisper-vad", group: "transcript", kind: "helper", name: "Voice detection", detail: "Silero VAD v6.2: skips silence", license: "MIT", url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin", sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987", size: 885098, min_ram_gb: 0.0 },
    // Voice separation (installed with every transcript package, 2026-11-17): sherpa-onnx v2.0.1 (GitHub release digest), the pyannote segmentation 3.0
    // model (int8, Hugging Face LFS SHA-256) and 3D-Speaker's ERes2Net VoxCeleb embedding (SHA-256 of the release file,
    // which lists no digest). Picked on sherpa's 2- and 4-speaker samples compressed to Opus like a recording: with
    // threshold 0.9 it found 2, 2, 2 and 4 voices, and one voice for one speaker; CAM++ and lower thresholds split people.
    Part { id: "speaker-engine", group: "speakers", kind: "engine", name: "Voice engine", detail: SPEAKER_DETAIL, license: "Apache-2.0", url: SPEAKER_URL, sha256: SPEAKER_SHA256, size: SPEAKER_SIZE, min_ram_gb: 0.0 },
    Part { id: "speaker-seg", group: "speakers", kind: "helper", name: "Speech turns", detail: "pyannote segmentation 3.0", license: "MIT", url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.int8.onnx", sha256: "d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d", size: 1540506, min_ram_gb: 0.0 },
    Part { id: "speaker-voice", group: "speakers", kind: "model", name: "Voice separation", detail: "Tells speakers apart", license: "Apache-2.0 (3D-Speaker)", url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx", sha256: "c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5", size: 26485263, min_ram_gb: 0.5 },
    Part { id: "llama-engine", group: "summary", kind: "engine", name: "Summary engine", detail: "llama.cpp b12318, CPU (x64)", license: "MIT", url: "https://github.com/ggml-org/llama.cpp/releases/download/b12318/llama-b12318-bin-win-cpu-x64.zip", sha256: "df4b8ca49b5d21202238faafe6685affcd3012dcc523be1ad0d74fda6a72c235", size: 25084375, min_ram_gb: 0.0 },
    Part { id: "qwen35-0.8b", group: "summary", kind: "model", name: "Qwen3.5 0.8B", detail: "Key points only", license: "Apache-2.0", url: "https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/Qwen3.5-0.8B-Q4_K_M.gguf", sha256: "bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517", size: 532517120, min_ram_gb: 1.5 },
    Part { id: "qwen35-2b", group: "summary", kind: "model", name: "Qwen3.5 2B", detail: "Topics and action items", license: "Apache-2.0", url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf", sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223", size: 1280835840, min_ram_gb: 2.5 },
    Part { id: "qwen35-4b", group: "summary", kind: "model", name: "Qwen3.5 4B", detail: "Detailed, for long meetings", license: "Apache-2.0", url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf", sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4", size: 2740937888, min_ram_gb: 4.5 },
];
/// Parts an older Sora offered; their files are deleted (Whisper Tiny, Qwen2.5, full-size Whisper Base).
const RETIRED: [&str; 4] = ["whisper-tiny", "qwen-0.5b", "qwen-1.5b", "whisper-base"];
fn part(id: &str) -> Option<&'static Part> { CATALOG.iter().find(|p| p.id == id) }
/// Only these hosts may serve a download (redirects included). Pure; tested.
pub fn host_allowed(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    ["github.com", "githubusercontent.com", "huggingface.co", "hf.co"].iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

fn root(app: &AppHandle) -> Result<PathBuf, String> { let d = app.path().app_data_dir().map_err(|e| e.to_string())?.join("local-ai"); fs::create_dir_all(&d).map_err(|e| e.to_string())?; Ok(d) }
fn file_of(app: &AppHandle, p: &Part) -> Result<PathBuf, String> {
    Ok(root(app)?.join(match p.kind { "engine" if p.url.ends_with(".tar.bz2") => format!("{}.tar.bz2", p.id), "engine" => format!("{}.zip", p.id), _ => format!("{}.bin", p.id) }))
}
fn engine_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> { Ok(root(app)?.join(id)) }
fn installed(app: &AppHandle, p: &Part) -> bool {
    if p.kind == "engine" { engine_exe(app, p.id).is_some() } else { file_of(app, p).is_ok_and(|f| fs::metadata(f).is_ok_and(|m| m.len() == p.size)) }
}
/// Whisper on the graphics card: ggml-vulkan.dll, packed into Sora.exe (data/converter/whisper-gpu.zip) and unpacked
/// to the converter cache, checked by SHA-256 like ffmpeg. whisper.cpp publishes no Vulkan build, so
/// tools/build-whisper-vulkan.cmd builds it from the same b5454 source as whisper-engine (it must match that engine's
/// ggml). On an RTX 3060 it was 2 to 4.7 times faster than the CPU on 14 min of FLEURS speech with the same word error
/// rate (within 3 points either way). The open files keep it from being swapped while Whisper loads it.
fn gpu_dir(app: &AppHandle) -> Option<(PathBuf, Vec<fs::File>)> {
    let cache = app.path().app_cache_dir().ok()?.join("converter");
    let (marker, locks) = crate::convert::engines::Engine::WhisperGpu.prepare(&cache).ok()?;
    Some((marker.parent()?.to_path_buf(), locks))
}
fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let p = dir.join(name);
    if p.is_file() { return Some(p); }
    if depth == 0 { return None; }
    fs::read_dir(dir).ok()?.flatten().filter(|e| e.path().is_dir()).find_map(|e| find_file(&e.path(), name, depth - 1))
}
fn hex(d: ring::digest::Digest) -> String { d.as_ref().iter().map(|b| format!("{b:02x}")).collect() }
pub(crate) fn feature_on(app: &AppHandle) -> Result<(), String> { if crate::desktop::preferences(app).feature_local_ai { Ok(()) } else { Err("Install a meeting package in Settings > Meeting first.".into()) } }

// ---------------------------------------------------------------- downloads
#[derive(Serialize, Clone, Default)]
struct Progress { got: u64, total: u64, state: String, error: String }
static DOWNLOADS: Mutex<Option<HashMap<String, Progress>>> = Mutex::new(None);
static CANCEL: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn set_progress(app: &AppHandle, id: &str, p: Progress) {
    if let Ok(mut d) = DOWNLOADS.lock() { d.get_or_insert_with(HashMap::new).insert(id.to_string(), p); }
    let _ = app.emit("ai-progress", json!({ "id": id }));
}
fn cancelled(id: &str) -> bool { CANCEL.lock().map_or(false, |c| c.iter().any(|x| x == id)) }

/// Downloads one part (resuming a .part file), checks size and SHA-256, unpacks an engine. Only on a click.
#[tauri::command]
pub async fn ai_download(app: AppHandle, id: String) -> Result<(), String> {
    // The Install click in Settings > Meeting is the opt-in: Local AI turns itself on once a package is there.
    let p = crate::desktop::preferences(&app);
    if !p.feature_meeting && !p.feature_local_ai { return Err("Turn on the meeting recorder in Settings > Meeting first.".into()); }
    let p = part(&id).ok_or("Unknown part.")?;
    if p.kind == "engine" { if let Some(hint) = engine_hint(p.id) { return Err(hint); } }
    if DOWNLOADS.lock().map_or(false, |d| d.as_ref().and_then(|d| d.get(&id)).is_some_and(|x| x.state == "downloading")) { return Err("Already downloading.".into()); }
    if let Ok(mut c) = CANCEL.lock() { c.retain(|x| x != &id); }
    let dest = file_of(&app, p)?;
    // Free space for the file (and, for an engine, its unpacked copy) plus some room.
    let need = p.size * if p.kind == "engine" { 3 } else { 1 } + 200 * 1024 * 1024;
    if free_bytes(&root(&app)?) < need { return Err(format!("Not enough free disk space: {} needed.", gb(need))); }
    set_progress(&app, &id, Progress { got: 0, total: p.size, state: "downloading".into(), error: String::new() });
    let result = fetch(&app, p, &dest).await;
    let result = match result { Ok(()) if p.kind == "engine" => unpack(&app, p, &dest), r => r };
    match &result {
        Ok(()) => set_progress(&app, &id, Progress { got: p.size, total: p.size, state: "done".into(), error: String::new() }),
        Err(e) => set_progress(&app, &id, Progress { got: 0, total: p.size, state: if cancelled(&id) { "cancelled" } else { "failed" }.into(), error: e.clone() }),
    }
    result
}
async fn fetch(app: &AppHandle, p: &Part, dest: &Path) -> Result<(), String> {
    let partial = dest.with_extension("part");
    // A resumed download is checked from its first byte.
    let mut digest = Context::new(&SHA256);
    let mut have = 0u64;
    if let Ok(mut f) = fs::File::open(&partial) {
        let mut buf = vec![0u8; 1 << 16];
        loop { let n = f.read(&mut buf).map_err(|e| e.to_string())?; if n == 0 { break; } digest.update(&buf[..n]); have += n as u64; }
        if have > p.size { have = 0; digest = Context::new(&SHA256); let _ = fs::remove_file(&partial); }
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|a| if a.url().scheme() == "https" && a.url().host_str().is_some_and(host_allowed) && a.previous().len() < 8 { a.follow() } else { a.stop() }))
        .connect_timeout(std::time::Duration::from_secs(20)).user_agent("Sora").build().map_err(|e| e.to_string())?;
    let mut req = client.get(p.url);
    if have > 0 { req = req.header("Range", format!("bytes={have}-")); }
    let res = req.send().await.map_err(|_| "Could not reach the download server. Check your connection.".to_string())?;
    if !res.url().host_str().is_some_and(host_allowed) { return Err("The download was sent to an unknown server, so Sora stopped.".into()); }
    let status = res.status();
    if have > 0 && status.as_u16() == 200 { have = 0; digest = Context::new(&SHA256); }
    else if !(status.is_success()) { return Err(format!("The server answered {status}.")); }
    let mut file = fs::OpenOptions::new().create(true).write(true).append(have > 0).truncate(have == 0).open(&partial).map_err(|e| e.to_string())?;
    let mut stream = res.bytes_stream();
    let mut got = have;
    let mut last = Instant::now();
    while let Some(chunk) = stream.next().await {
        if cancelled(p.id) { return Err("Download cancelled.".into()); }
        let chunk = chunk.map_err(|_| "The connection dropped. Click Download to resume.".to_string())?;
        got += chunk.len() as u64;
        if got > p.size { drop(file); let _ = fs::remove_file(&partial); return Err("The download is bigger than expected, so Sora threw it away.".into()); }
        digest.update(&chunk);
        file.write_all(&chunk).map_err(|e| if e.raw_os_error() == Some(112) { "The disk is full.".to_string() } else { e.to_string() })?;
        if last.elapsed().as_millis() > 250 { last = Instant::now(); set_progress(app, p.id, Progress { got, total: p.size, state: "downloading".into(), error: String::new() }); }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if got != p.size { return Err("The download stopped early. Click Download to resume.".into()); }
    if hex(digest.finish()) != p.sha256 { let _ = fs::remove_file(&partial); return Err("The download failed its integrity check and was deleted.".into()); }
    fs::rename(&partial, dest).map_err(|e| e.to_string())
}
/// Unpacks an engine archive into its own folder (names that would leave the folder are refused).
fn unpack(app: &AppHandle, p: &Part, zip_path: &Path) -> Result<(), String> {
    let dir = engine_dir(app, p.id)?;
    let tmp = dir.with_extension("unpacking");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    if p.url.ends_with(".tar.bz2") { untar(zip_path, &tmp)?; } else { unzip(zip_path, &tmp)?; }
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&tmp, &dir).map_err(|e| e.to_string())?;
    // The archive is no longer needed once unpacked (it was checked before).
    let _ = fs::remove_file(zip_path);
    if engine_exe(app, p.id).is_none() { return Err("The engine archive did not contain the expected program.".into()); }
    Ok(())
}
fn unzip(zip_path: &Path, tmp: &Path) -> Result<(), String> {
    let mut z = zip::ZipArchive::new(fs::File::open(zip_path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { return Err("The engine archive has an unsafe file name.".into()) };
        let out = tmp.join(rel);
        if f.is_dir() { fs::create_dir_all(&out).map_err(|e| e.to_string())?; continue; }
        if let Some(d) = out.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
        let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut w).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn ai_cancel_download(id: String) { if let Ok(mut c) = CANCEL.lock() { c.push(id); } }
/// Deletes a downloaded part; recordings and transcripts stay.
#[tauri::command]
pub fn ai_remove(app: AppHandle, id: String) -> Result<(), String> {
    let p = part(&id).ok_or("Unknown part.")?;
    if JOB.lock().map_or(false, |j| j.is_some()) { return Err("Wait until the current job ends.".into()); }
    let f = file_of(&app, p)?;
    let _ = fs::remove_file(&f); let _ = fs::remove_file(f.with_extension("part"));
    if p.kind == "engine" { let _ = fs::remove_dir_all(engine_dir(&app, p.id)?); }
    if let Ok(mut d) = DOWNLOADS.lock() { if let Some(d) = d.as_mut() { d.remove(&id); } }
    Ok(())
}

// ---------------------------------------------------------------- this PC
#[derive(Serialize, Clone, Debug)]
pub struct Hardware { pub ram_gb: f64, pub free_ram_gb: f64, pub cores: usize, pub avx2: bool, pub disk_free_gb: f64, pub on_battery: bool, pub battery: Option<u8> }
fn gb(b: u64) -> String { format!("{:.1} GB", b as f64 / 1073741824.0) }
/// The three bundles, by the PC's total memory: (name, transcript model, summary model).
pub const TIERS: [(&str, &str, &str); 3] = [("Sora AI Lite", "whisper-base-q5", "qwen35-0.8b"), ("Sora AI Balanced", "whisper-small", "qwen35-2b"), ("Sora AI Powerful", "whisper-turbo", "qwen35-4b")];
/// The bundle for this PC from its total memory (under 8 GB Lite, under 16 GB Balanced, else Powerful), one step lower
/// with fewer than 4 threads, Lite without AVX2. The summary model must also fit the memory free now: else a smaller
/// one, or none (pure; tested).
pub fn recommend(ram_gb: f64, free_ram_gb: f64, cores: usize, avx2: bool) -> (&'static str, &'static str, Option<&'static str>, Vec<String>) {
    let mut notes = vec![];
    // Windows reports a little under the installed memory (8 GB shows as 7.8).
    let mut tier: usize = if ram_gb < 7.5 { 0 } else if ram_gb < 15.0 { 1 } else { 2 };
    if cores < 4 { tier = tier.saturating_sub(1); }
    if !avx2 { tier = 0; notes.push("This CPU lacks AVX2: expect slow transcripts.".to_string()); }
    let (name, whisper, wanted) = TIERS[tier];
    let fits = |id: &str| part(id).is_some_and(|p| free_ram_gb >= p.min_ram_gb);
    let summary = TIERS[..=tier].iter().rev().map(|t| t.2).find(|id| fits(id));
    if summary != Some(wanted) { notes.push(if summary.is_some() { "Less memory is free now: a smaller summary model fits.".into() } else { "Too little free memory for a summary model now.".into() }); }
    (name, whisper, summary, notes)
}

// ---------------------------------------------------------------- jobs
struct JobState { kind: String, id: String, progress: u8, stage: String, child: Option<Child> }
static JOB: Mutex<Option<JobState>> = Mutex::new(None);
static STOP_JOB: AtomicBool = AtomicBool::new(false);
pub(crate) fn job_cancelled() -> bool { STOP_JOB.load(Ordering::SeqCst) }
pub(crate) fn job_set(app: &AppHandle, progress: u8, stage: &str) {
    if let Ok(mut j) = JOB.lock() { if let Some(j) = j.as_mut() { j.progress = progress; j.stage = stage.into(); } }
    let _ = app.emit("ai-job", job_view());
}
fn job_view() -> Value { JOB.lock().ok().and_then(|j| j.as_ref().map(|j| json!({ "kind": j.kind, "id": j.id, "progress": j.progress, "stage": j.stage }))).unwrap_or(Value::Null) }
/// Threads and priority per mode: Eco leaves most of the PC free, Fast uses nearly all of it (pure; tested).
pub fn threads_for(mode: &str, cores: usize) -> (usize, u32) {
    // Windows priority classes: IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS.
    match mode {
        "eco" => (1.max(cores / 4), 0x40),
        "fast" => (1.max(cores.saturating_sub(1)), 0x20),
        _ => (1.max(cores / 2), 0x4000),
    }
}
/// Runs a worker in a job object at the given priority; returns its stdout. Stderr lines go to `line` (progress).
fn run_worker(app: &AppHandle, mut cmd: Command, priority: u32, line: &dyn Fn(&str)) -> Result<String, String> {
    // Recording comes first: a step that has not started yet waits until the recording ends.
    if crate::meeting::is_recording() {
        let (p, s) = job_now();
        job_set(app, p, PAUSED);
        while crate::meeting::is_recording() && !STOP_JOB.load(Ordering::SeqCst) { std::thread::sleep(Duration::from_millis(500)); }
        job_set(app, p, &s);
    }
    if STOP_JOB.load(Ordering::SeqCst) { return Err("Cancelled.".into()); }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let (mut child, job) = crate::convert::engines::spawn_jobbed(&mut cmd)?;
    let pid = child.id();
    set_priority(&child, priority);
    let mut out = child.stdout.take().ok_or("No output.")?;
    let mut err = child.stderr.take().ok_or("No output.")?;
    if let Ok(mut j) = JOB.lock() { if let Some(j) = j.as_mut() { j.child = Some(child); } }
    let reader = std::thread::spawn(move || { let mut s = String::new(); let _ = out.read_to_string(&mut s); s });
    let done = Arc::new(AtomicBool::new(false));
    let holder = hold_while_recording(app, pid, done.clone());
    let mut buf = [0u8; 4096]; let mut cur = Vec::new(); let mut last_err = String::new();
    loop {
        let n = err.read(&mut buf).unwrap_or(0); if n == 0 { break; }
        for &b in &buf[..n] { if b == b'\n' || b == b'\r' { if !cur.is_empty() { let t = String::from_utf8_lossy(&cur).to_string(); line(&t); if t.to_lowercase().contains("error") { last_err = t.chars().take(200).collect(); } cur.clear(); } } else if cur.len() < 4096 { cur.push(b); } }
    }
    let stdout = reader.join().unwrap_or_default();
    let status = JOB.lock().ok().and_then(|mut j| j.as_mut().and_then(|j| j.child.take())).map(|mut c| c.wait());
    done.store(true, Ordering::SeqCst);
    let _ = holder.join();
    drop(job);
    if STOP_JOB.load(Ordering::SeqCst) { return Err("Cancelled.".into()); }
    match status { Some(Ok(s)) if s.success() => Ok(stdout), Some(Ok(s)) if s.code() == Some(-1073741795) || s.code() == Some(-1073741571) => Err("The engine does not run on this CPU.".into()),
        _ => Err(if last_err.is_empty() { "The engine stopped with an error (out of memory?).".into() } else { last_err }) }
}
const PAUSED: &str = "Paused while recording";
fn job_now() -> (u8, String) { JOB.lock().ok().and_then(|j| j.as_ref().map(|j| (j.progress, j.stage.clone()))).unwrap_or((0, String::new())) }
/// Recording comes first: a worker still running when a new recording starts (the transcript of the call before,
/// a summary) is frozen until that recording ends, so the call keeps the CPU and the GPU. Its stage says so.
fn hold_while_recording(app: &AppHandle, pid: u32, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut held: Option<(u8, String)> = None;
        while !done.load(Ordering::SeqCst) {
            let rec = crate::meeting::is_recording();
            if rec && held.is_none() { let now = job_now(); freeze(pid, true); job_set(&app, now.0, PAUSED); held = Some(now); }
            else if !rec { if let Some((p, s)) = held.take() { freeze(pid, false); job_set(&app, p, &s); } }
            std::thread::sleep(Duration::from_millis(500));
        }
        if held.is_some() { freeze(pid, false); }
    })
}
pub(crate) fn begin(kind: &str, id: &str) -> Result<(), String> {
    if crate::meeting::is_recording() { return Err("Wait until the recording stops: recording comes first.".into()); }
    let mut j = JOB.lock().map_err(|e| e.to_string())?;
    if j.is_some() { return Err("Another AI job is running.".into()); }
    STOP_JOB.store(false, Ordering::SeqCst);
    *j = Some(JobState { kind: kind.into(), id: id.into(), progress: 0, stage: "Starting".into(), child: None });
    Ok(())
}
pub(crate) fn end(app: &AppHandle) { if let Ok(mut j) = JOB.lock() { *j = None; } let _ = app.emit("ai-job", Value::Null); let _ = app.emit("meeting-history", ()); }
#[tauri::command]
pub fn ai_cancel() { STOP_JOB.store(true, Ordering::SeqCst); if let Ok(mut j) = JOB.lock() { if let Some(c) = j.as_mut().and_then(|j| j.child.as_mut()) { let _ = c.kill(); } } }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Segment { pub id: String, pub start: f64, pub end: f64, pub text: String,
    /// "me" (your microphone), "others" (the meeting app), "p1", "p2"… (one voice of the meeting app, when the voice
    /// package heard more than one) or "" (one track only, or both at once).
    #[serde(default, skip_serializing_if = "String::is_empty")] pub speaker: String }
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Transcript { pub id: String, pub model: String, pub language: String, pub created: i64, pub audio_s: f64, pub processing_s: f64, pub segments: Vec<Segment>, #[serde(default)] pub edited: bool,
    /// Names given to speakers in the panel ("p1" -> "Budi"); "me" is always You.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")] pub names: std::collections::BTreeMap<String, String> }
pub(crate) fn now_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64) }
/// whisper.cpp's JSON output (-oj) as lines (pure; tested). Run with -ml 1 -sow it holds one word per entry, each with
/// its own -di speaker, so a reply right after the other side is not folded into their line. Words join into one line
/// until the speaker changes, a pause of a second, a sentence ends after 3 s, or the line reaches 30 s.
/// `turns` (from `parse_turns`, empty without the voice package) splits the meeting app's side into Person 1, 2… by
/// voice, numbered in the order they first speak; with one voice it stays "others".
pub fn parse_whisper(v: &Value, turns: &[(f64, f64, usize)]) -> Vec<Segment> {
    let mut words: Vec<(f64, f64, &str, String)> = vec![];
    let mut last = "";
    for s in v.get("transcription").and_then(Value::as_array).into_iter().flatten() {
        let Some(raw) = s.get("text").and_then(Value::as_str) else { continue };
        if raw.trim().is_empty() || raw.trim() == "[BLANK_AUDIO]" { continue; }
        let ms = |k: &str| s.pointer(&format!("/offsets/{k}")).and_then(Value::as_f64).unwrap_or(0.0) / 1000.0;
        // -di: "0" is the left channel (the app), "1" the right (your mic). "?" (neither side clearly louder) is mostly
        // a short word: it keeps the speaker before it.
        let speaker = match s.get("speaker").and_then(Value::as_str) { Some("0") => "others", Some("1") => "me", Some(_) => last, None => "" };
        last = speaker;
        words.push((ms("from"), ms("to"), raw, speaker.to_string()));
    }
    let voices: Vec<Option<usize>> = words.iter().map(|w| if w.3 == "me" { None } else { voice_at(turns, w.0, w.1) }).collect();
    let mut order: Vec<usize> = vec![];
    for v in voices.iter().flatten() { if !order.contains(v) { order.push(*v); } }
    if order.len() > 1 {
        // A word between turns keeps the person before it.
        let mut prev = None;
        for (w, v) in words.iter_mut().zip(&voices) {
            if w.3 == "me" { continue; }
            prev = v.or(prev);
            if let Some(n) = prev.and_then(|v| order.iter().position(|x| *x == v)) { w.3 = format!("p{}", n + 1); }
        }
    }
    // A turn's first word often gets the speaker before it (its timing starts a little early): a word that opens a
    // sentence, has no sentence end itself and runs straight into another speaker's words belongs to them.
    for i in 1..words.len().saturating_sub(1) {
        let opens = words[i - 1].2.trim_end().ends_with(['.', '?', '!']) && !words[i].2.trim_end().ends_with(['.', '?', '!']);
        if opens && words[i].3 != words[i + 1].3 && words[i + 1].0 - words[i].1 < 0.3 { words[i].3 = words[i + 1].3.clone(); }
    }
    let mut out: Vec<Segment> = vec![];
    for (start, end, raw, speaker) in words {
        match out.last_mut() {
            Some(l) if l.speaker == speaker && start - l.end < 1.0 && l.end - l.start < 30.0 && !(l.end - l.start >= 3.0 && l.text.trim_end().ends_with(['.', '?', '!'])) => {
                if !raw.starts_with(' ') && !l.text.is_empty() { l.text.push(' '); }
                l.text.push_str(raw); l.end = end;
            }
            _ => out.push(Segment { id: String::new(), start, end, text: raw.to_string(), speaker }),
        }
    }
    for (i, s) in out.iter_mut().enumerate() { s.id = format!("seg-{}", i + 1); s.text = s.text.trim().to_string(); }
    out
}
/// sherpa-onnx-offline-speaker-diarization's lines "0.318 -- 6.865 speaker_00" as (start, end, voice) (pure; tested).
pub fn parse_turns(out: &str) -> Vec<(f64, f64, usize)> {
    out.lines().filter_map(|l| {
        let mut it = l.split_whitespace();
        let a = it.next()?.parse().ok()?;
        if it.next()? != "--" { return None; }
        let b = it.next()?.parse().ok()?;
        Some((a, b, it.next()?.strip_prefix("speaker_")?.parse().ok()?))
    }).collect()
}
/// The voice heard most during a word; else the nearest turn less than a second away.
fn voice_at(turns: &[(f64, f64, usize)], start: f64, end: f64) -> Option<usize> {
    let mut best = (0.0, None);
    for &(a, b, v) in turns { let o = end.min(b) - start.max(a); if o > best.0 { best = (o, Some(v)); } }
    best.1.or_else(|| turns.iter().map(|&(a, b, v)| ((start - b).max(a - end), v)).filter(|x| x.0 < 1.0).min_by(|x, y| x.0.total_cmp(&y.0)).map(|x| x.1))
}
fn bench_path(app: &AppHandle) -> Result<PathBuf, String> { Ok(root(app)?.join("bench.json")) }
fn bench(app: &AppHandle) -> HashMap<String, f64> { bench_path(app).ok().and_then(|p| fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }

/// Transcribes a recording with a downloaded Whisper model. Language "auto", "id", "en", or "default" for the one set in
/// Settings > Meeting.
#[tauri::command]
pub async fn ai_transcribe(app: AppHandle, id: String, model: String, language: String, mode: String) -> Result<Transcript, String> {
    feature_on(&app)?;
    let p = part(&model).filter(|p| p.group == "transcript" && p.kind == "model").ok_or("Pick a Whisper model.")?;
    if !installed(&app, p) { return Err("Download that model first.".into()); }
    let exe = engine_exe(&app, "whisper-engine").ok_or("Download the transcript engine first.")?;
    let language = if language == "default" { crate::desktop::preferences(&app).meeting_language } else { language };
    let lang = match language.as_str() { "id" | "en" => language.clone(), _ => "auto".into() };
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    begin("transcript", &id)?;
    let a = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || transcribe(&a, &id, &wav, p, &exe, &lang, &mode)).await.map_err(|e| e.to_string()).and_then(|r| r);
    end(&app);
    r
}
/// Right after a recording is saved: its transcript starts by itself when a transcript package is installed (Settings
/// installs one with the recorder), with this PC's recommended model if present and the language set in Settings.
/// A failure only leaves the Transcribe button in the panel, as before.
pub fn transcribe_after_recording(app: &AppHandle, id: &str) {
    let prefs = crate::desktop::preferences(app);
    if !prefs.feature_meeting || !prefs.feature_local_ai { return; }
    let Some(exe) = engine_exe(app, "whisper-engine") else { return };
    let hw = hardware(app);
    let wanted = recommend(hw.ram_gb, hw.free_ram_gb, hw.cores, hw.avx2).1;
    let model = |p: &&Part| p.group == "transcript" && p.kind == "model" && installed(app, p);
    let Some(p) = part(wanted).filter(model).or_else(|| CATALOG.iter().find(model)) else { return };
    let Some((wav, _)) = crate::meeting::files_of(app, id) else { return };
    let lang = match prefs.meeting_language.as_str() { "id" | "en" => prefs.meeting_language.clone(), _ => "auto".into() };
    // The transcript of the call before (or a summary) may still run, or the next call may be recording: this one waits
    // its turn instead of being skipped.
    while begin("transcript", id).is_err() {
        if !crate::desktop::preferences(app).feature_local_ai { return; }
        std::thread::sleep(Duration::from_secs(3));
    }
    let _ = transcribe(app, id, &wav, p, &exe, &lang, "balanced");
    end(app);
}
/// The "Words to know" from Settings as Whisper's initial prompt: trimmed, no repeats, at most 40 terms and 300
/// characters; None when there are none (pure; tested).
pub fn whisper_prompt(words: &str) -> Option<String> {
    let mut list: Vec<String> = vec![];
    for w in words.split([',', ';', '\n']).map(|w| w.trim().chars().filter(|c| !c.is_control()).collect::<String>()) {
        if !w.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&w)) && list.len() < 40 { list.push(w); }
    }
    let mut out = String::new();
    for w in list { if out.len() + w.len() + 2 > 300 { break; } if !out.is_empty() { out.push_str(", "); } out.push_str(&w); }
    (!out.is_empty()).then(|| out + ".")
}
fn transcribe(app: &AppHandle, id: &str, wav: &Path, p: &Part, exe: &Path, lang: &str, mode: &str) -> Result<Transcript, String> {
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input = work.path().join("in.wav");
    job_set(app, 2, "Preparing audio");
    // 16 kHz for Whisper. With a microphone track: stereo, the app on the left and you on the right, so Whisper's
    // stereo diarization (-di) tells who spoke by which side is louder; the others' echo in your mic stays quieter.
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("converter");
    let (ffmpeg, _locks) = crate::convert::engines::Engine::Ffmpeg.prepare(&cache)?;
    let mic = crate::meeting::audio_of(wav.parent().unwrap_or(Path::new(".")), &format!("{id} (mic)"));
    let mut cmd = Command::new(&ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-protocol_whitelist", "file", "-i"]).arg(wav);
    // shortcut: the app track sets the length (the mic is padded, a longer mic loses its tail); both tracks stop together today.
    if let Some(mic) = &mic { cmd.arg("-i").arg(mic).args(["-filter_complex", "[0:a]aformat=channel_layouts=mono[a];[1:a]aformat=channel_layouts=mono,apad[b];[a][b]amerge=inputs=2"]); }
    let channels = if mic.is_some() { 2.0 } else { 1.0 };
    cmd.args(["-ar", "16000", "-ac", if mic.is_some() { "2" } else { "1" }, "-c:a", "pcm_s16le"]).arg(&input);
    run_worker(app, cmd, threads_for("balanced", 4).1, &|_| {})?;
    let audio_s = fs::metadata(&input).map_or(0.0, |m| (m.len().saturating_sub(44)) as f64 / (32000.0 * channels));
    // The voice package listens to the meeting app's track alone (16 kHz mono); without it the lines keep "Others".
    let voices = voice_parts(app);
    let app_only = match (&voices, &mic) {
        (Some(_), Some(_)) => {
            let p = work.path().join("app.wav");
            let mut c = Command::new(&ffmpeg);
            c.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-protocol_whitelist", "file", "-i"]).arg(wav).args(["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"]).arg(&p);
            run_worker(app, c, threads_for("balanced", 4).1, &|_| {}).ok().map(|_| p)
        }
        (Some(_), None) => Some(input.clone()),
        _ => None,
    };
    let span = if app_only.is_some() { 0.83 } else { 0.93 };
    let hw = hardware(app);
    let (threads, priority) = threads_for(mode, hw.cores);
    let out_base = work.path().join("out");
    let (model, vad) = (file_of(app, p)?, part("whisper-vad").filter(|v| installed(app, v)).map(|v| file_of(app, v)).transpose()?);
    let exe_dir = exe.parent().unwrap_or(Path::new("."));
    let words = whisper_prompt(&crate::desktop::preferences(app).meeting_words);
    // ggml loads its backends from beside whisper-cli (the CPU ones) and from the folder it runs in: run in the GPU
    // folder and it finds ggml-vulkan.dll there too. A PC without a Vulkan driver just stays on the CPU.
    let gpu_files = gpu_dir(app);
    let gpu = gpu_files.as_ref().map(|g| g.0.clone());
    let whisper = |dir: &Path| {
        let mut cmd = Command::new(exe);
        cmd.current_dir(dir).arg("-m").arg(&model).arg("-f").arg(&input).args(["-l", lang, "-t", &threads.to_string(), "-oj", "-pp", "-of"]).arg(&out_base);
        // One word per entry (parse_whisper joins them into lines), so -di names the speaker of each word.
        cmd.args(["-ml", "1", "-sow"]);
        if mic.is_some() { cmd.arg("-di"); }
        if let Some(vad) = &vad { cmd.arg("--vad").arg("-vm").arg(vad); }
        // Names and terms to spell right, for every part VAD cuts the call into.
        if let Some(words) = &words { cmd.arg("--prompt").arg(words).arg("--carry-initial-prompt"); }
        cmd
    };
    job_set(app, 5, "Transcribing");
    let started = Instant::now();
    let progress = |stage: &'static str| move |l: &str| { if let Some(n) = l.split("progress =").nth(1).and_then(|r| r.trim().trim_end_matches('%').trim().parse::<f64>().ok()) { job_set(app, (5.0 + n * span) as u8, stage); } };
    let mut done = run_worker(app, whisper(gpu.as_deref().unwrap_or(exe_dir)), priority, &progress("Transcribing"));
    // The graphics driver failed (out of video memory, a crash): the same job again on the CPU alone.
    if done.is_err() && gpu.is_some() && !STOP_JOB.load(Ordering::SeqCst) {
        job_set(app, 5, "Transcribing on the CPU");
        let mut cmd = whisper(exe_dir); cmd.arg("-ng");
        done = run_worker(app, cmd, priority, &progress("Transcribing on the CPU"));
    }
    done?;
    let raw: Value = serde_json::from_slice(&fs::read(out_base.with_extension("json")).map_err(|_| "The engine wrote no transcript.".to_string())?).map_err(|e| e.to_string())?;
    let turns = match (&voices, &app_only) {
        (Some((dexe, seg, emb)), Some(audio)) => { job_set(app, 90, "Telling voices apart"); diarize(app, dexe, seg, emb, audio, threads, priority) }
        _ => vec![],
    };
    if STOP_JOB.load(Ordering::SeqCst) { return Err("Cancelled.".into()); }
    let segments = parse_whisper(&raw, &turns);
    let language = raw.pointer("/result/language").and_then(Value::as_str).unwrap_or(lang).to_string();
    let processing_s = started.elapsed().as_secs_f64();
    let t = Transcript { id: id.into(), model: p.id.into(), language, created: now_ms(), audio_s, processing_s, segments, edited: false, names: Default::default() };
    fs::write(wav.with_file_name(format!("{id}.transcript.json")), serde_json::to_vec_pretty(&t).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // Real speed on this PC, for the next estimate: seconds of work per second of audio (RTF).
    if audio_s > 5.0 { let mut b = bench(app); b.insert(format!("{}:{mode}", p.id), processing_s / audio_s); if let Ok(bp) = bench_path(app) { let _ = fs::write(bp, serde_json::to_vec(&b).unwrap_or_default()); } }
    Ok(t)
}
/// The voice package's program and its two models, when all of it is installed.
fn voice_parts(app: &AppHandle) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let model = |id: &str| part(id).filter(|p| installed(app, p)).and_then(|p| file_of(app, p).ok());
    Some((engine_exe(app, "speaker-engine")?, model("speaker-seg")?, model("speaker-voice")?))
}
/// Voice turns in the meeting app's track; none when the program fails (the transcript then keeps "Others").
fn diarize(app: &AppHandle, exe: &Path, seg: &Path, emb: &Path, audio: &Path, threads: usize, priority: u32) -> Vec<(f64, f64, usize)> {
    let mut cmd = Command::new(exe);
    cmd.current_dir(exe.parent().unwrap_or(Path::new(".")))
        .arg(format!("--segmentation.pyannote-model={}", seg.display())).arg(format!("--embedding.model={}", emb.display()))
        .arg(format!("--segmentation.num-threads={threads}")).arg(format!("--embedding.num-threads={threads}"))
        // Lower thresholds split one person in two on the test samples.
        .arg("--clustering.cluster-threshold=0.9").arg(audio);
    run_worker(app, cmd, priority, &|_| {}).map(|o| parse_turns(&o)).unwrap_or_default()
}
#[tauri::command]
pub fn ai_transcript(app: AppHandle, id: String) -> Result<Option<Value>, String> {
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let read = |ext: &str| fs::read(wav.with_file_name(format!("{id}.{ext}.json"))).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    Ok(read("transcript").map(|t| json!({ "transcript": t, "summary": read("summary") })))
}
/// Saves your edits to a transcript: the same segments (ids and times), new words.
#[tauri::command]
pub fn ai_transcript_save(app: AppHandle, id: String, segments: Vec<Segment>) -> Result<(), String> {
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let path = wav.with_file_name(format!("{id}.transcript.json"));
    let mut t: Transcript = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if segments.len() != t.segments.len() || segments.iter().zip(&t.segments).any(|(a, b)| a.id != b.id) { return Err("The transcript changed. Reopen it.".into()); }
    for (s, new) in t.segments.iter_mut().zip(segments) { s.text = new.text.chars().take(4000).collect(); }
    t.edited = true;
    fs::write(path, serde_json::to_vec_pretty(&t).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Names a speaker of a transcript; an empty name brings back "Person N" (pure; tested).
pub fn name_speaker(t: &mut Transcript, speaker: &str, name: &str) -> Result<(), String> {
    if speaker == "me" || !t.segments.iter().any(|s| s.speaker == speaker) { return Err("That speaker is not in this transcript.".into()); }
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
    if name.is_empty() { t.names.remove(speaker); } else { t.names.insert(speaker.into(), name); }
    Ok(())
}
/// Renames a speaker everywhere in one transcript (its lines, exports and the next summary).
#[tauri::command]
pub fn ai_speaker_name(app: AppHandle, id: String, speaker: String, name: String) -> Result<(), String> {
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let path = wav.with_file_name(format!("{id}.transcript.json"));
    let mut t: Transcript = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    name_speaker(&mut t, &speaker, &name)?;
    fs::write(path, serde_json::to_vec_pretty(&t).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- export
fn stamp(s: f64, comma: bool) -> String { let ms = (s * 1000.0).round().max(0.0) as u64; format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, if comma { ',' } else { '.' }, ms % 1000) }
/// "You: " / "Others: " / "Person 2: " (or the name given to that speaker) before a line, when the speaker is known.
fn who(names: &std::collections::BTreeMap<String, String>, s: &Segment) -> String {
    if let Some(n) = names.get(&s.speaker) { return format!("{n}: "); }
    match s.speaker.as_str() { "me" => "You: ".into(), "others" => "Others: ".into(), p => p.strip_prefix('p').and_then(|n| n.parse::<u32>().ok()).map_or(String::new(), |n| format!("Person {n}: ")) }
}
fn mmss(s: f64) -> String { let s = s.max(0.0) as u64; if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) } }
/// A transcript as TXT, Markdown, SRT, VTT or JSON (pure; tested).
pub fn export_text(t: &Transcript, title: &str, format: &str) -> Option<String> {
    Some(match format {
        "txt" => t.segments.iter().map(|s| format!("[{}] {}{}", mmss(s.start), who(&t.names, s), s.text)).collect::<Vec<_>>().join("\n") + "\n",
        "md" => format!("# {title}\n\n_Transcript made on this PC by {} ({}). May contain mistakes._\n\n", t.model, t.language) + &t.segments.iter().map(|s| format!("**{}** {}{}", mmss(s.start), who(&t.names, s), s.text)).collect::<Vec<_>>().join("\n\n") + "\n",
        "srt" => t.segments.iter().enumerate().map(|(i, s)| format!("{}\n{} --> {}\n{}{}\n", i + 1, stamp(s.start, true), stamp(s.end, true), who(&t.names, s), s.text)).collect::<Vec<_>>().join("\n"),
        "vtt" => "WEBVTT\n\n".to_string() + &t.segments.iter().map(|s| format!("{} --> {}\n{}{}\n", stamp(s.start, false), stamp(s.end, false), who(&t.names, s), s.text)).collect::<Vec<_>>().join("\n"),
        "json" => serde_json::to_string_pretty(t).ok()?,
        _ => return None,
    })
}
/// Saves the transcript next to the recording in that format and shows it in Explorer.
#[tauri::command]
pub fn ai_export(app: AppHandle, id: String, format: String) -> Result<String, String> {
    let (wav, meta) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let t: Transcript = serde_json::from_slice(&fs::read(wav.with_file_name(format!("{id}.transcript.json"))).map_err(|_| "Transcribe it first.".to_string())?).map_err(|e| e.to_string())?;
    let title = fs::read(meta).ok().and_then(|b| serde_json::from_slice::<crate::meeting::Meta>(&b).ok()).map(|m| m.title).unwrap_or_else(|| id.clone());
    let text = export_text(&t, &title, &format).ok_or("Unknown format.")?;
    let out = crate::meeting::unique(wav.parent().unwrap_or(Path::new(".")), &format!("{id} transcript"), &format);
    fs::write(&out, text).map_err(|e| e.to_string())?;
    reveal(&out);
    Ok(out.display().to_string())
}
/// A summary as a tidy Markdown or plain-text file (pure; tested): title, when, who wrote it, then the overview,
/// decisions, to-dos (with owner and deadline), open questions and topics, each with the time it was said.
pub fn summary_text(saved: &Value, segs: &[Segment], title: &str, recorded: &str, by: &str, md: bool) -> String {
    let s = &saved["summary"];
    let at = |v: &Value| v.get("supportingSegmentIds").and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str)
        .and_then(|id| segs.iter().find(|x| x.id == id)).map(|x| format!(" ({})", mmss(x.start))).unwrap_or_default();
    let list = |k: &str| s.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let mut out = String::new();
    let head = |out: &mut String, h: &str| { out.push_str(&if md { format!("\n## {h}\n\n") } else { format!("\n{}\n{}\n", h.to_uppercase(), "-".repeat(h.len())) }); };
    if md { out.push_str(&format!("# {title}\n\n")); } else { out.push_str(&format!("{title}\n{}\n", "=".repeat(title.chars().count().max(3)))); }
    let note = format!("Summary of the meeting{}. Made by {by}. AI can be wrong: check it against the transcript.", if recorded.is_empty() { String::new() } else { format!(" on {recorded}") });
    out.push_str(&if md { format!("_{note}_\n") } else { format!("{note}\n") });
    let overview = str_of(s, "overview");
    if !overview.is_empty() { head(&mut out, "Overview"); out.push_str(&overview); out.push('\n'); }
    let decisions = list("decisions");
    if !decisions.is_empty() {
        head(&mut out, "Decisions");
        for d in &decisions { let unsure = if d.get("confidence").and_then(Value::as_str) == Some("low") { " (unsure)" } else { "" }; out.push_str(&format!("- {}{unsure}{}\n", str_of(d, "text"), at(d))); }
    }
    let todo = list("actionItems");
    if !todo.is_empty() {
        head(&mut out, "To do");
        for a in &todo {
            let who: Vec<String> = ["owner", "deadline"].iter().map(|k| str_of(a, k)).filter(|x| !x.is_empty()).collect();
            let who = if who.is_empty() { String::new() } else { format!(" ({})", who.join(", ")) };
            out.push_str(&format!("- [ ] {}{who}{}\n", str_of(a, "task"), at(a)));
        }
    }
    let questions = list("openQuestions");
    if !questions.is_empty() { head(&mut out, "Open questions"); for q in &questions { out.push_str(&format!("- {}{}\n", str_of(q, "text"), at(q))); } }
    let topics = list("topics");
    if !topics.is_empty() { head(&mut out, "Topics"); for t in &topics { out.push_str(&format!("- {}{}\n", str_of(t, "title"), at(t))); } }
    if decisions.is_empty() && todo.is_empty() && questions.is_empty() && overview.is_empty() { out.push_str("\nNothing clear came out of this meeting. Read the transcript.\n"); }
    out
}
/// "2026-11-17 16-13 Zoom" -> "17 Nov 2026, 16:13" (the recording's name starts with its local start time).
fn recorded_on(id: &str) -> String {
    let b = id.as_bytes();
    if id.len() < 16 || b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b'-' || !id[..16].bytes().enumerate().all(|(i, c)| [4, 7, 10, 13].contains(&i) || c.is_ascii_digit()) { return String::new(); }
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = id[5..7].parse::<usize>().ok().and_then(|m| M.get(m.wrapping_sub(1))).copied().unwrap_or("");
    format!("{} {month} {}, {}:{}", id[8..10].trim_start_matches('0'), &id[..4], &id[11..13], &id[14..16])
}
/// Saves the summary next to the recording as Markdown (md) or text (txt) and shows it in Explorer.
#[tauri::command]
pub fn ai_export_summary(app: AppHandle, id: String, format: String) -> Result<String, String> {
    let md = match format.as_str() { "md" => true, "txt" => false, _ => return Err("Unknown format.".into()) };
    let (wav, text) = summary_doc(&app, &id, md)?;
    let out = crate::meeting::unique(wav.parent().unwrap_or(Path::new(".")), &format!("{id} summary"), &format);
    fs::write(&out, text).map_err(|e| e.to_string())?;
    reveal(&out);
    Ok(out.display().to_string())
}
/// Copies the summary as plain text (headings, to-dos with owner and deadline), for a chat or an e-mail.
#[tauri::command]
pub fn ai_copy_summary(app: AppHandle, id: String) -> Result<(), String> { crate::board::board_put_text(summary_doc(&app, &id, false)?.1) }
/// A recording's saved summary as a document (see `summary_text`), with the recording's path.
fn summary_doc(app: &AppHandle, id: &str, md: bool) -> Result<(PathBuf, String), String> {
    let (wav, meta) = crate::meeting::files_of(app, id).ok_or("That recording is gone.")?;
    let saved: Value = serde_json::from_slice(&fs::read(wav.with_file_name(format!("{id}.summary.json"))).map_err(|_| "Summarize it first.".to_string())?).map_err(|e| e.to_string())?;
    let t: Transcript = fs::read(wav.with_file_name(format!("{id}.transcript.json"))).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let title = fs::read(meta).ok().and_then(|b| serde_json::from_slice::<crate::meeting::Meta>(&b).ok()).map(|m| m.title).filter(|x| !x.trim().is_empty()).unwrap_or_else(|| id.to_string());
    let model = saved.get("model").and_then(Value::as_str).unwrap_or("");
    let by = match saved.get("cloud").and_then(Value::as_str) {
        Some(c) => format!("{c} ({}, cloud AI)", model.split_once(':').map_or(model, |x| x.1)),
        None => format!("{} on this PC", part(model).map_or(model, |p| p.name)),
    };
    let text = summary_text(&saved, &t.segments, &title, &recorded_on(id), &by, md);
    Ok((wav, text))
}
/// Copies the transcript (or a prompt with it, for an AI chat of your choice) to the clipboard. Nothing is sent anywhere.
#[tauri::command]
pub fn ai_copy(app: AppHandle, id: String, external: bool) -> Result<(), String> {
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let t: Transcript = serde_json::from_slice(&fs::read(wav.with_file_name(format!("{id}.transcript.json"))).map_err(|_| "Transcribe it first.".to_string())?).map_err(|e| e.to_string())?;
    let body = export_text(&t, &id, "txt").unwrap_or_default();
    let text = if external { format!("Summarize this meeting transcript. List decisions, action items (owner and deadline only when said), and open questions, each with its [mm:ss] time.\n\n{body}") } else { body };
    crate::board::board_put_text(text)
}

// ---------------------------------------------------------------- summary
/// The output the model must give (llama.cpp turns it into a grammar, so the reply is always this JSON).
pub(crate) fn summary_schema() -> Value {
    let ids = json!({ "type": "array", "items": { "type": "string" }, "maxItems": 6 });
    let nullable = json!({ "type": ["string", "null"] });
    json!({ "type": "object", "required": ["overview", "topics", "decisions", "actionItems", "openQuestions"], "properties": {
        "overview": { "type": "string" },
        "topics": { "type": "array", "maxItems": 8, "items": { "type": "object", "required": ["title", "supportingSegmentIds"], "properties": { "title": { "type": "string" }, "supportingSegmentIds": ids } } },
        "decisions": { "type": "array", "maxItems": 8, "items": { "type": "object", "required": ["text", "supportingSegmentIds", "confidence"], "properties": { "text": { "type": "string" }, "supportingSegmentIds": ids, "confidence": { "enum": ["low", "medium", "high"] } } } },
        "actionItems": { "type": "array", "maxItems": 10, "items": { "type": "object", "required": ["task", "owner", "deadline", "supportingSegmentIds"], "properties": { "task": { "type": "string" }, "owner": nullable, "deadline": nullable, "supportingSegmentIds": ids } } },
        "openQuestions": { "type": "array", "maxItems": 8, "items": { "type": "object", "required": ["text", "supportingSegmentIds"], "properties": { "text": { "type": "string" }, "supportingSegmentIds": ids } } }
    } })
}
/// Segments in pieces that fit the model's context (about `max_chars` of text each). Pure; tested.
pub fn chunks(segs: &[Segment], max_chars: usize) -> Vec<Vec<Segment>> {
    let mut out: Vec<Vec<Segment>> = vec![]; let mut cur = vec![]; let mut n = 0;
    for s in segs { let len = s.text.len() + 16; if n + len > max_chars && !cur.is_empty() { out.push(std::mem::take(&mut cur)); n = 0; } n += len; cur.push(s.clone()); }
    if !cur.is_empty() { out.push(cur); }
    out
}
/// What every summarizer is told, on this PC or in the cloud.
pub(crate) const SUMMARY_RULES: &str = "You summarize meeting transcripts faithfully. Use only what the transcript says. Write in the transcript's language. A line may start with You: (the person who recorded), Others: (the other side of the call) or Person N: (one voice on the other side, numbered by voice, not a name), or the name of a person. Cite the [seg-N] ids that support each point. owner and deadline must be null unless a name or a date is said explicitly in the cited lines. Never invent names, dates or decisions. Put each agreement in decisions, each task someone will do in actionItems, each unanswered question in openQuestions, and the main subjects in topics. owner is who will do the task: a name said in the lines, or the label of the person who says they will do it (You, Person 2). deadline is the day or date said for it. List each task once. A question that a later line answers is not an open question. Keep names, products and terms exactly as written in the transcript; never expand or explain them. Leave a list empty only when the transcript has none.";
/// Transcript lines as a summarizer reads them: "[seg-3] (01:05) Budi: …".
pub(crate) fn transcript_lines(segs: &[Segment], names: &std::collections::BTreeMap<String, String>) -> String {
    segs.iter().map(|s| format!("[{}] ({}) {}{}\n", s.id, mmss(s.start), who(names, s), s.text)).collect()
}
/// What every summarizer is asked after the lines: the JSON, in the transcript's own language. Without it a small
/// model drifts into the English of the rules, and its days and names then no longer match the lines (pure; tested).
pub(crate) fn ask_text(lines: &str, language: &str) -> String {
    let lang = match language { "id" => "Indonesian", "en" => "English", _ => "the transcript's language" };
    format!("Transcript:\n{lines}\nReturn the JSON summary. Write every text in it in {lang}, with days, dates and names as said in the transcript.")
}
/// ChatML; the empty think block is what Qwen3.5's own template writes to answer without thinking first.
fn prompt(segs: &[Segment], names: &std::collections::BTreeMap<String, String>, language: &str) -> String {
    format!("<|im_start|>system\n{SUMMARY_RULES}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n", ask_text(&transcript_lines(segs, names), language))
}
/// A long meeting summarized piece by piece: each piece is checked against its own lines, then the joined summary
/// once more against the whole transcript. What a piece's check found stays said in the final warnings.
pub(crate) struct Pieces { merged: Value, overviews: Vec<String>, warnings: Vec<Value>, names: std::collections::BTreeMap<String, String> }
impl Pieces {
    pub(crate) fn new(names: &std::collections::BTreeMap<String, String>) -> Self { Self { merged: json!({ "overview": "", "topics": [], "decisions": [], "actionItems": [], "openQuestions": [] }), overviews: vec![], warnings: vec![], names: names.clone() } }
    pub(crate) fn add(&mut self, raw: &Value, segs: &[Segment]) {
        let v = validate_summary(raw, segs, &self.names);
        for k in ["topics", "decisions", "actionItems", "openQuestions"] { if let (Some(a), Some(b)) = (self.merged[k].as_array_mut(), v[k].as_array()) { a.extend(b.iter().cloned()); } }
        let o = v["overview"].as_str().unwrap_or("").to_string(); if !o.is_empty() { self.overviews.push(o); }
        for w in v["warnings"].as_array().into_iter().flatten() { if !self.warnings.contains(w) { self.warnings.push(w.clone()); } }
    }
    pub(crate) fn finish(mut self, all: &[Segment]) -> Value {
        self.merged["overview"] = json!(self.overviews.join(" ").chars().take(1200).collect::<String>());
        let mut checked = validate_summary(&self.merged, all, &self.names);
        if let Some(list) = checked["warnings"].as_array_mut() { for w in self.warnings { if !list.contains(&w) { list.push(w); } } }
        checked
    }
}
/// The day or date a line gives ("hari Rabu", "paling lambat Jumat", "tanggal 12 Desember", "minggu ini"), as said;
/// "today" is not a deadline (pure; tested).
pub fn deadline_in(text: &str) -> Option<String> {
    const DAYS: [&str; 13] = ["senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
    const MONTHS: [&str; 24] = ["januari", "februari", "maret", "april", "mei", "juni", "juli", "agustus", "september", "oktober", "november", "desember",
        "january", "february", "march", "may", "june", "july", "august", "october", "december", "jan", "feb", "dec"];
    const PHRASES: [&str; 10] = ["minggu depan", "minggu ini", "bulan depan", "akhir bulan", "akhir minggu", "besok", "lusa", "next week", "this week", "tomorrow"];
    let cap = |w: &str| { let mut c = w.chars(); c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect()) };
    let l = text.to_lowercase();
    let words: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let digits = |w: &str| w.chars().all(|c| c.is_ascii_digit()) && w.len() <= 2;
    for (i, w) in words.iter().enumerate() {
        if *w == "tanggal" { if let Some(n) = words.get(i + 1).filter(|n| digits(n)) { let m = words.get(i + 2).filter(|m| MONTHS.contains(m)).map(|m| format!(" {}", cap(m))).unwrap_or_default(); return Some(format!("tanggal {n}{m}")); } }
        if digits(w) { if let Some(m) = words.get(i + 1).filter(|m| MONTHS.contains(m)) { return Some(format!("{w} {}", cap(m))); } }
        if DAYS.contains(w) { return Some(cap(w)); }
    }
    PHRASES.iter().find(|p| l.contains(*p)).map(|p| p.to_string())
}
/// Words that carry meaning, for matching a point to the lines it cites: lowercase, 3+ letters or a number, without
/// common English and Indonesian filler words. Pure; tested.
pub fn content_words(s: &str) -> std::collections::HashSet<String> {
    const STOP: [&str; 58] = ["the", "and", "for", "that", "this", "with", "will", "was", "are", "has", "have", "had", "but", "not", "from", "they", "we", "you", "our", "who", "what", "when", "which", "there", "their", "been", "about", "next", "also", "all", "can", "its", "into", "than", "then", "yang", "dan", "untuk", "dengan", "ini", "itu", "akan", "ada", "kita", "kami", "juga", "dari", "pada", "sudah", "belum", "saja", "atau", "bisa", "tidak", "karena", "jadi", "agar", "oleh"];
    // "targetnya" is "target": Indonesian's -nya ("its, the") would hide a shared word.
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| (w.chars().count() >= 3 || w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty()) && !STOP.contains(w))
        .map(|w| if w.chars().count() > 5 { w.strip_suffix("nya").unwrap_or(w) } else { w }.to_string()).collect()
}
/// A point without the "[seg-2] (00:06)" marks a small model copies from the prompt. Pure; tested.
pub fn strip_marks(s: &str) -> String {
    let mut t = s.trim();
    loop {
        let before = t;
        if let Some(r) = t.strip_prefix('[') { if let Some(end) = r.find(']') { if r[..end].starts_with("seg-") { t = r[end + 1..].trim_start(); } } }
        if let Some(r) = t.strip_prefix('(') { if let Some(end) = r.find(')') { if r[..end].chars().all(|c| c.is_ascii_digit() || c == ':') && end > 0 { t = r[end + 1..].trim_start(); } } }
        if t == before { break; }
    }
    t.to_string()
}
/// Whether a line backs a point: it shares two meaning words with it (one, for a point of one or two). Pure; tested.
pub fn backs(point: &str, line: &str) -> bool {
    let (p, l) = (content_words(point), content_words(line));
    let need = if p.len() >= 3 { 2 } else { p.len().min(1) };
    need > 0 && p.intersection(&l).count() >= need
}
/// Keeps only what the transcript backs (pure; tested). A cited line must share meaning words with the point (small
/// models cite seg-1..6 by habit); when none does, the best matching line is cited instead, or none. Owner and
/// deadline must be said around a cited line (two lines before it to one after, where a task is handed out and taken:
/// "Budi, bisa tolong…" / "Saya kumpulkan… Jumat"), or the owner is that line's speaker (You, Person 2, a given name).
/// A "decision" or "task" that is a question moves to the open questions; a question answered right after (without
/// "belum", "nanti"…) is not open. The same point twice, word for word or a task in other words, is kept once with
/// what each copy knew. The warning is always there.
pub fn validate_summary(raw: &Value, segs: &[Segment], names: &std::collections::BTreeMap<String, String>) -> Value {
    let known: HashMap<&str, &str> = segs.iter().map(|s| (s.id.as_str(), s.text.as_str())).collect();
    let mut warnings = vec![json!("AI summary may contain mistakes; verify with the transcript.")];
    let mut unbacked = false;
    let fixed = std::cell::Cell::new(false);
    let ids = |v: &Value, point: &str| -> Vec<String> {
        let cited: Vec<&str> = v.get("supportingSegmentIds").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let mut l: Vec<String> = vec![];
        for i in &cited { if known.get(i).is_some_and(|t| backs(point, t)) && !l.iter().any(|x| x == i) { l.push(i.to_string()); } }
        if l.len() != cited.len() { fixed.set(true); }
        if l.is_empty() {
            // The line that shares the most meaning words, if any backs the point at all.
            let words = content_words(point);
            if let Some(best) = segs.iter().filter(|s| backs(point, &s.text)).max_by_key(|s| content_words(&s.text).intersection(&words).count()) { l.push(best.id.clone()); }
        }
        l.truncate(6);
        l
    };
    let at = |id: &str| segs.iter().position(|s| s.id == id);
    let said = |word: &Value, cited: &[String]| -> Value {
        let Some(w) = word.as_str().map(str::trim).filter(|w| !w.is_empty() && !w.eq_ignore_ascii_case("null")) else { return Value::Null };
        let lw = w.to_lowercase();
        let found = cited.iter().filter_map(|i| at(i)).any(|k| {
            let speaker = who(names, &segs[k]).trim_end_matches(": ").to_lowercase();
            (!speaker.is_empty() && speaker == lw) || segs[k.saturating_sub(2)..(k + 2).min(segs.len())].iter().any(|s| s.text.to_lowercase().contains(&lw))
        });
        if found { json!(w) } else { Value::Null }
    };
    let txt = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(|s| strip_marks(s).chars().take(400).collect::<String>()).unwrap_or_default();
    let norm = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let mut seen = std::collections::HashSet::new();
    let mut fresh = |t: &str| !t.is_empty() && seen.insert(norm(t));
    let items = |key: &str| raw.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let is_question = |t: &str| t.trim_end().ends_with('?');
    let mut questions = vec![];
    let mut decisions = vec![];
    let mut actions = vec![];
    let mut topics = vec![];
    for v in items("decisions") {
        let t = txt(&v, "text"); if !fresh(&t) { continue; }
        let c = ids(&v, &t);
        if is_question(&t) { questions.push(json!({ "text": t, "supportingSegmentIds": c })); continue; }
        let conf = if c.is_empty() { unbacked = true; "low".to_string() } else { v.get("confidence").and_then(Value::as_str).filter(|x| ["low", "medium", "high"].contains(x)).unwrap_or("medium").to_string() };
        decisions.push(json!({ "text": t, "supportingSegmentIds": c, "confidence": conf }));
    }
    for v in items("actionItems") {
        let t = txt(&v, "task"); if !fresh(&t) { continue; }
        let c = ids(&v, &t);
        if is_question(&t) { questions.push(json!({ "text": t, "supportingSegmentIds": c })); continue; }
        let (mut owner, mut deadline) = (said(v.get("owner").unwrap_or(&Value::Null), &c), said(v.get("deadline").unwrap_or(&Value::Null), &c));
        // What a small model leaves out but the cited line says: "saya …" is its speaker, a day or date is the deadline.
        let lines: Vec<usize> = c.iter().filter_map(|i| at(i)).collect();
        if owner.is_null() {
            if let Some(k) = lines.iter().copied().find(|&k| segs[k].text.to_lowercase().split(|ch: char| !ch.is_alphanumeric()).any(|w| ["saya", "aku", "gue", "gw", "i"].contains(&w))) {
                let label = who(names, &segs[k]).trim_end_matches(": ").to_string();
                if !label.is_empty() { owner = json!(label); }
            }
        }
        if deadline.is_null() { if let Some(d) = lines.iter().find_map(|&k| deadline_in(&segs[k].text)) { deadline = json!(d); } }
        // The same task in other words (small models list "do X" and "finish X"): one, with what each copy knew.
        if let Some(a) = actions.iter_mut().find(|a: &&mut Value| { let p = a["task"].as_str().unwrap_or(""); backs(&t, p) || backs(p, &t) }) {
            if a["owner"].is_null() { a["owner"] = owner; }
            if a["deadline"].is_null() { a["deadline"] = deadline; }
            continue;
        }
        if c.is_empty() { unbacked = true; }
        actions.push(json!({ "task": t, "owner": owner, "deadline": deadline, "supportingSegmentIds": c }));
    }
    for v in items("openQuestions") { let t = txt(&v, "text"); if fresh(&t) { let c = ids(&v, &t); questions.push(json!({ "text": t, "supportingSegmentIds": c })); } }
    // A question answered in the next lines is not open; "belum pasti", "nanti dicek" are no answers.
    const UNSURE: [&str; 12] = ["belum", "tidak tahu", "nggak tahu", "gak tahu", "nanti", "cek dulu", "tanyakan", "not sure", "not yet", "don't know", "later", "check"];
    questions.retain(|q| {
        let text = q["text"].as_str().unwrap_or("");
        // From the last cited line that asks (a model may cite the answer too), else the last cited line.
        let cited: Vec<usize> = q["supportingSegmentIds"].as_array().into_iter().flatten().filter_map(Value::as_str).filter_map(|i| at(i)).collect();
        let last = cited.iter().copied().filter(|&k| segs[k].text.trim_end().ends_with('?')).max().or_else(|| cited.iter().copied().max());
        !last.is_some_and(|k| segs[k + 1..(k + 4).min(segs.len())].iter().any(|s| {
            let l = s.text.to_lowercase();
            !l.trim_end().ends_with('?') && backs(text, &s.text) && !UNSURE.iter().any(|u| l.contains(u))
        }))
    });
    for v in items("topics") { let t = txt(&v, "title"); if fresh(&t) { let c = ids(&v, &t); topics.push(json!({ "title": t, "supportingSegmentIds": c })); } }
    if unbacked { warnings.push(json!("Some points cite no part of the transcript.")); }
    if fixed.get() { warnings.push(json!("Sora removed or replaced citations that did not match the transcript.")); }
    json!({ "overview": txt(raw, "overview"), "topics": topics, "decisions": decisions, "actionItems": actions, "openQuestions": questions, "warnings": warnings })
}
/// The first complete JSON object in a model's reply (pure; tested).
pub fn first_json(s: &str) -> Option<Value> {
    let start = s.find('{')?;
    let mut depth = 0i32; let mut in_str = false; let mut esc = false;
    for (i, c) in s[start..].char_indices() {
        if in_str { if esc { esc = false } else if c == '\\' { esc = true } else if c == '"' { in_str = false } continue; }
        match c { '"' => in_str = true, '{' => depth += 1, '}' => { depth -= 1; if depth == 0 { return serde_json::from_str(&s[start..=start + i]).ok(); } } _ => {} }
    }
    None
}
#[tauri::command]
pub async fn ai_summarize(app: AppHandle, id: String, model: String, mode: String) -> Result<Value, String> {
    feature_on(&app)?;
    let p = part(&model).filter(|p| p.group == "summary" && p.kind == "model").ok_or("Pick a summary model.")?;
    if !installed(&app, p) { return Err("Download that model first.".into()); }
    let exe = engine_exe(&app, "llama-engine").ok_or("Download the summary engine first.")?;
    let (wav, _) = crate::meeting::files_of(&app, &id).ok_or("That recording is gone.")?;
    let t: Transcript = serde_json::from_slice(&fs::read(wav.with_file_name(format!("{id}.transcript.json"))).map_err(|_| "Transcribe it first: the summary is made from the transcript.".to_string())?).map_err(|e| e.to_string())?;
    if t.segments.is_empty() { return Err("The transcript is empty.".into()); }
    begin("summary", &id)?;
    let a = app.clone();
    let r = tauri::async_runtime::spawn_blocking(move || summarize(&a, &id, &wav, &t, p, &exe, &mode)).await.map_err(|e| e.to_string()).and_then(|r| r);
    end(&app);
    r
}
fn summarize(app: &AppHandle, id: &str, wav: &Path, t: &Transcript, p: &Part, exe: &Path, mode: &str) -> Result<Value, String> {
    let hw = hardware(app);
    let (threads, priority) = threads_for(mode, hw.cores);
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let parts = chunks(&t.segments, 9000);
    let schema_file = work.path().join("schema.json");
    fs::write(&schema_file, summary_schema().to_string()).map_err(|e| e.to_string())?;
    let mut pieces = Pieces::new(&t.names);
    for (i, segs) in parts.iter().enumerate() {
        if STOP_JOB.load(Ordering::SeqCst) { return Err("Cancelled.".into()); }
        job_set(app, (5 + i * 90 / parts.len()) as u8, &format!("Summarizing part {} of {}", i + 1, parts.len()));
        let pf = work.path().join(format!("p{i}.txt"));
        fs::write(&pf, prompt(segs, &t.names, &t.language)).map_err(|e| e.to_string())?;
        // Small models can loop until the token limit cuts the JSON: a repeat penalty, then one more try a little warmer.
        let mut raw = None;
        for temp in ["0.2", "0.5"] {
            let mut cmd = Command::new(exe);
            cmd.current_dir(exe.parent().unwrap_or(Path::new("."))).arg("-m").arg(file_of(app, p)?).arg("-f").arg(&pf)
                .args(["-n", "1500", "-c", "8192", "--temp", temp, "--repeat-penalty", "1.15", "-t", &threads.to_string(), "--no-display-prompt", "-no-cnv", "-jf"]).arg(&schema_file);
            let out = run_worker(app, cmd, priority, &|_| {})?;
            raw = first_json(&out);
            if raw.is_some() { break; }
        }
        let raw = raw.ok_or("The model gave no usable summary. Try again or use a bigger model.")?;
        pieces.add(&raw, segs);
    }
    let out = json!({ "id": id, "model": p.id, "created": now_ms(), "summary": pieces.finish(&t.segments), "aiGenerated": true });
    fs::write(wav.with_file_name(format!("{id}.summary.json")), serde_json::to_vec_pretty(&out).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------- status
#[tauri::command]
pub fn ai_status(app: AppHandle) -> Result<Value, String> {
    let hw = hardware(&app);
    let (tier, w, s, notes) = recommend(hw.ram_gb, hw.free_ram_gb, hw.cores, hw.avx2);
    if let Ok(r) = root(&app) { for id in RETIRED { let _ = fs::remove_file(r.join(format!("{id}.bin"))); let _ = fs::remove_file(r.join(format!("{id}.part"))); } }
    let dl = DOWNLOADS.lock().ok().and_then(|d| d.clone()).unwrap_or_default();
    let b = bench(&app);
    let parts: Vec<Value> = CATALOG.iter().map(|p| json!({ "part": p, "installed": installed(&app, p), "progress": dl.get(p.id), "rtf": b.get(&format!("{}:balanced", p.id)), "warn": hw.free_ram_gb < p.min_ram_gb })).collect();
    Ok(json!({ "on": crate::desktop::preferences(&app).feature_local_ai, "parts": parts, "hardware": hw, "recommend": { "tier": tier, "transcript": w, "summary": s, "notes": notes }, "job": job_view(), "cloud": crate::cloud_ai::state(&app) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seg(i: usize, text: &str) -> Segment { Segment { id: format!("seg-{i}"), start: i as f64 * 5.0, end: i as f64 * 5.0 + 4.5, text: text.into(), ..Default::default() } }
    #[test]
    fn downloads_come_only_from_pinned_hosts() {
        for h in ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com", "huggingface.co", "cdn-lfs.hf.co", "cas-bridge.xethub.hf.co"] { assert!(host_allowed(h), "{h}"); }
        for h in ["evilgithub.com", "github.com.evil.io", "example.org", "hf.co.attacker.net"] { assert!(!host_allowed(h), "{h}"); }
        for p in CATALOG { assert!(p.url.starts_with("https://") && p.sha256.len() == 64 && p.size > 0, "{}", p.id); }
    }
    #[test]
    fn models_fit_the_free_memory() {
        assert_eq!(recommend(3.8, 2.0, 4, true), ("Sora AI Lite", "whisper-base-q5", Some("qwen35-0.8b"), vec![]));
        assert_eq!(recommend(7.8, 4.0, 8, true), ("Sora AI Balanced", "whisper-small", Some("qwen35-2b"), vec![]));
        assert_eq!(recommend(31.8, 12.0, 16, true), ("Sora AI Powerful", "whisper-turbo", Some("qwen35-4b"), vec![]));
        assert_eq!(recommend(15.8, 3.0, 16, true).2, Some("qwen35-2b"), "a smaller summary model when memory is busy");
        assert_eq!(recommend(7.8, 1.0, 8, true).2, None, "no summary model on a nearly full PC");
        assert_eq!(recommend(31.8, 12.0, 2, true).1, "whisper-small", "few threads: one step lower");
        assert_eq!(recommend(31.8, 12.0, 16, false).1, "whisper-base-q5", "no AVX2: Lite");
        for (_, w, q) in TIERS { assert!(part(w).is_some() && part(q).is_some()); }
        assert_eq!(threads_for("eco", 16).0, 4);
        assert_eq!(threads_for("fast", 1).0, 1);
    }
    #[test]
    fn whisper_json_becomes_segments() {
        let v = json!({ "result": { "language": "id" }, "transcription": [
            { "offsets": { "from": 0, "to": 3200 }, "text": " Selamat pagi semua." },
            { "offsets": { "from": 3200, "to": 4000 }, "text": " [BLANK_AUDIO]" },
            { "offsets": { "from": 4000, "to": 9000 }, "text": " Let's start the review." } ] });
        let s = parse_whisper(&v, &[]);
        assert_eq!(s, vec![Segment { id: "seg-1".into(), start: 0.0, end: 3.2, text: "Selamat pagi semua.".into(), ..Default::default() }, Segment { id: "seg-2".into(), start: 4.0, end: 9.0, text: "Let's start the review.".into(), ..Default::default() }]);
    }
    #[test]
    fn words_join_into_lines_per_speaker() {
        // -ml 1 -sow -di: one word per entry. A reply right after the other side starts its own line.
        let w = |a: u32, b: u32, t: &str, sp: &str| json!({ "offsets": { "from": a, "to": b }, "text": t, "speaker": sp });
        let v = json!({ "transcription": [w(150, 380, " Good", "0"), w(380, 880, " morning", "0"), w(880, 9150, " everyone.", "0"),
            w(9150, 10560, " thanks.", "1"), w(10560, 10660, " I", "?"), w(10620, 10860, " have", "1"), w(10860, 11410, " one.", "1"),
            w(14000, 14500, " Next", "1"), w(100, 200, " [BLANK_AUDIO]", "0")] });
        let s: Vec<(String, String)> = parse_whisper(&v, &[]).into_iter().map(|s| (s.speaker, s.text)).collect();
        assert_eq!(s, [("others".into(), "Good morning everyone.".into()), ("me".into(), "thanks. I have one.".into()), ("me".into(), "Next".into())]);
    }
    #[test]
    fn summaries_keep_who_and_when_and_say_each_thing_once() {
        // From the demo meetings: the owner is named a line before ("Budi, bisa tolong…" / "Saya kumpulkan…") or is the
        // speaker; the same task came twice in other words; an answered question was listed as open.
        let s = |i: usize, sp: &str, text: &str| Segment { speaker: sp.into(), ..seg(i, text) };
        let segs = vec![s(1, "p1", "Budi, bisa tolong kumpulkan contoh rekaman yang salah?"), s(2, "p2", "Bisa. Saya kumpulkan lima contoh paling lambat Jumat."),
            s(3, "me", "Oke, saya yang ubah rekomendasinya. Target selesai hari Rabu."), s(4, "me", "Kita perlu ukur hasilnya juga. Targetnya apa, unduhan atau pengikut?"),
            s(5, "p1", "Targetnya 10 ribu unduhan baru di akhir Maret."), s(6, "p2", "Apakah kita ikut pameran teknologi di Jakarta?"), s(7, "p1", "Itu belum pasti, tergantung biaya stan.")];
        let raw = json!({ "overview": "Rapat.", "topics": [], "decisions": [],
            "actionItems": [
                { "task": "Kumpulkan lima contoh rekaman yang salah", "owner": "Budi", "deadline": null, "supportingSegmentIds": ["seg-2"] },
                { "task": "Menyelesaikan pengumpulan contoh rekaman", "owner": null, "deadline": "Jumat", "supportingSegmentIds": ["seg-2"] },
                { "task": "Ubah rekomendasi paket", "owner": "You", "deadline": "Rabu", "supportingSegmentIds": ["seg-3"] } ],
            "openQuestions": [{ "text": "Apa target pengukuran hasil, unduhan atau pengikut?", "supportingSegmentIds": ["seg-4"] }, { "text": "Apakah kita ikut pameran teknologi di Jakarta?", "supportingSegmentIds": ["seg-6"] }] });
        let v = validate_summary(&raw, &segs, &Default::default());
        let todo: Vec<(String, Value, Value)> = v["actionItems"].as_array().unwrap().iter().map(|a| (a["task"].as_str().unwrap().to_string(), a["owner"].clone(), a["deadline"].clone())).collect();
        assert_eq!(todo, [("Kumpulkan lima contoh rekaman yang salah".to_string(), json!("Budi"), json!("Jumat")), ("Ubah rekomendasi paket".to_string(), json!("You"), json!("Rabu"))],
            "the owner said just before or speaking counts; the same task twice is one, with what each copy knew");
        let q: Vec<&str> = v["openQuestions"].as_array().unwrap().iter().map(|q| q["text"].as_str().unwrap()).collect();
        assert_eq!(q, ["Apakah kita ikut pameran teknologi di Jakarta?"], "a question answered right after is not open; 'belum pasti' is no answer");
        let mut named = std::collections::BTreeMap::new(); named.insert("p2".to_string(), "Budi".to_string());
        let raw2 = json!({ "actionItems": [{ "task": "Kumpulkan lima contoh", "owner": "Budi", "deadline": "Jumat", "supportingSegmentIds": ["seg-2"] }] });
        let segs2 = vec![s(1, "p1", "Oke."), s(2, "p2", "Bisa. Saya kumpulkan lima contoh paling lambat Jumat.")];
        assert_eq!(validate_summary(&raw2, &segs2, &named)["actionItems"][0]["owner"], json!("Budi"), "a name given to the speaker counts");
        assert_eq!(validate_summary(&raw2, &segs2, &Default::default())["actionItems"][0]["owner"], json!("Person 2"), "an invented owner is dropped; who says 'saya' is the owner");
    }
    #[test]
    fn words_to_know_become_one_short_prompt() {
        assert_eq!(whisper_prompt(" QRIS, Kopi Nusantara ,qris;\nSora\n"), Some("QRIS, Kopi Nusantara, Sora.".into()));
        assert_eq!(whisper_prompt(" , ;\n"), None);
        let many = (0..100).map(|i| format!("word{i}")).collect::<Vec<_>>().join(",");
        assert!(whisper_prompt(&many).unwrap().len() <= 301);
    }
    #[test]
    fn days_and_dates_are_found_in_a_line() {
        assert_eq!(deadline_in("Oke, saya yang ubah rekomendasinya. Target selesai hari Rabu."), Some("Rabu".into()));
        assert_eq!(deadline_in("Saya kumpulkan lima contoh paling lambat Jumat."), Some("Jumat".into()));
        assert_eq!(deadline_in("Kami sepakat target peluncuran tanggal 12 Desember."), Some("tanggal 12 Desember".into()));
        assert_eq!(deadline_in("Saya hubungi lima kreator minggu ini."), Some("minggu ini".into()));
        assert_eq!(deadline_in("I will send it on Friday."), Some("Friday".into()));
        assert_eq!(deadline_in("Bagus, hari ini cukup."), None, "today is not a deadline");
        assert_eq!(deadline_in("Kita bahas sprint dua minggu ke depan."), None);
    }
    #[test]
    fn owner_and_deadline_come_from_the_line_itself_when_the_model_leaves_them_out() {
        let s = |i: usize, sp: &str, text: &str| Segment { speaker: sp.into(), ..seg(i, text) };
        let segs = vec![s(1, "p1", "Setuju, paket Lite jadi rekomendasi."), s(2, "me", "Oke, saya yang ubah rekomendasinya. Target selesai hari Rabu."),
            s(3, "me", "Kita perlu ukur hasilnya juga. Targetnya apa?"), s(4, "me", "Unduhan atau pengikut?"), s(5, "p1", "Targetnya 10 ribu unduhan baru di akhir Maret.")];
        let raw = json!({ "actionItems": [{ "task": "Ubah rekomendasi paket Lite", "owner": null, "deadline": null, "supportingSegmentIds": ["seg-2"] }],
            "openQuestions": [{ "text": "Targetnya apa? Unduhan atau pengikut?", "supportingSegmentIds": ["seg-4", "seg-5"] }] });
        let v = validate_summary(&raw, &segs, &Default::default());
        assert_eq!((v["actionItems"][0]["owner"].clone(), v["actionItems"][0]["deadline"].clone()), (json!("You"), json!("Rabu")), "'saya' is the speaker; the day is in the line");
        assert!(v["openQuestions"].as_array().unwrap().is_empty(), "the answer is found even when the model cites it too");
    }
    #[test]
    fn summaries_are_asked_in_the_transcripts_language() {
        assert!(ask_text("[seg-1] (00:00) Halo", "id").contains("in Indonesian, with days, dates and names as said"));
        assert!(ask_text("x", "en").contains("in English"));
        assert!(ask_text("x", "auto").contains("in the transcript's language"));
        assert!(prompt(&[seg(1, "Halo")], &Default::default(), "id").contains("[seg-1] (00:05) Halo\n\nReturn the JSON summary. Write every text in it in Indonesian"));
    }
    #[test]
    fn a_turns_first_word_goes_with_its_turn() {
        // Seen on a real transcript: "Saya" went to the speaker before, the rest of the turn to the right one.
        let w = |a: u32, b: u32, t: &str, sp: &str| json!({ "offsets": { "from": a, "to": b }, "text": t, "speaker": sp });
        let v = json!({ "transcription": [w(0, 900, " Target", "1"), w(900, 1600, " Rabu.", "1"), w(1700, 1900, " Saya", "1"), w(1900, 2300, " sudah", "0"), w(2300, 2900, " coba.", "0"),
            w(4000, 4400, " Bisa.", "1"), w(4500, 4900, " Oke", "0"), w(4900, 5300, " deh.", "0")] });
        let s: Vec<(String, String)> = parse_whisper(&v, &[]).into_iter().map(|s| (s.speaker, s.text)).collect();
        assert_eq!(s, [("me".into(), "Target Rabu.".into()), ("others".into(), "Saya sudah coba.".into()), ("me".into(), "Bisa.".into()), ("others".into(), "Oke deh.".into())], "a one-word answer with its full stop stays");
    }
    #[test]
    fn diarization_output_becomes_turns() {
        let out = "Started\n0.318 -- 6.865 speaker_00\n7.017 -- 10.679 speaker_01\nDuration : 10.7 s\n";
        assert_eq!(parse_turns(out), [(0.318, 6.865, 0), (7.017, 10.679, 1)]);
    }
    #[test]
    fn voices_split_others_into_people() {
        let w = |a: u32, b: u32, t: &str, sp: Option<&str>| { let mut v = json!({ "offsets": { "from": a, "to": b }, "text": t }); if let Some(sp) = sp { v["speaker"] = json!(sp); } v };
        let v = json!({ "transcription": [w(0, 900, " Halo", Some("0")), w(900, 1800, " semua.", Some("0")), w(2500, 3000, " Ya.", Some("1")), w(4000, 4800, " Oke", Some("0")), w(4800, 5200, " deh.", Some("?"))] });
        let turns = [(0.0, 2.0, 3), (3.8, 6.0, 1)];
        let s: Vec<(String, String)> = parse_whisper(&v, &turns).into_iter().map(|s| (s.speaker, s.text)).collect();
        assert_eq!(s, [("p1".into(), "Halo semua.".into()), ("me".into(), "Ya.".into()), ("p2".into(), "Oke deh.".into())]);
        let one: Vec<String> = parse_whisper(&v, &[(0.0, 6.0, 4)]).into_iter().map(|s| s.speaker).collect();
        assert_eq!(one, ["others", "me", "others"], "one voice on the app's side stays Others");
        let mono = json!({ "transcription": [w(0, 900, " A.", None), w(4000, 4500, " B.", None)] });
        let s: Vec<String> = parse_whisper(&mono, &turns).into_iter().map(|s| s.speaker).collect();
        assert_eq!(s, ["p1", "p2"], "one track: every line gets a person");
        let mut a = seg(1, "Halo"); a.speaker = "p2".into();
        assert_eq!(export_text(&Transcript { segments: vec![a], ..Default::default() }, "M", "txt").unwrap(), "[00:05] Person 2: Halo\n");
    }
    #[test]
    #[cfg_attr(all(not(win), windows), ignore = "the macOS freeze uses kill -STOP, which a stubs run on Windows lacks")]
    fn a_frozen_worker_waits() {
        // ping -n 3 takes about two seconds; frozen, it is still running after three.
        let mut c = Command::new("ping").args([if cfg!(windows) { "-n" } else { "-c" }, "3", "127.0.0.1"]).stdout(Stdio::null()).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        freeze(c.id(), true);
        std::thread::sleep(Duration::from_secs(3));
        assert!(c.try_wait().unwrap().is_none(), "frozen");
        freeze(c.id(), false);
        let t = Instant::now();
        while c.try_wait().unwrap().is_none() { assert!(t.elapsed() < Duration::from_secs(10), "resumes"); std::thread::sleep(Duration::from_millis(100)); }
    }
    #[test]
    fn summaries_export_tidy() {
        let segs = vec![seg(1, "Kita sepakat rilis versi dua"), seg(2, "Budi kirim laporan Jumat"), seg(3, "Kapan demo?")];
        let saved = json!({ "summary": { "overview": "Rapat mingguan.", "topics": [{ "title": "Rilis", "supportingSegmentIds": ["seg-1"] }],
            "decisions": [{ "text": "Rilis versi dua", "supportingSegmentIds": ["seg-1"], "confidence": "medium" }, { "text": "Pakai Vulkan", "supportingSegmentIds": [], "confidence": "low" }],
            "actionItems": [{ "task": "Kirim laporan", "owner": "Budi", "deadline": "Jumat", "supportingSegmentIds": ["seg-2"] }, { "task": "Cek GPU", "owner": null, "deadline": null, "supportingSegmentIds": [] }],
            "openQuestions": [{ "text": "Kapan demo?", "supportingSegmentIds": ["seg-3"] }], "warnings": [] } });
        let md = summary_text(&saved, &segs, "Weekly sync", "17 Nov 2026, 10:00", "Qwen3.5 2B on this PC", true);
        assert_eq!(md, "# Weekly sync\n\n_Summary of the meeting on 17 Nov 2026, 10:00. Made by Qwen3.5 2B on this PC. AI can be wrong: check it against the transcript._\n\n## Overview\n\nRapat mingguan.\n\n## Decisions\n\n- Rilis versi dua (00:05)\n- Pakai Vulkan (unsure)\n\n## To do\n\n- [ ] Kirim laporan (Budi, Jumat) (00:10)\n- [ ] Cek GPU\n\n## Open questions\n\n- Kapan demo? (00:15)\n\n## Topics\n\n- Rilis (00:05)\n");
        let txt = summary_text(&saved, &segs, "Weekly sync", "", "Claude", false);
        assert!(txt.starts_with("Weekly sync\n===========\nSummary of the meeting. Made by Claude."));
        assert!(txt.contains("\nTO DO\n-----\n- [ ] Kirim laporan (Budi, Jumat) (00:10)\n"));
        assert!(summary_text(&json!({ "summary": {} }), &[], "M", "", "x", true).contains("Nothing clear came out"));
        assert_eq!(recorded_on("2026-11-07 09-05 Zoom"), "7 Nov 2026, 09:05");
        assert_eq!(recorded_on("Rapat"), "");
    }
    #[test]
    fn speakers_can_be_named() {
        let mut a = seg(1, "Halo"); a.speaker = "p1".into();
        let mut b = seg(2, "Oke"); b.speaker = "me".into();
        let mut t = Transcript { segments: vec![a, b, seg(3, "Ya")], ..Default::default() };
        name_speaker(&mut t, "p1", "  Budi\n").unwrap();
        assert_eq!(export_text(&t, "M", "txt").unwrap(), "[00:05] Budi: Halo\n[00:10] You: Oke\n[00:15] Ya\n");
        assert!(name_speaker(&mut t, "me", "Alex").is_err(), "You stays You");
        assert!(name_speaker(&mut t, "p7", "X").is_err(), "only speakers in the transcript");
        name_speaker(&mut t, "p1", " ").unwrap();
        assert!(t.names.is_empty(), "an empty name brings Person 1 back");
        assert!(!serde_json::to_string(&t).unwrap().contains("names"), "old transcripts keep their shape");
    }
    #[test]
    fn stereo_diarization_names_the_side() {
        // whisper.cpp -di on a stereo file: left (0) is the meeting app, right (1) your microphone.
        let v = json!({ "transcription": [
            { "offsets": { "from": 0, "to": 6320 }, "text": " Good morning everyone.", "speaker": "0" },
            { "offsets": { "from": 9120, "to": 17360 }, "text": " I have a question.", "speaker": "1" },
            { "offsets": { "from": 17360, "to": 19000 }, "text": " Both at once.", "speaker": "?" } ] });
        let s: Vec<String> = parse_whisper(&v, &[]).into_iter().map(|s| s.speaker).collect();
        assert_eq!(s, ["others", "me", "me"], "an unclear line keeps the speaker before it");
        let mut a = seg(1, "Halo"); a.speaker = "me".into();
        let mut b = seg(2, "Oke"); b.speaker = "others".into();
        let t = Transcript { segments: vec![a, b, seg(3, "Ya")], ..Default::default() };
        assert_eq!(export_text(&t, "M", "txt").unwrap(), "[00:05] You: Halo\n[00:10] Others: Oke\n[00:15] Ya\n");
        assert!(!serde_json::to_string(&seg(3, "Ya")).unwrap().contains("speaker"), "old transcripts keep their shape");
    }
    #[test]
    fn exports_are_well_formed() {
        let t = Transcript { id: "m".into(), model: "whisper-base".into(), language: "id".into(), segments: vec![seg(1, "Halo"), seg(2, "Oke")], ..Default::default() };
        assert_eq!(export_text(&t, "M", "srt").unwrap(), "1\n00:00:05,000 --> 00:00:09,500\nHalo\n\n2\n00:00:10,000 --> 00:00:14,500\nOke\n");
        assert!(export_text(&t, "M", "vtt").unwrap().starts_with("WEBVTT\n\n00:00:05.000 --> 00:00:09.500\nHalo\n"));
        assert_eq!(export_text(&t, "M", "txt").unwrap(), "[00:05] Halo\n[00:10] Oke\n");
        assert!(export_text(&t, "M", "md").unwrap().contains("# M") && export_text(&t, "M", "json").is_some());
        assert!(export_text(&t, "M", "exe").is_none());
    }
    #[test]
    fn summaries_keep_only_what_the_transcript_says() {
        let segs = vec![seg(1, "Budi will send the report by Friday."), seg(2, "We agreed to ship version two."), seg(3, "Who owns the budget?")];
        let raw = json!({ "overview": "Planning.", "topics": [{ "title": "Ship version two", "supportingSegmentIds": ["seg-2", "seg-99"] }],
            "decisions": [{ "text": "Ship v2", "supportingSegmentIds": ["seg-2"], "confidence": "high" }, { "text": "Hire two people", "supportingSegmentIds": [], "confidence": "high" }],
            "actionItems": [{ "task": "Send report", "owner": "Budi", "deadline": "Friday", "supportingSegmentIds": ["seg-1"] }, { "task": "Book room", "owner": "Sari", "deadline": "Monday", "supportingSegmentIds": ["seg-2"] }],
            "openQuestions": [{ "text": "Budget owner?", "supportingSegmentIds": ["seg-3"] }] });
        let v = validate_summary(&raw, &segs, &Default::default());
        assert_eq!(v["topics"][0]["supportingSegmentIds"], json!(["seg-2"]), "an id that does not exist is dropped");
        assert_eq!(v["decisions"][1]["confidence"], "low", "no evidence: low confidence");
        assert_eq!((v["actionItems"][0]["owner"].as_str(), v["actionItems"][0]["deadline"].as_str()), (Some("Budi"), Some("Friday")));
        assert!(v["actionItems"][1]["owner"].is_null() && v["actionItems"][1]["deadline"].is_null(), "a name or date the lines never say is not kept");
        let w = v["warnings"].to_string();
        assert!(w.contains("cite no part") && w.contains("did not match"), "{w}");
        assert!(v["warnings"][0].as_str().unwrap().contains("mistakes"));
        let empty = validate_summary(&json!({}), &segs, &Default::default());
        assert_eq!(empty["decisions"], json!([]));
    }
    #[test]
    fn citations_must_match_what_the_line_says() {
        let segs = vec![seg(1, "Good morning everyone."), seg(2, "Sari will prepare the new pricing deck by Wednesday."), seg(3, "Who is responsible for the vendor contract renewal?"), seg(4, "The team agreed to delay the mobile release by one week.")];
        // What a 0.5B model really gave: lines cited by habit, a question filed as a decision, the same task twice.
        let raw = json!({ "overview": "x",
            "decisions": [{ "text": "The team agreed to delay the mobile release by one week.", "supportingSegmentIds": ["seg-1", "seg-2"] }, { "text": "Who is responsible for the vendor contract renewal?", "supportingSegmentIds": ["seg-1"] }],
            "actionItems": [{ "task": "Sari will prepare the new pricing deck by Wednesday.", "owner": "Sari", "deadline": "Wednesday", "supportingSegmentIds": ["seg-1", "seg-2", "seg-3"] }, { "task": "sari will prepare the new pricing deck by wednesday", "supportingSegmentIds": ["seg-2"] }] });
        let v = validate_summary(&raw, &segs, &Default::default());
        assert_eq!(v["decisions"][0]["supportingSegmentIds"], json!(["seg-4"]), "wrong lines dropped, the right one found");
        assert_eq!(v["decisions"].as_array().unwrap().len(), 1, "the question left the decisions");
        assert_eq!(v["openQuestions"][0]["supportingSegmentIds"], json!(["seg-3"]));
        assert_eq!(v["actionItems"].as_array().unwrap().len(), 1, "the same task once");
        assert_eq!(v["actionItems"][0]["supportingSegmentIds"], json!(["seg-2"]));
        assert_eq!((v["actionItems"][0]["owner"].as_str(), v["actionItems"][0]["deadline"].as_str()), (Some("Sari"), Some("Wednesday")));
        assert!(v["warnings"].to_string().contains("did not match"));
        assert_eq!(strip_marks("[seg-2] (00:06) Sari will prepare the deck."), "Sari will prepare the deck.");
        assert_eq!(strip_marks("(10:00) [seg-12]  Ship it"), "Ship it");
        assert_eq!(strip_marks("[note] keep (this)"), "[note] keep (this)");
        assert!(backs("Kirim laporan", "Budi kirim laporan hari Jumat.") && !backs("Hire two people", "We agreed to ship version two."));
        assert!(content_words("The team and we will ship v2 in 2026").contains("2026") && !content_words("the and yang dan").iter().any(|_| true));
    }
    #[test]
    fn long_transcripts_are_cut_into_pieces_and_replies_parsed() {
        let segs: Vec<Segment> = (1..=100).map(|i| seg(i, &"kata ".repeat(40))).collect();
        let c = chunks(&segs, 3000);
        assert!(c.len() > 5 && c.iter().all(|p| p.iter().map(|s| s.text.len() + 16).sum::<usize>() <= 3000 + 216));
        assert_eq!(c.iter().map(Vec::len).sum::<usize>(), 100, "no segment lost");
        assert_eq!(first_json("noise {\"a\":\"}\",\"b\":{\"c\":1}} tail").unwrap(), json!({ "a": "}", "b": { "c": 1 } }));
        assert!(first_json("no json").is_none());
        assert!(summary_schema()["properties"]["actionItems"]["items"]["properties"]["owner"]["type"].is_array());
    }
    /// Each bundle on this PC with the downloaded engines and models: transcript and summary times, and what the summary
    /// found. Needs a speech WAV (16 kHz mono) in SORA_AI_WAV. `cargo test --release tiers_run_here -- --ignored --nocapture`
    #[test]
    #[ignore]
    /// The transcript steps with the installed Lite and Voices packages, on a recording's two tracks:
    /// SORA_AI_APP and SORA_AI_MIC (Opus or WAV), SORA_FFMPEG. Prints the lines with their speakers.
    fn voices_run_here() {
        let root = PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.nook.desktop").join("local-ai");
        let (app, mic, ffmpeg) = (std::env::var("SORA_AI_APP").unwrap(), std::env::var("SORA_AI_MIC").unwrap(), std::env::var("SORA_FFMPEG").unwrap());
        let whisper = find_file(&root.join("whisper-engine"), "whisper-cli.exe", 3).expect("whisper engine");
        let voices = find_file(&root.join("speaker-engine"), "sherpa-onnx-offline-speaker-diarization.exe", 3).expect("voice engine");
        let work = tempfile::tempdir().unwrap();
        let (input, app_wav, base) = (work.path().join("in.wav"), work.path().join("app.wav"), work.path().join("out"));
        let ff = |args: &[&str], out: &Path| assert!(Command::new(&ffmpeg).args(["-loglevel", "error", "-y"]).args(args).arg(out).status().unwrap().success());
        ff(&["-i", &app, "-i", &mic, "-filter_complex", "[0:a]aformat=channel_layouts=mono[a];[1:a]aformat=channel_layouts=mono,apad[b];[a][b]amerge=inputs=2", "-ar", "16000", "-ac", "2", "-c:a", "pcm_s16le"], &input);
        ff(&["-i", &app, "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"], &app_wav);
        let t0 = Instant::now();
        assert!(Command::new(&whisper).current_dir(whisper.parent().unwrap()).arg("-m").arg(root.join("whisper-base-q5.bin")).arg("-f").arg(&input)
            .args(["-l", "en", "-t", "6", "-oj", "-of"]).arg(&base).args(["-ml", "1", "-sow", "-di", "--vad", "-vm"]).arg(root.join("whisper-vad.bin"))
            .stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success());
        let t1 = Instant::now();
        let out = Command::new(&voices).current_dir(voices.parent().unwrap()).arg(format!("--segmentation.pyannote-model={}", root.join("speaker-seg.bin").display()))
            .arg(format!("--embedding.model={}", root.join("speaker-voice.bin").display())).args(["--segmentation.num-threads=6", "--embedding.num-threads=6", "--clustering.cluster-threshold=0.9"])
            .arg(&app_wav).stderr(Stdio::null()).output().unwrap();
        let turns = parse_turns(&String::from_utf8_lossy(&out.stdout));
        println!("whisper {:.1}s, voices {:.1}s, turns {turns:?}", (t1 - t0).as_secs_f32(), t1.elapsed().as_secs_f32());
        let segs = parse_whisper(&serde_json::from_slice(&fs::read(base.with_extension("json")).unwrap()).unwrap(), &turns);
        for s in &segs { println!("[{}] {}{}", mmss(s.start), who(&Default::default(), s), s.text); }
        assert!(segs.iter().any(|s| s.speaker == "p1") && segs.iter().any(|s| s.speaker == "p2") && segs.iter().any(|s| s.speaker == "me"));
    }
    #[test]
    #[ignore]
    fn tiers_run_here() {
        let root = PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.nook.desktop").join("local-ai");
        let wav = PathBuf::from(std::env::var("SORA_AI_WAV").expect("SORA_AI_WAV"));
        let whisper = find_file(&root.join("whisper-engine"), "whisper-cli.exe", 3).expect("whisper engine");
        let llama = find_file(&root.join("llama-engine"), "llama-completion.exe", 3).expect("llama engine");
        let threads = threads_for("balanced", std::thread::available_parallelism().map_or(2, |n| n.get())).0.to_string();
        let work = tempfile::tempdir().unwrap();
        let schema = work.path().join("schema.json");
        fs::write(&schema, summary_schema().to_string()).unwrap();
        for (name, w, q) in TIERS {
            let t0 = Instant::now();
            let base = work.path().join(w);
            let ok = Command::new(&whisper).current_dir(whisper.parent().unwrap()).arg("-m").arg(root.join(format!("{w}.bin"))).arg("-f").arg(&wav)
                .args(["-l", "auto", "-t", &threads, "-oj", "-of"]).arg(&base).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success();
            assert!(ok, "{w} ran");
            let segs = parse_whisper(&serde_json::from_slice(&fs::read(base.with_extension("json")).unwrap()).unwrap(), &[]);
            let tw = t0.elapsed().as_secs_f32();
            let t1 = Instant::now();
            let pf = work.path().join(format!("{q}.txt"));
            fs::write(&pf, prompt(&segs, &Default::default(), "auto")).unwrap();
            let out = Command::new(&llama).current_dir(llama.parent().unwrap()).arg("-m").arg(root.join(format!("{q}.bin"))).arg("-f").arg(&pf)
                .args(["-n", "1500", "-c", "8192", "--temp", "0.2", "--repeat-penalty", "1.15", "-t", &threads, "--no-display-prompt", "-no-cnv", "-jf"]).arg(&schema).stderr(Stdio::null()).output().unwrap();
            let raw = first_json(&String::from_utf8_lossy(&out.stdout)).expect("summary JSON");
            let v = validate_summary(&raw, &segs, &Default::default());
            let n = |k: &str| v[k].as_array().map_or(0, |a| a.len());
            println!("{name}: {w} {tw:.1} s ({} segments), {q} {:.1} s; decisions {}, actions {}, questions {}, topics {}\n  {}\n  actions: {}\n  warnings: {}",
                segs.len(), t1.elapsed().as_secs_f32(), n("decisions"), n("actionItems"), n("openQuestions"), n("topics"), v["overview"],
                v["actionItems"].as_array().unwrap().iter().map(|a| format!("{} / {} / {}", a["task"], a["owner"], a["deadline"])).collect::<Vec<_>>().join("; "), v["warnings"]);
        }
    }
}

#[cfg(all(test, win))]
mod priority_tests {
    /// threads_for keeps the priority classes as plain numbers (so it builds on macOS too): they must stay Windows' own.
    #[test]
    fn classes_match_windows() {
        use windows::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS};
        assert_eq!(super::threads_for("eco", 8).1, IDLE_PRIORITY_CLASS.0);
        assert_eq!(super::threads_for("fast", 8).1, NORMAL_PRIORITY_CLASS.0);
        assert_eq!(super::threads_for("balanced", 8).1, BELOW_NORMAL_PRIORITY_CLASS.0);
    }
}
