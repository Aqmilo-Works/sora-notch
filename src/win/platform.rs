//! The Windows backend of `platform`: window frame and region, shell open and pictures, the media session, mouse state.
use std::{sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
use base64::Engine;
use windows::{core::{HSTRING, PCWSTR}, Media::Control::{GlobalSystemMediaTransportControlsSessionManager as MediaManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus}, Storage::Streams::{DataReader, InputStreamOptions}, Win32::{Foundation::HWND, UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL}}};
use crate::{err, AppResult, Media};

fn hwnd(window: &tauri::WebviewWindow) -> AppResult<HWND> { Ok(HWND(window.hwnd().map_err(err)?.0 as _)) }

/// Undecorated Tauri windows keep WS_CAPTION (tao re-applies it on every show, hide or click-through change). While the notch window
/// has a region DWM does not draw its frame, so Windows itself paints a classic title bar over the page for a frame when the window
/// activates or its region changes. The caption styles are removed for good (tao's own style updates are filtered on the way in),
/// so there is no frame left to paint; non-client painting is refused as well.
pub(crate) fn hide_frame(window: &tauri::WebviewWindow) {
    let Ok(hwnd) = hwnd(window) else { return };
    use windows::Win32::{Foundation::{LPARAM, LRESULT, WPARAM}, UI::{Shell::{DefSubclassProc, SetWindowSubclass}, WindowsAndMessaging::*}};
    const FRAME: u32 = WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0;
    // Undocumented messages the theme engine uses to draw the caption and frame directly.
    const WM_NCUAHDRAWCAPTION: u32 = 0xAE;
    const WM_NCUAHDRAWFRAME: u32 = 0xAF;
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
        match msg {
            WM_STYLECHANGING if wp.0 as i32 == GWL_STYLE.0 => {
                let s = &mut *(lp.0 as *mut STYLESTRUCT);
                s.styleNew = (s.styleNew & !FRAME) | WS_POPUP.0;
                LRESULT(0)
            }
            WM_NCPAINT | WM_NCUAHDRAWCAPTION | WM_NCUAHDRAWFRAME => LRESULT(0),
            // lParam -1: update the active state without repainting the non-client area.
            WM_NCACTIVATE => DefSubclassProc(hwnd, msg, wp, LPARAM(-1)),
            _ => DefSubclassProc(hwnd, msg, wp, lp),
        }
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(proc), 0x50_52, 0);
        SetWindowLongPtrW(hwnd, GWL_STYLE, (GetWindowLongPtrW(hwnd, GWL_STYLE) & !(FRAME as isize)) | WS_POPUP.0 as isize);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
/// Open: the whole window. Closed: only the mini notch (292 x 64 design px plus its 13 px corner wings).
fn clip(hwnd: HWND, expanded: bool, width: i32, k: f64) {
    use windows::Win32::Graphics::Gdi::{CombineRgn, CreateEllipticRgn, CreateRectRgn, SetWindowRgn, HRGN, RGN_DIFF, RGN_OR};
    unsafe {
        if expanded { SetWindowRgn(hwnd, HRGN::default(), true); return }
        // Transparent pixels inside a window region show up white, so the region follows the notch exactly: the body plus the two
        // ears, each ear being its box minus the circle that rounds it (same numbers as the page's .notch:before/:after).
        let s = |v: f64| (v * k).round() as i32;
        let (l, r) = ((width - s(292.0)) / 2, (width + s(292.0)) / 2);
        let rgn = CreateRectRgn(l, 0, r, s(66.0));
        let ear = |x0: i32, cx: i32| {
            let e = CreateRectRgn(x0, 0, x0 + s(13.0), s(13.0));
            let c = CreateEllipticRgn(cx - s(12.0), s(1.0), cx + s(12.0), s(25.0));
            CombineRgn(e, e, c, RGN_DIFF);
            CombineRgn(rgn, rgn, e, RGN_OR);
        };
        ear(l - s(12.0), l - s(12.0));
        ear(r - s(1.0), r + s(12.0));
        SetWindowRgn(hwnd, rgn, true);
    }
}
/// Windows brings a shown window in front already.
pub(crate) fn show_on_top(_: &tauri::WebviewWindow) {}
/// Moves and sizes the notch window in one step (physical px), then clips it to the mini notch when closed (`k`: design px to physical px).
pub(crate) fn place_window(window: &tauri::WebviewWindow, x: i32, y: i32, width: u32, height: u32, expanded: bool, k: f64) -> AppResult<()> {
    // Move and resize together so an Explorer drag never sees an off-centre drop target.
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::{GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER}};
    let hwnd = hwnd(window)?;
    // Every open and close lands here: an unchanged window is not moved again (that would make WebView2 re-layout mid-morph).
    let mut now = RECT::default();
    let same = unsafe { GetWindowRect(hwnd, &mut now) }.is_ok() && now == RECT { left: x, top: y, right: x + width as i32, bottom: y + height as i32 };
    if !same { unsafe { SetWindowPos(hwnd, None, x, y, width as i32, height as i32, SWP_NOACTIVATE | SWP_NOZORDER) }.map_err(err)?; }
    clip(hwnd, expanded, width as i32, k);
    Ok(())
}

