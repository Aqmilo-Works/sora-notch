//! The Windows side of meeting (meeting.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::{implement, Interface, HSTRING, PCWSTR, PWSTR}, Win32::{Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM, TRUE}, Media::Audio::*, System::{Com::{CoCreateInstance, CoInitializeEx, IAgileObject, CLSCTX_ALL, COINIT_MULTITHREADED}, Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS}, Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ}, Threading::{CreateEventW, GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION}}, UI::{Shell::{FOLDERID_Documents, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible}}}};
/// The Windows build number ("CurrentBuildNumber"), 0 when unknown.
pub fn windows_build() -> u32 {
    unsafe {
        let mut buf = [0u16; 32];
        let mut size = (buf.len() * 2) as u32;
        let r = RegGetValueW(HKEY_LOCAL_MACHINE, &HSTRING::from(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"), &HSTRING::from("CurrentBuildNumber"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size));
        if r.is_err() { return 0; }
        String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]).trim().parse().unwrap_or(0)
    }
}
pub(super) fn processes() -> HashMap<u32, (u32, String)> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            map.insert(e.th32ProcessID, (e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile).trim_end_matches('\0').to_lowercase()));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    map
}
/// Visible windows with a title, per process id.
pub(super) fn window_titles() -> HashMap<u32, Vec<String>> {
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        let map = &mut *(l.0 as *mut HashMap<u32, Vec<String>>);
        if !IsWindowVisible(h).as_bool() { return TRUE; }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(h, &mut buf);
        if n <= 0 { return TRUE; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        map.entry(pid).or_default().push(String::from_utf16_lossy(&buf[..n as usize]));
        TRUE
    }
    let mut map: HashMap<u32, Vec<String>> = HashMap::new();
    unsafe { let _ = EnumWindows(Some(each), LPARAM(&mut map as *mut _ as isize)); }
    map
}
/// Processes with an audio session that is playing now, on any active output device.
pub(super) fn active_audio_pids() -> HashSet<u32> {
    let mut out = HashSet::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(devices) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        let Ok(list) = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
        for i in 0..list.GetCount().unwrap_or(0) {
            let Ok(dev) = list.Item(i) else { continue };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sessions) = mgr.GetSessionEnumerator() else { continue };
            for j in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(s) = sessions.GetSession(j) else { continue };
                let Ok(s2) = s.cast::<IAudioSessionControl2>() else { continue };
                if s2.GetState().ok() == Some(AudioSessionStateActive) { if let Ok(pid) = s2.GetProcessId() { out.insert(pid); } }
            }
        }
    }
    out
}
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
pub(super) struct Activated(mpsc::Sender<()>);
impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _op: Option<&IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> { let _ = self.0.send(()); Ok(()) }
}
impl windows::Win32::System::Com::IAgileObject_Impl for Activated_Impl {}
/// PROPVARIANT holding a VT_BLOB (the layout of the 24-byte PROPVARIANT on x64), for the activation parameters.
#[repr(C)]
pub(super) struct BlobVariant { vt: u16, r1: u16, r2: u16, r3: u16, size: u32, data: *const u8 }
pub(super) const VT_BLOB: u16 = 65;
/// An audio client capturing one process tree (Windows process loopback).
pub(super) unsafe fn process_client(pid: u32) -> Result<IAudioClient, String> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS { ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 { ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE } } };
    let pv = BlobVariant { vt: VT_BLOB, r1: 0, r2: 0, r3: 0, size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, data: &params as *const _ as *const u8 };
    let (tx, rx) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Activated(tx).into();
    let op = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&pv as *const BlobVariant as *const windows::core::PROPVARIANT), &handler).map_err(|e| format!("Windows refused the app capture: {}", e.message()))?;
    rx.recv_timeout(Duration::from_secs(5)).map_err(|_| "Windows did not start the app capture in time.".to_string())?;
    let mut hr = windows::core::HRESULT(0);
    let mut unk = None;
    op.GetActivateResult(&mut hr, &mut unk).map_err(|e| e.message().to_string())?;
    hr.ok().map_err(|e| format!("Windows refused the app capture: {}", e.message()))?;
    unk.ok_or("No audio client.")?.cast::<IAudioClient>().map_err(|e| e.message().to_string())
}
pub(super) fn format() -> WAVEFORMATEX {
    WAVEFORMATEX { wFormatTag: 1, nChannels: CHANNELS, nSamplesPerSec: RATE, nAvgBytesPerSec: RATE * BLOCK, nBlockAlign: BLOCK as u16, wBitsPerSample: BITS, cbSize: 0 }
}
pub(super) unsafe fn open(source: Source) -> Result<(IAudioClient, IAudioCaptureClient, HANDLE), String> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    let (client, loopback) = match source {
        Source::Process(pid) => (process_client(pid)?, true),
        Source::System | Source::Mic => {
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.message().to_string())?;
            let dev = devices.GetDefaultAudioEndpoint(if source == Source::Mic { eCapture } else { eRender }, if source == Source::Mic { eCommunications } else { eConsole }).map_err(|_| if source == Source::Mic { "No microphone found.".to_string() } else { "No audio output found.".to_string() })?;
            (dev.Activate::<IAudioClient>(CLSCTX_ALL, None).map_err(|e| e.message().to_string())?, source == Source::System)
        }
    };
    let fmt = format();
    // As in Microsoft's ApplicationLoopback sample: shared, event driven, the engine converts to our PCM format.
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | if loopback { AUDCLNT_STREAMFLAGS_LOOPBACK } else { AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY };
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &fmt, None).map_err(|e| if source == Source::Mic { format!("The microphone is not available: {}", e.message()) } else { e.message().to_string() })?;
    let event = CreateEventW(None, false, false, None).map_err(|e| e.message().to_string())?;
    client.SetEventHandle(event).map_err(|e| e.message().to_string())?;
    let capture = client.GetService::<IAudioCaptureClient>().map_err(|e| e.message().to_string())?;
    Ok((client, capture, event))
}
pub(super) fn alive(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok() && code == 259;
        let _ = CloseHandle(h);
        ok
    }
}
/// One capture loop: waits for packets, writes them (silence while paused is dropped), pads gaps where the source sent
/// nothing (an app's loopback is quiet while it plays nothing), and stops on request, on the target's exit or on a
/// write error (disk full): what was written is kept.
pub(super) fn run_capture(source: Source, path: PathBuf, live: Arc<Live>, ready: mpsc::Sender<Result<(), String>>, is_mic: bool) -> Result<PathBuf, String> {
    unsafe {
        let (client, capture, event) = match open(source) { Ok(x) => x, Err(e) => { let _ = ready.send(Err(e.clone())); return Err(e) } };
        let mut wav = match WavWriter::create(&path) { Ok(w) => w, Err(e) => { let m = format!("Could not create the file: {e}"); let _ = ready.send(Err(m.clone())); return Err(m) } };
        if let Err(e) = client.Start() { let m = e.message().to_string(); let _ = ready.send(Err(m.clone())); let _ = std::fs::remove_file(part_path(&path)); return Err(m); }
        // Only now is the recording real: the audio engine runs.
        let _ = ready.send(Ok(()));
        let mut active_since = Instant::now();
        let mut written_frames: u64 = 0;
        let mut active_ms_before: u64 = 0;
        let mut was_paused = false;
        let mut last_check = Instant::now();
        let zeros = vec![0u8; (RATE as usize / 10) * BLOCK as usize];
        let mut failure: Option<String> = None;
        while !live.stop.load(Ordering::SeqCst) {
            let _ = WaitForSingleObject(event, 200);
            let paused = live.paused.load(Ordering::SeqCst);
            if paused != was_paused {
                if paused { active_ms_before += active_since.elapsed().as_millis() as u64; } else { active_since = Instant::now(); written_frames = active_ms_before * RATE as u64 / 1000; }
                was_paused = paused;
            }
            let mut peak = 0u16;
            loop {
                let Ok(n) = capture.GetNextPacketSize() else { failure = Some("The audio device went away.".into()); break };
                if n == 0 { break; }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut::<u8>(), 0u32, 0u32);
                if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() { failure = Some("The audio device went away.".into()); break; }
                // AUDCLNT_S_BUFFER_EMPTY is a success code with no frames: nothing more for now.
                if frames == 0 { let _ = capture.ReleaseBuffer(0); break; }
                let bytes = frames as usize * BLOCK as usize;
                if !paused {
                    let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null();
                    let res = if silent { write_zeros(&mut wav, bytes, &zeros) } else {
                        let slice = std::slice::from_raw_parts(data, bytes);
                        for s in slice.chunks_exact(2) { peak = peak.max(i16::from_le_bytes([s[0], s[1]]).unsigned_abs()); }
                        wav.write(slice)
                    };
                    if let Err(e) = res { failure = Some(write_error(&e)); let _ = capture.ReleaseBuffer(frames); break; }
                    written_frames += frames as u64;
                }
                let _ = capture.ReleaseBuffer(frames);
            }
            if failure.is_some() { break; }
            (if is_mic { &live.mic_level } else { &live.level }).store(peak as u32, Ordering::SeqCst);
            if !paused {
                // Fill time the source was quiet, so the file's clock matches the meeting's.
                let expected = (active_ms_before + active_since.elapsed().as_millis() as u64) * RATE as u64 / 1000;
                if expected > written_frames + RATE as u64 / 5 {
                    let gap = ((expected - written_frames - RATE as u64 / 20) as usize).min(RATE as usize * 2) * BLOCK as usize;
                    if let Err(e) = write_zeros(&mut wav, gap, &zeros) { failure = Some(write_error(&e)); break; }
                    written_frames += (gap / BLOCK as usize) as u64;
                }
                if !is_mic { live.recorded_ms.store(active_ms_before + active_since.elapsed().as_millis() as u64, Ordering::SeqCst); }
            }
            if let Source::Process(pid) = source {
                if last_check.elapsed() > Duration::from_secs(1) { last_check = Instant::now(); if !alive(pid) { failure = Some("The meeting app closed, so the recording stopped.".into()); break; } }
            }
        }
        let _ = client.Stop();
        let _ = CloseHandle(event);
        if let Some(f) = &failure { if let Ok(mut e) = live.error.lock() { if e.is_empty() { *e = f.clone(); } } live.stop.store(true, Ordering::SeqCst); }
        wav.finish().map_err(|e| format!("Could not finish the file: {e}"))
    }
}
pub(super) fn documents() -> PathBuf {
    unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok().and_then(|p| p.to_string().ok()).map(PathBuf::from) }
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Documents"))).unwrap_or_else(|| PathBuf::from("."))
}
/// Makes `<x>.wav` into `<x>.opus` with the bundled FFmpeg (low priority), then deletes the WAV. The WAV stays when
/// anything fails, so a recording is never lost to compression.
/// Compression runs below normal priority, so the PC stays quick meanwhile.
pub(super) fn below_normal(child: &std::process::Child) {
    unsafe { use std::os::windows::io::AsRawHandle; let _ = windows::Win32::System::Threading::SetPriorityClass(HANDLE(child.as_raw_handle()), windows::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS); }
}
/// "2026-11-12 14-03" in local time, for file names.
pub fn local_stamp() -> String {
    unsafe {
        let t = windows::Win32::System::SystemInformation::GetLocalTime();
        format!("{:04}-{:02}-{:02} {:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute)
    }
}
#[allow(dead_code)]
pub(super) fn _unused(_: PCWSTR, _: PWSTR) {}
