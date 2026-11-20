//! The macOS side of meeting (meeting.rs), its `os` module. Everything else there is shared.
//! "All sound" is ScreenCaptureKit's audio of the whole display with Sora's own sound left out (macOS asks for Screen
//! Recording permission once); the microphone is AVAudioEngine's input (Microphone permission). Both arrive as float
//! samples and are written as the same 48 kHz 16-bit stereo WAV as on Windows, pauses and quiet gaps handled alike.
//! One meeting app alone is ScreenCaptureKit with a filter that includes only that app (Windows: process loopback).
//! shortcut: macOS does not say which apps play sound, so a meeting app is found by its call window only.
#![allow(unused_imports, unused_unsafe)]
use super::*;
use std::{ffi::c_void, ptr::NonNull};
use block2::RcBlock;
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass};
use objc2_avf_audio::{AVAudioEngine, AVAudioPCMBuffer, AVAudioTime};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList};
use objc2_core_media::{CMBlockBuffer, CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{SCContentFilter, SCRunningApplication, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput, SCStreamOutputType};

/// One app alone works on every macOS Sora runs on (ScreenCaptureKit, macOS 12.3+), so this passes the build check.
pub fn windows_build() -> u32 { u32::MAX }
/// The meeting apps' macOS program names under the names PROVIDERS knows them by (Windows'), so the shared scan finds
/// them (pure; tested).
pub(super) fn known_as(mac_name: &str) -> &str {
    match mac_name {
        "zoom.us" => "zoom.exe",
        "msteams" | "microsoft teams" | "microsoft teams (work or school)" => "ms-teams.exe",
        "webex" | "cisco webex meetings" => "webexhost.exe",
        "slack" => "slack.exe",
        "lark" => "lark.exe",
        "feishu" => "feishu.exe",
        "google chrome" => "chrome.exe",
        "microsoft edge" => "msedge.exe",
        "firefox" => "firefox.exe",
        "brave browser" => "brave.exe",
        "opera" => "opera.exe",
        "vivaldi" => "vivaldi.exe",
        other => other,
    }
}
/// Running processes: pid -> (parent pid, program name in lower case), from `ps`.
pub(super) fn processes() -> HashMap<u32, (u32, String)> {
    let Ok(out) = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,comm="]).output() else { return HashMap::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| {
        let mut f = l.split_whitespace();
        let (pid, ppid) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
        let name = f.collect::<Vec<_>>().join(" ");
        let name = name.rsplit('/').next().unwrap_or(&name).to_lowercase();
        Some((pid, (ppid, known_as(&name).to_string())))
    }).collect()
}
/// Window titles per process, from the window server (the titles need the Screen Recording permission recording asks for).
pub(super) fn window_titles() -> HashMap<u32, Vec<String>> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative: u32) -> *const c_void;
        static kCGWindowOwnerPID: *const c_void;
        static kCGWindowName: *const c_void;
        static kCGWindowLayer: *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(a: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
        fn CFDictionaryGetValue(d: *const c_void, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(n: *const c_void, kind: isize, out: *mut c_void) -> bool;
        fn CFStringGetCString(s: *const c_void, buf: *mut u8, size: isize, encoding: u32) -> bool;
    }
    let mut map: HashMap<u32, Vec<String>> = HashMap::new();
    unsafe {
        // kCGWindowListOptionAll | ExcludeDesktopElements: a call window may sit behind others or on another Space.
        let list = CGWindowListCopyWindowInfo(16, 0);
        if list.is_null() { return map; }
        for i in 0..CFArrayGetCount(list) {
            let d = CFArrayGetValueAtIndex(list, i);
            let num = |key| { let v = CFDictionaryGetValue(d, key); let mut out = 0i32; (!v.is_null() && CFNumberGetValue(v, 3, (&mut out as *mut i32).cast())).then_some(out) };
            if num(kCGWindowLayer) != Some(0) { continue; }
            let (Some(pid), name) = (num(kCGWindowOwnerPID), CFDictionaryGetValue(d, kCGWindowName)) else { continue };
            if name.is_null() { continue; }
            let mut buf = [0u8; 512];
            // kCFStringEncodingUTF8
            if !CFStringGetCString(name, buf.as_mut_ptr(), buf.len() as isize, 0x0800_0100) { continue; }
            let title = std::ffi::CStr::from_ptr(buf.as_ptr().cast()).to_string_lossy().trim().to_string();
            if !title.is_empty() { map.entry(pid as u32).or_default().push(title); }
        }
        CFRelease(list);
    }
    map
}
pub(super) fn active_audio_pids() -> HashSet<u32> { HashSet::new() }
pub(super) fn alive(pid: u32) -> bool { unsafe { libc::kill(pid as i32, 0) == 0 } }