/// Opens a file or folder with its default app.
pub(crate) fn open_path(target: &str) -> AppResult<()> {
    let target = HSTRING::from(target);
    let verb = HSTRING::from("open");
    let result = unsafe { ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(target.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    if result.0 as isize <= 32 { Err("Windows could not open this file".into()) } else { Ok(()) }
}

/// The media session manager. The page polls every 750 ms, and a fresh RequestAsync each time is a round trip to the Windows
/// media service (a CPU blip per poll), so one manager is reused; it is renewed each minute in case the service restarted.
fn media_manager() -> AppResult<MediaManager> {
    static MANAGER: Mutex<Option<(MediaManager, std::time::Instant)>> = Mutex::new(None);
    let mut slot = MANAGER.lock().map_err(err)?;
    if let Some((m, at)) = slot.as_ref() { if at.elapsed() < std::time::Duration::from_secs(60) { return Ok(m.clone()); } }
    let m = MediaManager::RequestAsync().map_err(err)?.get().map_err(err)?;
    *slot = Some((m.clone(), std::time::Instant::now()));
    Ok(m)
}
pub(crate) fn media_read(with_art: bool) -> AppResult<Option<Media>> {
    let manager = media_manager()?;
    let session = match manager.GetCurrentSession() { Ok(s) => s, Err(_) => return Ok(None) };
    let info = session.GetPlaybackInfo().map_err(err)?;
    let status = info.PlaybackStatus().map_err(err)?;
    if matches!(status, PlaybackStatus::Closed | PlaybackStatus::Stopped) { return Ok(None); }
    let properties = session.TryGetMediaPropertiesAsync().map_err(err)?.get().map_err(err)?;
    let controls = info.Controls().map_err(err)?;
    let playing = status == PlaybackStatus::Playing;
    let timeline = session.GetTimelineProperties().map_err(err)?;
    let start = timeline.StartTime().map_err(err)?.Duration as f64 / 10_000_000.0;
    let duration = (timeline.EndTime().map_err(err)?.Duration as f64 / 10_000_000.0 - start).max(0.0);
    let mut position = timeline.Position().map_err(err)?.Duration as f64 / 10_000_000.0 - start;
    let playback_rate = info.PlaybackRate().and_then(|r| r.Value()).ok().filter(|r| r.is_finite() && *r > 0.0 && *r <= 8.0).unwrap_or(1.0);
    if playing {
        let updated = timeline.LastUpdatedTime().map_err(err)?.UniversalTime as f64 / 10_000_000.0 - 11644473600.0;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(err)?.as_secs_f64();
        let age = now - updated;
        if updated > 0.0 && age >= 0.0 && age <= duration.max(60.0) { position += age * playback_rate; }
    }
    // The cover is read (and base64-encoded) once per track; later polls that ask for it get the same string back.
    static ART: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));
    let key = format!("{}|{}|{}|{}", properties.Title().unwrap_or_default(), properties.Artist().unwrap_or_default(), properties.AlbumTitle().unwrap_or_default(), session.SourceAppUserModelId().unwrap_or_default());
    let cached = if with_art { ART.lock().ok().filter(|a| a.0 == key && !a.1.is_empty()).map(|a| a.1.clone()) } else { None };
    let artwork = if cached.is_some() { cached } else if with_art { (|| -> windows::core::Result<String> {
        let stream = properties.Thumbnail()?.OpenReadAsync()?.get()?;
        let size = stream.Size()?.min(4 * 1024 * 1024) as u32;
        if size == 0 { return Ok(String::new()); }
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        reader.SetInputStreamOptions(InputStreamOptions::None)?;
        reader.LoadAsync(size)?.get()?;
        let mut bytes = vec![0; size as usize];
        reader.ReadBytes(&mut bytes)?;
        let mime = stream.ContentType()?.to_string();
        Ok(format!("data:{};base64,{}", if mime.starts_with("image/") { &mime } else { "image/png" }, base64::engine::general_purpose::STANDARD.encode(bytes)))
    })().ok().filter(|a| !a.is_empty()) } else { None };
    if let Some(art) = &artwork { if let Ok(mut a) = ART.lock() { if a.0 != key { *a = (key, art.clone()); } } }
    Ok(Some(Media { title: properties.Title().map_err(err)?.to_string(), artist: properties.Artist().map_err(err)?.to_string(), album: properties.AlbumTitle().map_err(err)?.to_string(), source: session.SourceAppUserModelId().map_err(err)?.to_string(), playing, position: position.clamp(0.0, duration.max(0.0)), duration, playback_rate, artwork, can_seek: controls.IsPlaybackPositionEnabled().unwrap_or(false), can_play: controls.IsPlayPauseToggleEnabled().unwrap_or(false), can_next: controls.IsNextEnabled().unwrap_or(false), can_previous: controls.IsPreviousEnabled().unwrap_or(false) }))
}
/// Drives the current Windows media session: "toggle", "next", "previous" or "seek" (position in seconds).
pub(crate) fn media_command(action: &str, position: Option<f64>) -> AppResult<()> {
        let session = media_manager()?.GetCurrentSession().map_err(err)?;
        let accepted = match action {
            "toggle" => session.TryTogglePlayPauseAsync().map_err(err)?.get().map_err(err)?,
            "next" => session.TrySkipNextAsync().map_err(err)?.get().map_err(err)?,
            "previous" => session.TrySkipPreviousAsync().map_err(err)?.get().map_err(err)?,
            "seek" => {
                let p = position.unwrap_or(0.0);
                if !p.is_finite() || p < 0.0 { return Err("Invalid position".into()); }
                let timeline = session.GetTimelineProperties().map_err(err)?;
                let start = timeline.StartTime().map_err(err)?.Duration;
                let end = timeline.EndTime().map_err(err)?.Duration;
                let target = start.saturating_add((p * 10_000_000.0) as i64).clamp(start, end.max(start));
                session.TryChangePlaybackPositionAsync(target).map_err(err)?.get().map_err(err)?
            },
            _ => return Err("Unknown control".into()),
        };
        if accepted { Ok(()) } else { Err("This player does not support that control".into()) }
}
/// The icon of the app playing now, as a PNG data URL; `source` must still be the current session's app.
pub(crate) fn media_source_icon(source: String) -> AppResult<String> {
    let session = media_manager()?.GetCurrentSession().map_err(err)?;
    let current = session.SourceAppUserModelId().map_err(err)?.to_string();
    if source != current || source.is_empty() { return Err("Media source changed".into()); }
    if std::path::Path::new(&source).is_absolute() && std::path::Path::new(&source).is_file() {
        if let Ok(icon) = shell_image(source.clone(), true) { return Ok(icon); }
    }
    shell_image(format!("shell:AppsFolder\\{}", source), true)
}

