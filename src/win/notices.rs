//! The Windows side of notices (notices.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// The alarm sound: Windows' own alarm tone, played once, without waiting.
pub(super) fn ring() {
    use windows::{core::w, Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT}};
    unsafe { let _ = PlaySoundW(w!(r"C:WindowsMediaAlarm01.wav"), None, SND_FILENAME | SND_ASYNC | SND_NODEFAULT); }
}
/// No Windows fade-in: the reminder must appear at once, exactly over the capsule it grows from.
pub(super) fn no_fade(window: &WebviewWindow) {
    if let Ok(h) = window.hwnd() {
        use windows::Win32::{Foundation::{BOOL, HWND}, Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED}};
        let off = BOOL(1);
        unsafe { let _ = DwmSetWindowAttribute(HWND(h.0 as _), DWMWA_TRANSITIONS_FORCEDISABLED, &off as *const BOOL as _, std::mem::size_of::<BOOL>() as u32); }
    }
}
pub(super) fn follow_notch(window: &WebviewWindow, start: (i32, i32), attached: bool, width: i32, dpi: f64) {
    let Ok(h) = window.hwnd() else { return };
    let (raw, w, label) = (h.0 as isize, window.clone(), window.label().to_string());
    thread::spawn(move || unsafe {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{IsWindow, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER}};
        let hwnd = HWND(raw as _);
        let (mut last, mut shown) = (start, None::<serde_json::Value>);
        while IsWindow(hwnd).as_bool() {
            thread::sleep(Duration::from_millis(10));
            let Some(geom) = crate::notch_ui::notch_geometry() else { continue };
            let (at, place) = notice_place(width, dpi, geom, crate::notch_ui::capsule_rect());
            if at != last { let _ = SetWindowPos(hwnd, HWND::default(), at.0, at.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE); last = at; }
            // The page already drew the start; afterwards only changes (joined, let go, the capsule's exact spot).
            let first = shown.is_none() && place["attached"] == attached;
            if first { shown = Some(place); continue; }
            if shown.as_ref() != Some(&place) { let _ = w.emit_to(label.as_str(), "notice-place", &place); shown = Some(place); }
        }
    });
}
