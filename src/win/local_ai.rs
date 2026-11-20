//! The Windows side of local_ai (local_ai.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) fn hardware(app: &AppHandle) -> Hardware {
    use windows::Win32::System::{Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS}, SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX}};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let _ = unsafe { GlobalMemoryStatusEx(&mut m) };
    let mut ps = SYSTEM_POWER_STATUS::default();
    let power = unsafe { GetSystemPowerStatus(&mut ps) }.is_ok();
    let g = |b: u64| (b as f64 / 1073741824.0 * 10.0).round() / 10.0;
    Hardware { ram_gb: g(m.ullTotalPhys), free_ram_gb: g(m.ullAvailPhys), cores: std::thread::available_parallelism().map_or(2, |n| n.get()), avx2: std::is_x86_feature_detected!("avx2"),
        disk_free_gb: g(root(app).map(|r| free_bytes(&r)).unwrap_or(0)), on_battery: power && ps.ACLineStatus == 0, battery: (power && ps.BatteryLifePercent <= 100).then_some(ps.BatteryLifePercent) }
}
pub(super) fn free_bytes(dir: &Path) -> u64 {
    let mut free = 0u64;
    let _ = unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(&windows::core::HSTRING::from(dir.as_os_str()), Some(&mut free), None, None) };
    free
}
/// Sets a worker's Windows priority class.
pub(super) fn set_priority(child: &std::process::Child, priority: u32) { unsafe { use std::os::windows::io::AsRawHandle; let _ = windows::Win32::System::Threading::SetPriorityClass(windows::Win32::Foundation::HANDLE(child.as_raw_handle()), windows::Win32::System::Threading::PROCESS_CREATION_FLAGS(priority)); } }
/// Suspends or resumes every thread of a process (one of Sora's own workers).
pub(super) fn freeze(pid: u32, on: bool) {
    use windows::Win32::{Foundation::CloseHandle, System::{Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32}, Threading::{OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME}}};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else { return };
        let mut e = THREADENTRY32 { dwSize: std::mem::size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut more = Thread32First(snap, &mut e).is_ok();
        while more {
            if e.th32OwnerProcessID == pid {
                if let Ok(h) = OpenThread(THREAD_SUSPEND_RESUME, false, e.th32ThreadID) { if on { SuspendThread(h); } else { ResumeThread(h); } let _ = CloseHandle(h); }
            }
            more = Thread32Next(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
}
/// The program inside an unpacked engine: whisper-cli, the voice separation, or llama-completion (else llama-cli).
pub(super) fn engine_exe(app: &AppHandle, id: &str) -> Option<PathBuf> {
    let dir = engine_dir(app, id).ok()?;
    let names: &[&str] = match id { "whisper-engine" => &["whisper-cli.exe", "main.exe"], "speaker-engine" => &["sherpa-onnx-offline-speaker-diarization.exe"], _ => &["llama-completion.exe", "llama-cli.exe"] };
    for name in names { if let Some(p) = find_file(&dir, name, 3) { return Some(p); } }
    None
}
/// Engines download from their releases on Windows.
pub(super) fn engine_hint(_id: &str) -> Option<String> { None }
/// Shows a file selected in Explorer.
pub(super) fn reveal(file: &Path) { let _ = Command::new("explorer.exe").arg(format!("/select,{}", file.display())).spawn(); }
/// The voice separation package for Windows (sherpa-onnx's own release).
pub(super) const SPEAKER_DETAIL: &str = "sherpa-onnx v2.0.1, CPU (x64)";
pub(super) const SPEAKER_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-win-x64-shared-MT-Release-no-tts.tar.bz2";
pub(super) const SPEAKER_SHA256: &str = "a644dfe2f7944afec9a356d2557c0452380f20ffcf2f8c704653cfce37558596";
pub(super) const SPEAKER_SIZE: u64 = 25567228;
/// The voice engine's .tar.bz2 with Windows' own tar (bsdtar refuses absolute names and ".." without -P). Only the
/// program and its ONNX Runtime are kept: 19 MB of the 68 MB.
pub(super) fn untar(archive: &Path, tmp: &Path) -> Result<(), String> {
    let tar = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into())).join("System32").join("tar.exe");
    let mut cmd = Command::new(tar);
    cmd.arg("-xjf").arg(archive).arg("-C").arg(tmp).args(["--include", "*/bin/sherpa-onnx-offline-speaker-diarization.exe", "--include", "*/bin/onnxruntime*.dll"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let (mut child, _job) = crate::convert::engines::spawn_jobbed(&mut cmd)?;
    if child.wait().map_err(|e| e.to_string())?.success() { Ok(()) } else { Err("The engine archive could not be unpacked.".into()) }
}