/// Windows' own picture for a path (thumbnail, or the icon only) at a chosen size, as BGRA rows top-down
/// (premultiplied, as the shell hands it out; jumbo icons go up to 256 px).
fn shell_pixels_sized(path: String, icon_only: bool, size: i32) -> AppResult<(u32, u32, Vec<u8>)> {
    unsafe {
        use windows::Win32::{System::Com::*, UI::Shell::*, Foundation::SIZE, Graphics::Gdi::*};
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> AppResult<(u32, u32, Vec<u8>)> {
            let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).map_err(err)?;
            let bitmap = factory.GetImage(SIZE {cx:size,cy:size}, if icon_only { SIIGBF_BIGGERSIZEOK | SIIGBF_ICONONLY } else { SIIGBF_BIGGERSIZEOK }).map_err(err)?;
            let mut desc = BITMAP::default();
            GetObjectW(bitmap, std::mem::size_of::<BITMAP>() as i32, Some(&mut desc as *mut _ as _));
            let w=desc.bmWidth as u32; let h=desc.bmHeight.unsigned_abs();
            let mut info=BITMAPINFO::default();info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;info.bmiHeader.biWidth=w as i32;info.bmiHeader.biHeight=-(h as i32);info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB.0;
            let dc=CreateCompatibleDC(None);let mut bytes=vec![0u8;(w*h*4) as usize];
            let rows=GetDIBits(dc,bitmap,0,h,Some(bytes.as_mut_ptr() as _),&mut info,DIB_RGB_COLORS);
            let _=DeleteDC(dc);let _=DeleteObject(bitmap);
            if rows==0 { return Err("Thumbnail unavailable".into()); }
            // Pictures without an alpha channel come back with alpha 0 everywhere: they are opaque.
            if bytes.chunks_exact(4).all(|p|p[3]==0) { for pixel in bytes.chunks_exact_mut(4) { pixel[3]=255; } }
            Ok((w,h,bytes))
        })();
        if initialized {CoUninitialize();} result
    }
}
/// The shell's picture for a path as a PNG data URL: the icon at 256 px, or a 128 px thumbnail.
pub(crate) fn shell_image(path: String, icon_only: bool) -> AppResult<String> {
    let (w,h,mut bytes)=shell_pixels_sized(path,icon_only,if icon_only {256} else {128})?;
    // The shell premultiplies alpha; PNG wants it straight, otherwise soft edges come out dark and jagged.
    for pixel in bytes.chunks_exact_mut(4) { pixel.swap(0,2); let a=pixel[3] as u32; if a>0&&a<255 { for c in &mut pixel[..3] { *c=((*c as u32*255+a/2)/a).min(255) as u8; } } }
    let img=image::RgbaImage::from_raw(w,h,bytes).ok_or("Invalid thumbnail")?;
    let mut png=std::io::Cursor::new(Vec::new());img.write_to(&mut png,image::ImageFormat::Png).map_err(err)?;
    Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(png.into_inner())))
}

