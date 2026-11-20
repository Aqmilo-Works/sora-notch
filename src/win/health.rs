//! The Windows side of health (health.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::PWSTR, Win32::{Foundation::CloseHandle, System::{SystemInformation::{GetLocalTime, GetTickCount}, Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION}},
    UI::{Shell::{SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_QUIET_TIME}, Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO}, WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId}}}};
pub(super) fn today() -> String { let t = unsafe { GetLocalTime() }; format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay) }
/// A full-screen app, a game or a presentation is in front (Windows holds its own notifications then too).
pub(super) fn busy() -> bool { unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| s != QUNS_ACCEPTS_NOTIFICATIONS && s != QUNS_QUIET_TIME) }
pub(super) fn idle_secs() -> u32 {
    let mut i = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut i) }.as_bool() { unsafe { GetTickCount() }.wrapping_sub(i.dwTime) / 1000 } else { 0 }
}
/// The program in front, by its file name (not Sora itself).
pub(super) fn foreground_app() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() { return None; }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == std::process::id() { return None; }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        ok.then(|| pretty(&String::from_utf16_lossy(&buf[..len as usize])))
    }
}
