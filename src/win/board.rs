//! The Windows side of board (board.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::{w, PCWSTR}, Win32::{Foundation::{HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
    UI::WindowsAndMessaging::*}};
pub(super) unsafe fn format_named(name: PCWSTR) -> u32 { RegisterClipboardFormatW(name) }
pub(super) unsafe fn bytes_of(format: u32) -> Option<Vec<u8>> {
    let h = GetClipboardData(format).ok()?;
    let g = HGLOBAL(h.0);
    let size = GlobalSize(g);
    let p = GlobalLock(g) as *const u8;
    if p.is_null() { return None; }
    let v = std::slice::from_raw_parts(p, size).to_vec();
    let _ = GlobalUnlock(g);
    Some(v)
}
pub(super) unsafe fn text_of() -> Option<String> {
    let b = bytes_of(13)?; // CF_UNICODETEXT
    let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}
/// A clipboard bitmap as an image: the PNG format when there is one (keeps transparency), else the device-independent bitmap.
pub(super) unsafe fn picture_of() -> Option<image::DynamicImage> {
    let png = format_named(w!("PNG"));
    if png != 0 && IsClipboardFormatAvailable(png).is_ok() {
        if let Some(b) = bytes_of(png) { if let Ok(i) = image::load_from_memory_with_format(&b, image::ImageFormat::Png) { return Some(i); } }
    }
    let dib = bytes_of(8)?; // CF_DIB
    if dib.len() < 40 { return None; }
    let header = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    let bits = u16::from_le_bytes(dib[14..16].try_into().ok()?) as usize;
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    let used = u32::from_le_bytes(dib[32..36].try_into().ok()?) as usize;
    let masks = if header == 40 && compression == 3 { 12 } else { 0 };
    let palette = if used > 0 { used * 4 } else if bits <= 8 { (1usize << bits) * 4 } else { 0 };
    let mut bmp = Vec::with_capacity(dib.len() + 14);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&((14 + header + masks + palette) as u32).to_le_bytes());
    bmp.extend_from_slice(&dib);
    // A screenshot's 32-bit pixels often have no alpha set: take it as opaque.
    image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok().map(|i| image::DynamicImage::ImageRgb8(i.to_rgb8()))
}
pub(super) unsafe fn capture() {
    if now_ms() < SKIP_UNTIL.load(Ordering::SeqCst) { return; }
    if with(|s| s.capture.unwrap_or(true)) != Some(true) { return; }
    let mut opened = false;
    for _ in 0..6 { if OpenClipboard(HWND::default()).is_ok() { opened = true; break; } thread::sleep(std::time::Duration::from_millis(25)); }
    if !opened { return; }
    // Password managers and the like mark what must not be kept.
    let excluded = IsClipboardFormatAvailable(format_named(w!("ExcludeClipboardContentFromMonitorProcessing"))).is_ok()
        || bytes_of(format_named(w!("CanIncludeInClipboardHistory"))).is_some_and(|b| b.len() >= 4 && b[..4] == [0, 0, 0, 0]);
    let content = if excluded { None } else if IsClipboardFormatAvailable(13).is_ok() { text_of().map(Err) } else { picture_of().map(Ok) };
    let _ = CloseClipboard();
    match content { Some(Err(text)) => add_text(text, source(false)), Some(Ok(img)) => add_image(img, source(true)), None => {} }
}
/// The file of the program that owns a window; nothing for Sora itself.
pub(super) unsafe fn exe_of(hwnd: HWND) -> Option<String> {
    use windows::{core::PWSTR, Win32::{Foundation::CloseHandle, System::Threading::*}};
    if hwnd.0.is_null() { return None; }
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == std::process::id() { return None; }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(process);
    let path = ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))?;
    // A Store app (Calculator, Photos) sits inside a frame window of ApplicationFrameHost: the app is the child of another process.
    if file_of(&path) == "applicationframehost.exe" {
        unsafe extern "system" fn pick(h: HWND, lp: LPARAM) -> windows::Win32::Foundation::BOOL {
            let found = &mut *(lp.0 as *mut (u32, HWND));
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid != found.0 { found.1 = h; return false.into(); }
            true.into()
        }
        let mut found = (pid, HWND::default());
        let _ = EnumChildWindows(hwnd, Some(pick), LPARAM(&mut found as *mut _ as isize));
        return if found.1.0.is_null() { Some(path) } else { exe_of(found.1) };
    }
    Some(path)
}
/// The name Windows shows for a program (its file description, "Google Chrome"), else its file name ("Chrome").
pub(super) fn app_name(path: &str) -> String {
    use windows::{core::HSTRING, Win32::{System::Com::*, UI::Shell::{*, PropertiesSystem::PROPERTYKEY}}};
    // System.FileDescription
    const DESCRIPTION: PROPERTYKEY = PROPERTYKEY { fmtid: windows::core::GUID::from_u128(0x0cef7d53_fa64_11d1_a203_0000f81fedee), pid: 3 };
    let described = unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let name = SHCreateItemFromParsingName::<_, _, IShellItem2>(&HSTRING::from(path), None).and_then(|i| i.GetString(&DESCRIPTION))
            .ok().map(|p| { let s = p.to_string().unwrap_or_default(); CoTaskMemFree(Some(p.0 as _)); s });
        if initialized { CoUninitialize(); }
        name
    };
    described.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| {
        let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let mut c = stem.chars();
        c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
    })
}
/// (name, file) of the program a copy came from: the clipboard's owner when it is an app. Else the app in front, as Supaste does
/// on a Mac: an app that copies with no owner window (Lark), a Windows relay (the clipboard service, Win+V), or a picture
/// from a screenshot tool (the snip has closed by now, and the window in front is what it shows).
pub(super) fn source(picture: bool) -> (String, String) {
    let owner = unsafe { GetClipboardOwner().ok().and_then(|h| exe_of(h)) };
    let front = || unsafe { exe_of(GetForegroundWindow()) }.filter(|p| !is_relay(p) && !is_screenshot_tool(p));
    let path = match owner {
        Some(o) if !is_relay(&o) && !(picture && is_screenshot_tool(&o)) => Some(o),
        o => front().or(o.filter(|p| !is_relay(p))),
    };
    path.map(|p| (app_name(&p), p)).unwrap_or_default()
}
/// The lines in a picture with their boxes, in the first language of the Windows profile that has OCR installed.
/// None when Windows has no OCR language or the picture is too big for it.
pub(super) fn ocr_lines(img: &image::DynamicImage) -> Option<Vec<OcrLine>> {
    use windows::{Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap}, Media::Ocr::OcrEngine, Storage::Streams::DataWriter};
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    // shortcut: a picture larger than OCR takes (about 10000 px) is skipped, not scaled; scale it if very long screenshots matter.
    let max = OcrEngine::MaxImageDimension().ok()?;
    if w > max || h > max { return None; }
    let mut bgra = rgba.into_raw();
    for px in bgra.chunks_exact_mut(4) { px.swap(0, 2); }
    let run = || -> windows::core::Result<Vec<OcrLine>> {
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(&writer.DetachBuffer()?, BitmapPixelFormat::Bgra8, w as i32, h as i32)?;
        let result = engine.RecognizeAsync(&bitmap)?.get()?;
        let mut out = Vec::new();
        for line in result.Lines()? {
            let mut words = Vec::new();
            for word in line.Words()? {
                let r = word.BoundingRect()?;
                words.push(OcrWord { text: word.Text()?.to_string(), x: r.X, y: r.Y, w: r.Width, h: r.Height });
            }
            let text = line.Text()?.to_string();
            if !words.is_empty() && !text.trim().is_empty() { out.push(OcrLine { text, words }); }
        }
        Ok(out)
    };
    run().ok()
}
pub(super) unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // A short wait, so the program that is still writing the clipboard (it may set several formats) is done and not blocked.
    if msg == WM_CLIPBOARDUPDATE { SetTimer(hwnd, 1, 120, None); return LRESULT(0); }
    if msg == WM_TIMER && wp.0 == 1 { let _ = KillTimer(hwnd, 1); capture(); return LRESULT(0); }
    DefWindowProcW(hwnd, msg, wp, lp)
}
pub(super) fn listen() {
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let class = w!("SoraBoardListener");
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: HINSTANCE(module.0), lpszClassName: class, ..Default::default() };
        if RegisterClassW(&wc) == 0 { return; }
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, HINSTANCE(module.0), None) else { return };
        if AddClipboardFormatListener(hwnd).is_err() { return; }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() { let _ = TranslateMessage(&msg); DispatchMessageW(&msg); }
    }
}
/// The text on the clipboard now: read here, so the page never asks Windows for clipboard access.
#[tauri::command]
pub fn board_clip_text() -> Option<String> {
    unsafe {
        let mut opened = false;
        for _ in 0..6 { if OpenClipboard(HWND::default()).is_ok() { opened = true; break; } thread::sleep(std::time::Duration::from_millis(25)); }
        if !opened { return None; }
        let text = text_of();
        let _ = CloseClipboard();
        text.map(|t| t.trim().chars().take(2048).collect())
    }
}
/// Puts some text on the clipboard (a chat message copied from the history); the board does not take it as a new copy.
#[tauri::command]
pub fn board_put_text(text: String) -> AppResult<()> {
    SKIP_UNTIL.store(now_ms() + 1500, Ordering::SeqCst);
    unsafe {
        let mut opened = false;
        for _ in 0..8 { if OpenClipboard(HWND::default()).is_ok() { opened = true; break; } thread::sleep(std::time::Duration::from_millis(25)); }
        if !opened { return Err("The clipboard is busy. Try again.".into()); }
        let result = (|| -> AppResult<()> {
            EmptyClipboard().map_err(err)?;
            let mut units: Vec<u16> = text.chars().take(MAX_TEXT).collect::<String>().encode_utf16().collect();
            units.push(0);
            put(13, &units.iter().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>())
        })();
        let _ = CloseClipboard();
        result
    }
}
/// Puts the item back on the clipboard (text as text, a picture as a picture too).
#[tauri::command]
pub fn board_copy(id: String) -> AppResult<()> {
    let item = with(|s| s.items.iter().find(|i| i.id == id).cloned()).flatten().ok_or("That item is gone.")?;
    SKIP_UNTIL.store(now_ms() + 1500, Ordering::SeqCst);
    unsafe {
        let mut opened = false;
        for _ in 0..8 { if OpenClipboard(HWND::default()).is_ok() { opened = true; break; } thread::sleep(std::time::Duration::from_millis(25)); }
        if !opened { return Err("The clipboard is busy. Try again.".into()); }
        let result = (|| -> AppResult<()> {
            EmptyClipboard().map_err(err)?;
            if item.kind == "image" {
                let (png, _) = files(&id).ok_or("No picture.")?;
                let bytes = fs::read(&png).map_err(err)?;
                let img = image::load_from_memory(&bytes).map_err(err)?.to_rgb8();
                let mut bmp = Cursor::new(Vec::new());
                image::codecs::bmp::BmpEncoder::new(&mut bmp).encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8).map_err(err)?;
                let bmp = bmp.into_inner();
                put(8, &bmp[14..])?;
                let png_format = format_named(w!("PNG"));
                if png_format != 0 { put(png_format, &bytes)?; }
            } else {
                let mut units: Vec<u16> = item.text.encode_utf16().collect();
                units.push(0);
                let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
                put(13, &bytes)?;
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}
pub(super) unsafe fn put(format: u32, bytes: &[u8]) -> AppResult<()> {
    let g = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(err)?;
    let p = GlobalLock(g) as *mut u8;
    if p.is_null() { return Err("Out of memory.".into()); }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(g);
    SetClipboardData(format, HANDLE(g.0)).map(|_| ()).map_err(err)
}
