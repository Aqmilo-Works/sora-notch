//! The macOS side of local_ai (local_ai.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// shortcut: on macOS free memory is taken as half the total and the battery is not read; upgrade with host_statistics64 and IOKit.
pub(super) fn hardware(app: &AppHandle) -> Hardware {
    let total = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok()).unwrap_or(0);
    let g = |b: u64| (b as f64 / 1073741824.0 * 10.0).round() / 10.0;
    // Apple Silicon has NEON, which llama.cpp and whisper.cpp use as they use AVX2 on a PC.
    #[cfg(target_arch = "x86_64")]
    let simd = std::is_x86_feature_detected!("avx2");
    #[cfg(not(target_arch = "x86_64"))]
    let simd = true;
    Hardware { ram_gb: g(total), free_ram_gb: g(total / 2), cores: std::thread::available_parallelism().map_or(2, |n| n.get()), avx2: simd,
        disk_free_gb: g(root(app).map(|r| free_bytes(&r)).unwrap_or(0)), on_battery: false, battery: None }
}
/// Free space from `df` (1024-byte blocks, the 4th column).
pub(super) fn free_bytes(dir: &Path) -> u64 {
    let out = std::process::Command::new("df").arg("-Pk").arg(dir).output().ok();
    out.and_then(|o| String::from_utf8_lossy(&o.stdout).lines().nth(1).and_then(|l| l.split_whitespace().nth(3).and_then(|n| n.parse::<u64>().ok()))).map_or(0, |k| k * 1024)
}
/// shortcut: macOS runs workers at normal priority; upgrade with setpriority (nice) after spawn.
pub(super) fn set_priority(_child: &std::process::Child, _priority: u32) {}
/// Stops or continues one of Sora's own workers (SIGSTOP / SIGCONT).
pub(super) fn freeze(pid: u32, on: bool) { let _ = std::process::Command::new("kill").arg(if on { "-STOP" } else { "-CONT" }).arg(pid.to_string()).status(); }
/// The engines on macOS come inside Sora (whisper-cli and llama-completion with Metal, built by the CI for this CPU),
/// else Homebrew's builds: whisper-cli from whisper-cpp, llama-completion (or llama-cli) from llama.cpp. The models are
/// downloaded by Sora as on Windows.
/// Voice separation is sherpa-onnx's macOS release, downloaded and checked like on Windows.
pub(super) fn engine_exe(_app: &AppHandle, id: &str) -> Option<PathBuf> {
    if id == "speaker-engine" { return find_file(&engine_dir(_app, id).ok()?, "sherpa-onnx-offline-speaker-diarization", 3); }
    let names: &[&str] = match id { "whisper-engine" => &["whisper-cli"], "llama-engine" => &["llama-completion", "llama-cli"], _ => &[] };
    names.iter().find_map(|n| crate::whatsapp::module(n)).or_else(|| names.iter().find_map(|n| crate::platform::find_cli(n)))
}
/// What to do instead of a download for an engine on macOS.
pub(super) fn engine_hint(id: &str) -> Option<String> {
    Some(match id {
        "whisper-engine" => "On a Mac the transcript engine comes from Homebrew: run  brew install whisper-cpp  in Terminal, then open this again.".into(),
        "llama-engine" => "On a Mac the summary engine comes from Homebrew: run  brew install llama.cpp  in Terminal, then open this again.".into(),
        _ => return None,
    })
}
/// Shows a file selected in Finder.
pub(super) fn reveal(file: &Path) { let _ = Command::new("open").arg("-R").arg(file).spawn(); }
/// The voice separation package for macOS: sherpa-onnx's universal (Apple silicon and Intel) shared build.
pub(super) const SPEAKER_DETAIL: &str = "sherpa-onnx v2.0.1, CPU (macOS)";
pub(super) const SPEAKER_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-osx-universal2-shared-no-tts.tar.bz2";
pub(super) const SPEAKER_SHA256: &str = "9021b6f33e8d1657fe5ae7a13e83fc948707f86b749dc7b775680db6451cc2b8";
pub(super) const SPEAKER_SIZE: u64 = 40097083;
/// The voice engine's .tar.bz2 with macOS's own tar: only the program and the libraries it loads (ONNX Runtime and
/// sherpa-onnx's own, found through its rpath in ../lib).
pub(super) fn untar(archive: &Path, tmp: &Path) -> Result<(), String> {
    let ok = Command::new("/usr/bin/tar").arg("-xjf").arg(archive).arg("-C").arg(tmp)
        .args(["--include", "*/bin/sherpa-onnx-offline-speaker-diarization", "--include", "*/lib/*.dylib"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|e| e.to_string())?.success();
    if ok { Ok(()) } else { Err("The engine archive could not be unpacked.".into()) }
}