/// Float channels at `rate` as 48 kHz 16-bit stereo, and the loudest sample: mono goes to both sides, more channels keep
/// the first two, other rates are resampled linearly (pure; tested).
pub(super) fn to_pcm(channels: &[&[f32]], rate: f64) -> (Vec<u8>, u16) {
    let n = channels.first().map_or(0, |c| c.len());
    if n == 0 || !(rate > 0.0) { return (Vec::new(), 0); }
    let frames = (n as f64 * RATE as f64 / rate).round() as usize;
    let (mut out, mut peak) = (Vec::with_capacity(frames * BLOCK as usize), 0u16);
    for i in 0..frames {
        let pos = i as f64 * rate / RATE as f64;
        let (j, t) = (pos.floor() as usize, (pos - pos.floor()) as f32);
        for side in 0..2 {
            let c = channels[side.min(channels.len() - 1)];
            let (a, b) = (c[j.min(n - 1)], c[(j + 1).min(n - 1)]);
            let v = ((a + (b - a) * t).clamp(-1.0, 1.0) * 32767.0) as i16;
            peak = peak.max(v.unsigned_abs());
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    (out, peak)
}
type Pcm = (Vec<u8>, u16);

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" { fn CFRelease(cf: *const c_void); }

/// The samples of one ScreenCaptureKit audio buffer (float, one buffer per channel or interleaved), as PCM.
unsafe fn sample_pcm(sb: &CMSampleBuffer) -> Option<Pcm> {
    let mut size = 0usize;
    sb.audio_buffer_list_with_retained_block_buffer(&mut size, std::ptr::null_mut(), 0, None, None, 0, std::ptr::null_mut());
    if size == 0 { return None; }
    let mut storage = vec![0u64; size.div_ceil(8)];
    let list = storage.as_mut_ptr() as *mut AudioBufferList;
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment
    if sb.audio_buffer_list_with_retained_block_buffer(std::ptr::null_mut(), list, size, None, None, 1, &mut block) != 0 { return None; }
    let buffers: &[AudioBuffer] = std::slice::from_raw_parts((*list).mBuffers.as_ptr(), (*list).mNumberBuffers as usize);
    let floats = |b: &AudioBuffer| if b.mData.is_null() { &[][..] } else { std::slice::from_raw_parts(b.mData as *const f32, b.mDataByteSize as usize / 4) };
    let pcm = if buffers.len() == 1 && buffers[0].mNumberChannels >= 2 {
        let all = floats(&buffers[0]);
        let ch = buffers[0].mNumberChannels as usize;
        let (l, r): (Vec<f32>, Vec<f32>) = all.chunks_exact(ch).map(|f| (f[0], f[1])).unzip();
        to_pcm(&[&l, &r], RATE as f64)
    } else {
        let chans: Vec<&[f32]> = buffers.iter().map(floats).collect();
        to_pcm(&chans, RATE as f64)
    };
    if !block.is_null() { CFRelease(block as *const c_void); }
    Some(pcm)
}

struct OutputIvars { tx: Mutex<mpsc::Sender<Pcm>> }
define_class!(
    // Receives ScreenCaptureKit's audio buffers on its own queue and hands them to the writing loop.
    #[unsafe(super(NSObject))]
    #[name = "SoraMeetingAudio"]
    #[ivars = OutputIvars]
    struct AudioOutput;

    unsafe impl NSObjectProtocol for AudioOutput {}

    unsafe impl SCStreamOutput for AudioOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Audio { return; }
            if let Some(pcm) = unsafe { sample_pcm(sample) } { if let Ok(tx) = self.ivars().tx.lock() { let _ = tx.send(pcm); } }
        }
    }
);

/// A running source; dropping it stops it.
enum Capture { Screen(Retained<SCStream>, Retained<AudioOutput>), Mic(Retained<AVAudioEngine>, RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>) }
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            match self {
                Capture::Screen(stream, _) => stream.stopCaptureWithCompletionHandler(None),
                Capture::Mic(engine, _) => { engine.inputNode().removeTapOnBus(0); engine.stop(); }
            }
        }
    }
}

