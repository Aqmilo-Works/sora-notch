//! The macOS side of desktop (desktop.rs), its `os` module. Everything else there is shared.
//! The shortcut is watched by polling the keyboard state on the desktop loop's 40 ms tick (no event tap, so no permission
//! prompt in most cases), and the edge handle's hover works from the pointer position.
#![allow(unused_imports, unused_unsafe)]
use super::*;
use std::collections::HashMap;

/// Registered shortcuts: id -> (Win32 modifier bits from `shortcut`, macOS virtual key code), and whether each was held.
static KEYS: Mutex<Option<HashMap<i32, (u32, u16, bool)>>> = Mutex::new(None);

/// The macOS virtual key code (kVK_*) for a Win32 virtual key from `shortcut` (pure; tested).
pub(super) fn mac_key(vk: u32) -> Option<u16> {
    const LETTERS: [u16; 26] = [0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23, 0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06];
    const DIGITS: [u16; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
    const FKEYS: [u16; 20] = [0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F, 0x69, 0x6B, 0x71, 0x6A, 0x40, 0x4F, 0x50, 0x5A];
    Some(match vk {
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x70..=0x83 => FKEYS[(vk - 0x70) as usize],
        0x20 => 0x31, 0x0D => 0x24, 0x09 => 0x30, 0x08 => 0x33, 0x2E => 0x75,
        0x24 => 0x73, 0x23 => 0x77, 0x21 => 0x74, 0x22 => 0x79,
        0x25 => 0x7B, 0x26 => 0x7E, 0x27 => 0x7C, 0x28 => 0x7D,
        _ => return None,
    })
}
pub(super) fn register(value:&str,id:i32)->Result<bool,String>{
    let Some((mods, vk)) = shortcut(value)? else { return Ok(false) };
    let key = mac_key(vk).ok_or("That key is not on a Mac keyboard. Choose another one.")?;
    let mut k = KEYS.lock().map_err(|e| e.to_string())?;
    let map = k.get_or_insert_with(HashMap::new);
    if map.iter().any(|(other, v)| *other != id && (v.0, v.1) == (mods, key)) { return Err("That shortcut is already in use. Choose another combination.".into()); }
    map.insert(id, (mods, key, true));
    Ok(true)
}
pub(super) fn unregister(id:i32){ if let Ok(mut k) = KEYS.lock() { if let Some(m) = k.as_mut() { m.remove(&id); } } }
/// Whether the modifiers held now are exactly the shortcut's, no more and no fewer (pure; tested). `flags` are
/// kCGEventFlagMask* bits, `mods` Win32's MOD_ALT 1 (Option), MOD_CONTROL 2, MOD_SHIFT 4, MOD_WIN 8 (Command).
pub(super) fn modifiers_match(mods: u32, flags: u64) -> bool {
    const CONTROL: u64 = 0x40000; const OPTION: u64 = 0x80000; const SHIFT: u64 = 0x20000; const COMMAND: u64 = 0x100000;
    (flags & CONTROL != 0) == (mods & 2 != 0) && (flags & OPTION != 0) == (mods & 1 != 0) && (flags & SHIFT != 0) == (mods & 4 != 0) && (flags & COMMAND != 0) == (mods & 8 != 0)
}
/// 1 when shortcut `id` was just pressed (the key went down with its modifiers held), else 0.
pub(super) fn hotkey_hits(id:i32)->usize{
    let Ok(mut k) = KEYS.lock() else { return 0 };
    let Some((mods, key, held)) = k.as_mut().and_then(|m| m.get_mut(&id)) else { return 0 };
    let now = crate::platform::key_down(*key) && modifiers_match(*mods, crate::platform::modifier_flags());
    // Starts as held, so a shortcut that was down when registered does not fire at once.
    let hit = now && !*held;
    *held = now;
    hit as usize
}
pub(super) fn window_handle(w:&tauri::WebviewWindow)->Option<tauri::WebviewWindow>{Some(w.clone())}
pub(super) fn own_window(app:&tauri::AppHandle)->Option<tauri::WebviewWindow>{app.get_webview_window("main")}
/// Whether the front app (NSWorkspace) has a window filling the display (CGWindowListCopyWindowInfo): full screen, which
/// Hide over full screen hides the notch for. None while a Sora window is in front.
pub(super) fn foreground(own:tauri::WebviewWindow)->Option<(bool,bool)>{
    use objc2_app_kit::NSWorkspace;
    let Some(app) = (unsafe { NSWorkspace::sharedWorkspace().frontmostApplication() }) else { return Some((false, false)) };
    let pid = unsafe { app.processIdentifier() };
    // Settings (or another Sora window) is in front: the notch keeps whatever state it had.
    if pid as u32 == std::process::id() { return None; }
    let monitor = own.current_monitor().ok().flatten()?;
    let k = monitor.scale_factor();
    let to_pts = |v: i32| v as f64 / k;
    let (screen, work) = (monitor.size(), monitor.work_area());
    let display = (to_pts(monitor.position().x), to_pts(monitor.position().y), screen.width as f64 / k, screen.height as f64 / k);
    let usable = (to_pts(work.position.x), to_pts(work.position.y), work.size.width as f64 / k, work.size.height as f64 / k);
    let Some(front) = front_window(pid) else { return Some((false, false)) };
    // A zoomed window leaves the menu bar (and the notch in it) alone: on a Mac only full screen counts as either.
    let _ = usable;
    let full = covers(front, display);
    Some((full, full))
}
/// Whether a window (x, y, w, h in points) covers an area, give or take 2 points (pure; tested).
pub(super) fn covers(win: (f64, f64, f64, f64), area: (f64, f64, f64, f64)) -> bool {
    win.0 <= area.0 + 2.0 && win.1 <= area.1 + 2.0 && win.0 + win.2 >= area.0 + area.2 - 2.0 && win.1 + win.3 >= area.1 + area.3 - 2.0
}
/// The bounds (points, top-left origin) of the frontmost normal window of app `pid`, from the window server's list.
fn front_window(pid: i32) -> Option<(f64, f64, f64, f64)> {
    use std::ffi::c_void;
    #[repr(C)] #[derive(Default, Clone, Copy)] struct Rect { x: f64, y: f64, w: f64, h: f64 }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative: u32) -> *const c_void;
        fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut Rect) -> bool;
        static kCGWindowOwnerPID: *const c_void;
        static kCGWindowLayer: *const c_void;
        static kCGWindowBounds: *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(a: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
        fn CFDictionaryGetValue(d: *const c_void, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(n: *const c_void, kind: isize, out: *mut c_void) -> bool;
        fn CFRelease(cf: *const c_void);
    }
    const ON_SCREEN_ONLY: u32 = 1; const EXCLUDE_DESKTOP: u32 = 16; const SINT32: isize = 3;
    unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if list.is_null() { return None; }
        let num = |d: *const c_void, key: *const c_void| -> Option<i32> {
            let v = CFDictionaryGetValue(d, key);
            let mut out = 0i32;
            (!v.is_null() && CFNumberGetValue(v, SINT32, (&mut out as *mut i32).cast())).then_some(out)
        };
        let mut found = None;
        // Front to back: the first layer-0 window of the app is the one in front.
        for i in 0..CFArrayGetCount(list) {
            let d = CFArrayGetValueAtIndex(list, i);
            if num(d, kCGWindowOwnerPID) != Some(pid) || num(d, kCGWindowLayer) != Some(0) { continue; }
            let mut r = Rect::default();
            let b = CFDictionaryGetValue(d, kCGWindowBounds);
            if !b.is_null() && CGRectMakeWithDictionaryRepresentation(b, &mut r) && r.w > 0.0 { found = Some((r.x, r.y, r.w, r.h)); break; }
        }
        CFRelease(list);
        found
    }
}
pub(super) fn pointer_on_edge(w:tauri::WebviewWindow)->bool{
    if crate::platform::mouse_down()[0] { return false; } // dragging something: not a deliberate hover
    let (Some(cursor), Ok(pos), Ok(size), Ok(k)) = (crate::platform::cursor_pos(), w.outer_position(), w.outer_size(), w.scale_factor()) else { return false };
    in_edge_zone(pos.x + size.width as i32 / 2, pos.y, k, cursor)
}
/// Files dragged from the Finder reach the notch as the window's own drop events once the pointer is over it.
#[derive(Default)]
pub(super) struct DragWatch;
impl DragWatch{pub(super) fn step(&mut self,_app:&tauri::AppHandle){}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_keys_and_modifiers() {
        assert_eq!((mac_key(0x41), mac_key(0x58), mac_key(0x20), mac_key(0x7B), mac_key(0x2D)), (Some(0x00), Some(0x07), Some(0x31), Some(0x6F), None));
        // Ctrl+Alt+Space held exactly; with Command or without Option it is not the shortcut.
        assert!(modifiers_match(2 | 1, 0x40000 | 0x80000));
        assert!(!modifiers_match(2 | 1, 0x40000) && !modifiers_match(2, 0x40000 | 0x100000) && !modifiers_match(2 | 4, 0x40000 | 0x80000));
        // Command+Shift+Space, the Mac's own: Command and Shift exactly.
        assert!(modifiers_match(8 | 4, 0x100000 | 0x20000) && !modifiers_match(8 | 4, 0x100000) && !modifiers_match(8 | 4, 0x100000 | 0x20000 | 0x80000));
        // A zoomed window covers the usable area but not the menu bar; a full-screen one covers the display.
        let (display, usable) = ((0.0, 0.0, 1512.0, 982.0), (0.0, 37.0, 1512.0, 945.0));
        assert!(covers((0.0, 37.0, 1512.0, 945.0), usable) && !covers((0.0, 37.0, 1512.0, 945.0), display) && covers((0.0, 0.0, 1512.0, 982.0), display));
        assert!(!covers((100.0, 100.0, 800.0, 600.0), usable));
    }
}