/// Newer chat helpers come from the signed manifest (chat_updates.rs).
pub(crate) const CHAT_HELPER_UPDATES: bool = true;
/// The tray icon stays hidden: the notch itself opens Settings and quits.
pub(crate) const TRAY_VISIBLE: bool = false;
/// The Beta features (Meeting, the chat apps) are offered.
pub(crate) const BETA: bool = true;
/// Windows lets the notch start at the very top of the display (physical px below its top edge).
pub(crate) fn top_inset(_monitor: &tauri::Monitor) -> i32 { 0 }
/// The program a self-update replaced (Sora.exe.old) is removed on the next start.
pub(crate) fn remove_old_update() { if let Ok(exe) = std::env::current_exe() { let mut old = exe.into_os_string(); old.push(".old"); let _ = std::fs::remove_file(old); } }
/// The portable Sora.exe replaces itself from a signed GitHub release (update.rs).
pub(crate) const SELF_UPDATES: bool = true;

/// Deletes `paths` (folders and files) once this Sora has exited (uninstall.rs): a hidden PowerShell waits for this
/// process, then tries for up to 15 s while WebView2 lets go of its files.
pub(crate) fn remove_after_exit(paths: &[std::path::PathBuf]) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    let list = paths.iter().map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''"))).collect::<Vec<_>>().join(",");
    let script = format!("Wait-Process -Id {} -ErrorAction SilentlyContinue; $p=@({list}); for($i=0;$i -lt 30;$i++){{ $p=@($p|?{{Test-Path -LiteralPath $_}}); if(!$p.Count){{break}}; foreach($x in $p){{Remove-Item -LiteralPath $x -Recurse -Force -ErrorAction SilentlyContinue}}; Start-Sleep -Milliseconds 500 }}", std::process::id());
    let system = std::env::var_os("SystemRoot").ok_or("Windows directory unavailable")?;
    std::process::Command::new(std::path::PathBuf::from(system).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(0x08000000 | 0x00000008).spawn().map_err(|e| format!("Could not start the uninstall ({e})."))?;
    Ok(())
}
/// A child process started with no console window (CREATE_NO_WINDOW).
pub(crate) fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}
/// Left, right and middle mouse buttons held right now (read only: clicks still go to the app under the cursor).
pub(crate) fn mouse_down() -> [bool; 3] {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON, VK_MBUTTON};
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].map(|key| unsafe { GetAsyncKeyState(key.0 as i32) < 0 })
}
/// The cursor in physical screen px.
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }.ok().map(|_| (cursor.x, cursor.y))
}