/// All of the display's sound except Sora's own, or (`only`) the sound of one app alone: every process of the app the
/// pid belongs to (its helpers play the sound in browsers and in Teams).
fn open_screen(tx: mpsc::Sender<Pcm>, only: Option<u32>) -> Result<Capture, String> {
    unsafe {
        let (ctx, crx) = mpsc::channel::<Option<usize>>();
        let got = RcBlock::new(move |content: *mut SCShareableContent, _error: *mut NSError| {
            let kept = if content.is_null() { None } else { Retained::retain(content).map(|c| Retained::into_raw(c) as usize) };
            let _ = ctx.send(kept);
        });
        SCShareableContent::getShareableContentWithCompletionHandler(&got);
        let raw = crx.recv_timeout(Duration::from_secs(8)).ok().flatten()
            .ok_or("macOS did not allow Sora to record the screen's sound. Turn Sora on in System Settings > Privacy & Security > Screen & System Audio Recording, then try again.")?;
        let content: Retained<SCShareableContent> = Retained::from_raw(raw as *mut SCShareableContent).ok_or("No shareable content.")?;
        let display = content.displays().firstObject().ok_or("No display to record from.")?;
        let apps = content.applications();
        let filter = match only {
            Some(pid) => {
                let bundle = apps.iter().find(|a| a.processID() == pid as i32).map(|a| a.bundleIdentifier().to_string()).ok_or("That meeting app is gone. Scan again.")?;
                let its: Vec<Retained<SCRunningApplication>> = apps.iter().filter(|a| a.bundleIdentifier().to_string() == bundle).collect();
                SCContentFilter::initWithDisplay_includingApplications_exceptingWindows(SCContentFilter::alloc(), &display, &NSArray::from_retained_slice(&its), &NSArray::new())
            }
            None => {
                let me = std::process::id() as i32;
                let own: Vec<Retained<SCRunningApplication>> = apps.iter().filter(|a| a.processID() == me).collect();
                SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(SCContentFilter::alloc(), &display, &NSArray::from_retained_slice(&own), &NSArray::new())
            }
        };
        let config = SCStreamConfiguration::new();
        config.setCapturesAudio(true);
        config.setSampleRate(RATE as isize);
        config.setChannelCount(CHANNELS as isize);
        config.setExcludesCurrentProcessAudio(true);
        // The picture is not used: the smallest one, once a second.
        config.setWidth(2);
        config.setHeight(2);
        config.setMinimumFrameInterval(CMTime { value: 1, timescale: 1, flags: CMTimeFlags::Valid, epoch: 0 });
        let output = AudioOutput::alloc().set_ivars(OutputIvars { tx: Mutex::new(tx) });
        let output: Retained<AudioOutput> = msg_send![super(output), init];
        let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None);
        let queue = dispatch2::DispatchQueue::new("sora.meeting.audio", None);
        stream.addStreamOutput_type_sampleHandlerQueue_error(ProtocolObject::from_ref(&*output), SCStreamOutputType::Audio, Some(&queue)).map_err(|e| e.localizedDescription().to_string())?;
        let (stx, srx) = mpsc::channel::<Option<String>>();
        let started = RcBlock::new(move |error: *mut NSError| { let _ = stx.send(error.as_ref().map(|e| e.localizedDescription().to_string())); });
        stream.startCaptureWithCompletionHandler(Some(&started));
        match srx.recv_timeout(Duration::from_secs(8)) {
            Ok(None) => Ok(Capture::Screen(stream, output)),
            Ok(Some(e)) => Err(format!("macOS could not start recording the sound: {e}")),
            Err(_) => Err("macOS did not start recording the sound.".into()),
        }
    }
}
/// The default microphone.
fn open_mic(tx: mpsc::Sender<Pcm>) -> Result<Capture, String> {
    unsafe {
        let engine = AVAudioEngine::new();
        let input = engine.inputNode();
        let format = input.outputFormatForBus(0);
        if format.sampleRate() <= 0.0 || format.channelCount() == 0 { return Err("No microphone is available.".into()); }
        let tap = RcBlock::new(move |buffer: NonNull<AVAudioPCMBuffer>, _when: NonNull<AVAudioTime>| {
            let buffer = buffer.as_ref();
            let frames = buffer.frameLength() as usize;
            let data = buffer.floatChannelData();
            if data.is_null() || frames == 0 { return; }
            let f = buffer.format();
            let chans: Vec<&[f32]> = (0..f.channelCount() as usize).map(|c| std::slice::from_raw_parts((*data.add(c)).as_ptr() as *const f32, frames)).collect();
            let _ = tx.send(to_pcm(&chans, f.sampleRate()));
        });
        input.installTapOnBus_bufferSize_format_block(0, 4800, Some(&format), RcBlock::as_ptr(&tap));
        engine.prepare();
        engine.startAndReturnError().map_err(|_| "macOS did not allow Sora to use the microphone. Turn Sora on in System Settings > Privacy & Security > Microphone.".to_string())?;
        Ok(Capture::Mic(engine, tap))
    }
}

/// One capture loop, as on Windows: takes what the source sends, writes it (dropped while paused), pads gaps where the
/// source sent nothing (macOS sends no audio while nothing plays), and stops on request or on a write error.
pub(super) fn run_capture(source: Source, path: PathBuf, live: Arc<Live>, ready: mpsc::Sender<Result<(), String>>, is_mic: bool) -> Result<PathBuf, String> {
    let (tx, rx) = mpsc::channel::<Pcm>();
    let capture = match source {
        Source::Mic => open_mic(tx),
        Source::System => open_screen(tx, None),
        Source::Process(pid) => open_screen(tx, Some(pid)),
    };
    let capture = match capture { Ok(c) => c, Err(e) => { let _ = ready.send(Err(e.clone())); return Err(e) } };
    let mut wav = match WavWriter::create(&path) { Ok(w) => w, Err(e) => { let m = format!("Could not create the file: {e}"); let _ = ready.send(Err(m.clone())); return Err(m) } };
    let _ = ready.send(Ok(()));
    let mut active_since = Instant::now();
    let (mut written_frames, mut active_ms_before, mut was_paused) = (0u64, 0u64, false);
    let zeros = vec![0u8; (RATE as usize / 10) * BLOCK as usize];
    let mut failure: Option<String> = None;
    let mut last_check = Instant::now();
    while !live.stop.load(Ordering::SeqCst) {
        if let Source::Process(pid) = source {
            if last_check.elapsed() > Duration::from_secs(1) { last_check = Instant::now(); if !alive(pid) { failure = Some("The meeting app closed, so the recording stopped.".into()); break; } }
        }
        let first = rx.recv_timeout(Duration::from_millis(200)).ok();
        let paused = live.paused.load(Ordering::SeqCst);
        if paused != was_paused {
            if paused { active_ms_before += active_since.elapsed().as_millis() as u64; } else { active_since = Instant::now(); written_frames = active_ms_before * RATE as u64 / 1000; }
            was_paused = paused;
        }
        let mut peak = 0u16;
        for (bytes, p) in first.into_iter().chain(rx.try_iter()) {
            if paused { continue; }
            peak = peak.max(p);
            if let Err(e) = wav.write(&bytes) { failure = Some(write_error(&e)); break; }
            written_frames += (bytes.len() / BLOCK as usize) as u64;
        }
        if failure.is_some() { break; }
        (if is_mic { &live.mic_level } else { &live.level }).store(peak as u32, Ordering::SeqCst);
        if !paused {
            let expected = (active_ms_before + active_since.elapsed().as_millis() as u64) * RATE as u64 / 1000;
            if expected > written_frames + RATE as u64 / 5 {
                let gap = ((expected - written_frames - RATE as u64 / 20) as usize).min(RATE as usize * 2) * BLOCK as usize;
                if let Err(e) = write_zeros(&mut wav, gap, &zeros) { failure = Some(write_error(&e)); break; }
                written_frames += (gap / BLOCK as usize) as u64;
            }
            if !is_mic { live.recorded_ms.store(active_ms_before + active_since.elapsed().as_millis() as u64, Ordering::SeqCst); }
        }
    }
    drop(capture);
    if let Some(f) = &failure { if let Ok(mut e) = live.error.lock() { if e.is_empty() { *e = f.clone(); } } live.stop.store(true, Ordering::SeqCst); }
    wav.finish().map_err(|e| format!("Could not finish the file: {e}"))
}
pub(super) fn documents() -> PathBuf { std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents")).unwrap_or_else(|| PathBuf::from(".")) }
/// Compression runs at a lower priority, so the Mac stays quick meanwhile.
pub(super) fn below_normal(child: &std::process::Child) { unsafe { libc::setpriority(libc::PRIO_PROCESS, child.id(), 10); } }
/// "2026-11-12 14-03" in local time, for file names.
pub fn local_stamp() -> String {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() { return "Meeting".into(); }
        format!("{:04}-{:02}-{:02} {:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
    }
}

#[cfg(test)]
mod mac_tests {
    #[test]
    fn mac_apps_take_their_windows_names() {
        assert_eq!((super::known_as("zoom.us"), super::known_as("google chrome"), super::known_as("msteams"), super::known_as("finder")), ("zoom.exe", "chrome.exe", "ms-teams.exe", "finder"));
    }
    #[test]
    fn floats_become_48k_stereo() {
        // Mono 24 kHz: twice the frames, both sides the same; full scale clamps to 32767.
        let (pcm, peak) = super::to_pcm(&[&[0.0, 1.0]], 24_000.0);
        let s: Vec<i16> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(s.len(), 8);
        assert_eq!((s[0], s[1], s[2], s[3], peak), (0, 0, 16383, 16383, 32767));
    }
}
