//! The notch, drawn natively (Win32 layered window + Direct2D), so no WebView runs while the notch is closed:
//! the notch shape with its inverted top corners, one spring that morphs the mini notch into the panel and back
//! (width, height and corners move together), the edge-handle capsule it shrinks into while Sora is hidden, sliding in
//! and out of view, and the mascot's face rig (a port of ui/mascot.js: springs, blink, attention, the listening dance with
//! its music notes, the one-eye wink of the start-up greeting).
//! Home, Music and Tray are the web page in panel mode, in a WebView window that exists only while the panel is open
//! (created on open, destroyed on close). The focus timer's deadline lives here, so it chimes while the page is closed.
use std::{cell::{Cell, RefCell}, path::PathBuf, sync::{atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering}, Mutex, OnceLock}, time::{Duration, Instant}};
pub use crate::marks::{app_info, marks, MarkView};
use crate::marks::{mark_index, MARKS};
use tauri::{Emitter, Manager};
use windows::{core::w, Foundation::Numerics::Matrix3x2,
    Win32::{Foundation::*, Graphics::{Direct2D::{Common::*, *}, DirectWrite::*, Dwm::DwmFlush, Dxgi::Common::*, Gdi::*},
    Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT},
    System::{Com::*, LibraryLoader::GetModuleHandleW},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*}}};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, TrySuspendCompletedHandler};
use wvcore::Interface;
use crate::draw::{bezier, color, fill_geometry, mul, parse_path, path_geometry, pt, rand, rect, rotate, scale, translate, Spring};

/// The drawing canvas in notch px (the old CSS px): the widest panel plus the corner wings, before the notch scale.
const CW: f32 = 744.0;
/// Tall enough for the largest notice under the tallest panel (Home menu, Chat history): 234 + 150 + gap 10 + a chat card
/// being answered with six lines (three of theirs, three of one's own: ~275) + the queued cards peeking below it. At 480 a
/// chat card under Chat history was cut off.
/// The window itself only covers what is drawn, so the extra height costs nothing while it is empty.
const CH: f32 = 930.0;
/// How much taller the panel grows while one of Home's menus (a control panel) is open.
const TALL: f32 = 150.0;
/// The most a view that fits its contents (an agent's question, the Board preview) may grow: 250 more than the menus.
const TALLEST: f32 = 400.0;
/// A reminder (focus timer, calendar) drawn by the notch itself: its size and corner, the gap under the notch,
/// how long it stays. A chat notice is larger, so a message reads at a glance.
const NOTICE: (f32, f32, f32) = (330.0, 68.0, 22.0);
/// A chat card: compact, like a phone's banner (picture, name, message); taller when the message needs two lines.
const CHAT_NOTICE: (f32, f32, f32) = (400.0, 86.0, 24.0);
/// A chat card fits its words: width between these, and up to this many lines of message.
const CHAT_WIDTH: (f32, f32) = (320.0, 430.0);
const CHAT_LINES: u32 = 2;
/// A chat card's room: padding all round, the picture, where the words start (padding + picture + gap 14), the name's
/// top, the message's top (the name row, then a 6 px breath), a message line when several are stacked.
const CHAT_PAD: f32 = 20.0;
const CHAT_PIC: f32 = 46.0;
const CHAT_TEXT_X: f32 = CHAT_PAD + CHAT_PIC + 14.0;
const CHAT_NAME_Y: f32 = 19.0;
const CHAT_MSG_Y: f32 = 45.0;
const CHAT_ROW: f32 = 26.0;
/// What a chat card says: the name in white, for a group the group's name in grey after it, and the message lines.
/// A group's lines arrive as "Sam: on my way" (Windows' own group notices, and Sora's WhatsApp and Telegram): the
/// newest line's sender is the name, and their own lines lose the "Sam: " in front.
fn chat_words(title: &str, detail: &str) -> (String, Option<String>, Vec<String>) {
    let parts: Vec<&str> = detail.split(MSG_SEP).collect();
    // The name comes from the newest line someone else wrote (one's own replies are kept as they are).
    match parts.iter().rev().find(|p| !p.starts_with(MINE)).and_then(|p| group_line(p)) {
        Some((who, _)) => (who.to_string(), Some(title.to_string()), parts.iter().map(|p| match group_line(p) { Some((w, m)) if w == who && !p.starts_with(MINE) => m.to_string(), _ => p.to_string() }).collect()),
        None => (title.to_string(), None, parts.iter().map(|p| p.to_string()).collect()),
    }
}
/// Lines one card holds, one each, in the order they came: the sender's last CHAT_MERGE messages and, apart from them,
/// the user's last CHAT_MERGE replies from the card. A fourth of either kind pushes out the oldest of that kind (sliding
/// out at the top when it was the top line); replying never pushes out the sender's messages.
const CHAT_MERGE: usize = 3;
/// A card's lines after one more came in: at most CHAT_MERGE of the sender's and CHAT_MERGE of one's own, oldest out
/// first; and the line that left, when it was the top one (it slides out; any other just goes).
fn keep_recent(mut lines: Vec<String>) -> (Vec<String>, String) {
    let mut dropped = String::new();
    for mine in [false, true] {
        while lines.iter().filter(|l| l.starts_with(MINE) == mine).count() > CHAT_MERGE {
            let Some(i) = lines.iter().position(|l| l.starts_with(MINE) == mine) else { break };
            let gone = lines.remove(i);
            if i == 0 { dropped = gone; }
        }
    }
    (lines, dropped)
}
/// Between the messages joined on one card (the unit separator: never part of a message).
const MSG_SEP: char = '\u{1f}';
/// In front of a line the user sent from the card (never part of a message): drawn right-aligned, as one's own.
const MINE: char = '\u{2}';
const NOTICE_GAP: f32 = 10.0;
const NOTICE_HOLD: u32 = 8000;
/// A notice with others waiting behind it stays shorter.
const QUEUED_HOLD: u32 = 4500;
const CHAT: usize = 2;
/// Name and plain line icon (24 x 24, as in Settings) per kind: timer, calendar, chat, water, alarm, agent, recorder,
/// receiving (LocalSend).
/// The reminder icons are the Mac's (ui/notice.html, drawn on a 32 grid), scaled to this 24 grid.
const NOTICE_KINDS: [(&str, &str); 8] = [
    ("FOCUS TIMER", "M6 3h12M6 21h12M7.5 3.75v3c0 3 2.25 3.75 4.5 5.25-2.25 1.5-4.5 2.25-4.5 5.25v3m9-16.5v3c0 3-2.25 3.75-4.5 5.25 2.25 1.5 4.5 2.25 4.5 5.25v3M9 6.75h6l-3 3zm0 12 3-3 3 3z"),
    ("CALENDAR", "M6.75 5.25h10.5a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3H6.75a3 3 0 0 1-3-3V8.25a3 3 0 0 1 3-3zM3.75 9.75h16.5M8.25 3v4.5M15.75 3v4.5m-6.75 7.5 2.25 2.25 3.75-4.5"),
    ("CHAT", "M5 5h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-7l-5 4v-4H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Z"),
    ("WATER", "M12 3c3.75 4.5 6 7.5 6 11.25a6 6 0 0 1-12 0c0-3.75 2.25-6.75 6-11.25zM9 15a3 3 0 0 0 3 3"),
    ("ALARM", "M18.75 12.75a6.75 6.75 0 1 1-13.5 0a6.75 6.75 0 1 1 13.5 0zM12 9v3.75l2.25 1.5M3.75 6l3-2.25m13.5 2.25-3-2.25"),
    ("AGENT", "M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8ZM18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8Z"),
    ("RECORDER", "M12 3a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3ZM5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"),
    ("RECEIVE", "M9 2.5h6a2.5 2.5 0 0 1 2.5 2.5v14a2.5 2.5 0 0 1-2.5 2.5H9A2.5 2.5 0 0 1 6.5 19V5A2.5 2.5 0 0 1 9 2.5ZM12 7.5v7m-3-3 3 3 3-3M10.5 18h3"),
];
/// The LocalSend notice (kind 7): "ls-ask:<id>" has Accept / Deny under its words, "ls-busy" a bar that follows the
/// transfer (lan_receive::status), "ls-done" is a plain reminder.
const RECEIVE: usize = 7;
fn receive_rows(n: &Notice) -> bool { n.kind == RECEIVE && (n.target.starts_with("ls-ask:") || n.target == "ls-busy") }
/// A contact and a group, for a messenger notice without a picture.
const PERSON: &str = "M12 11.5a3.8 3.8 0 1 0 0-7.6 3.8 3.8 0 0 0 0 7.6ZM4.5 20.3c.8-3.5 3.9-5.3 7.5-5.3s6.7 1.8 7.5 5.3";
/// A check, for a finished timer.
const CHECK: &str = "M7 12.5l3.2 3.2L17 8.8";
/// The reply field's send button: an arrow up.
const SEND_ARROW: &str = "M12 18V6M6.8 11.2 12 6l5.2 5.2";
const GROUP: &str = "M9.2 11a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4ZM3 19.6c.6-3 3-4.7 6.2-4.7s5.6 1.7 6.2 4.7M16 4.9a3 3 0 0 1 0 5.9M18 14.9c1.7.5 2.8 2 3.2 4.7";
/// "09:30" inside a reminder's detail, and the detail without it ("Starts at 09:30" becomes `fallback`, "09:30 · Bring the report" becomes "Bring the report").
fn split_clock(detail: &str, fallback: &str) -> (String, Option<String>) {
    let b = detail.as_bytes();
    let at = (0..b.len().saturating_sub(4)).find(|&i| b[i].is_ascii_digit() && b[i + 1].is_ascii_digit() && b[i + 2] == b':' && b[i + 3].is_ascii_digit() && b[i + 4].is_ascii_digit());
    let Some(i) = at else { return (detail.to_string(), None) };
    let rest = format!("{}{}", &detail[..i], &detail[i + 5..]).replace("Starts at", "").replace("It's", "");
    let rest = rest.trim().trim_start_matches('·').trim().to_string();
    (if rest.is_empty() { fallback.to_string() } else { rest }, Some(detail[i..i + 5].to_string()))
}
/// A group chat's notice puts the sender in front of the message ("Sam: on my way").
fn looks_like_group(detail: &str) -> bool { group_line(detail).is_some() }
/// "Sam: on my way" as (sender, words).
fn group_line(line: &str) -> Option<(&str, &str)> { line.split_once(": ").filter(|(who, _)| !who.trim().is_empty() && who.chars().count() <= 28) }
/// `label` replaces the kind's name (a chat app's name); `target`: what a click opens; `picture`: the sender's or group's.
/// A reply typed right in a chat card (WhatsApp, Telegram): sent by whatsapp.rs / telegram.rs, as from Chat history.
struct ReplyBox { key: String, name: String, text: Vec<char>, caret: usize, high: Option<u16>, sending: bool, sent_at: Option<Instant>, outgoing: String, error: String, typed_at: Option<Instant>, blink: Instant, opened: Instant }
/// The answer of a reply sent from a card, picked up on the notch's thread (WM_REPLY_DONE).
static REPLY_DONE: Mutex<Option<Result<(), String>>> = Mutex::new(None);
/// A card's reply field: its height, the gap above it, the send button beside it (a disc as tall as the field), and how
/// long the field takes to come in.
const REPLY_FIELD: f32 = 34.0;
const REPLY_GAP: f32 = 16.0;
const REPLY_SEND_GAP: f32 = 8.0;
const REPLY_IN: f32 = 0.34;
/// A card being answered folds once its field has been empty and untouched this long (ms): after a send, or never typed in.
const REPLY_IDLE: u32 = 15_000;
struct Notice { kind: usize, label: String, title: String, detail: String, target: String, picture: Option<crate::draw::Picture>, at: Instant, hold_ms: u32, size: (f32, f32, f32), born: Instant, merged: Instant, from: (f32, f32, f32), refill: f32, done: std::sync::mpsc::Sender<()>,
    /// It took the place of the one on show (no fold in between): its words fade in while the card glides to its size.
    swapped: bool,
    /// The line a merge pushed out at the top (it slides up and fades while the others move up); empty when none did.
    dropped: String }
/// Notices waiting to be taken up by the notch (more than one can arrive between two of its messages).
static NOTICE_IN: Mutex<Vec<Notice>> = Mutex::new(Vec::new());
/// The region last given to the panel page (see clip_page).
static CLIP: Mutex<Option<[i32; 5]>> = Mutex::new(None);
/// Shows a reminder in the notch. The receiver hears when it has gone; it is dropped at once (disconnected) when the
/// notch cannot show it (web notch, or hidden away), and the caller then uses the separate reminder window.
/// The LocalSend transfer moved on: the notch draws its bar again.
pub fn receive_progress() {
    ANIMATING.store(true, Ordering::SeqCst);
    if let Some(h) = hwnd() { unsafe { let _ = PostMessageW(h, WM_NOTICE, WPARAM(0), LPARAM(0)); } }
}
pub fn notify(kind: &str, label: &str, title: &str, detail: &str, target: &str, picture: Option<crate::draw::Picture>) -> Option<std::sync::mpsc::Receiver<()>> {
    let h = hwnd().filter(|_| enabled())?;
    let (done, rx) = std::sync::mpsc::channel();
    let kind = match kind { "event" => 1, "chat" => CHAT, "water" => 3, "alarm" => 4, "agent" => 5, "meeting" => 6, "receive" => RECEIVE, _ => 0 };
    NOTICE_IN.lock().ok()?.push(Notice { kind, label: label.to_string(), title: title.into(), detail: detail.into(), target: target.into(), picture, at: Instant::now(), hold_ms: NOTICE_HOLD, size: CHAT_NOTICE, born: Instant::now(), merged: Instant::now(), from: CHAT_NOTICE, refill: 1.0, done, dropped: String::new(), swapped: false });
    unsafe { let _ = PostMessageW(h, WM_NOTICE, WPARAM(0), LPARAM(0)); }
    Some(rx)
}
const MID: f32 = CW / 2.0;
const COMPACT: (f32, f32, f32) = (292.0, 64.0, 20.0);
const PANEL: (f32, f32, f32) = (720.0, 234.0, 30.0);
/// The edge handle: a floating capsule 8 px below the top of the screen.
const CAPSULE: (f32, f32, f32) = (64.0, 10.0, 8.0);
/// How much the capsule swells while the pointer rests on it (a small Dynamic Island peek).
const PEEK: (f32, f32) = (18.0, 14.0);
const EAR: f32 = 12.0;
const BLACK: u32 = 0x050506;
/// Sora's one accent colour (with black and white).
/// Which part of the screen's top edge the notch hangs from.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Align { Left, Center, Right }
impl Align { fn parse(s: &str) -> Self { match s { "left" => Self::Left, "right" => Self::Right, _ => Self::Center } } }
/// Gap (dip) between the screen's side and a left or right notch.
const SIDE_GAP: f32 = 20.0;
/// The notch scale for the Size slider (0 smallest, 100 largest); a narrow screen can still shrink it further.
fn size_scale(size: u32) -> f32 { 0.64 + (0.85 - 0.64) * size.min(100) as f32 / 100.0 }
/// Centre of the mini notch inside the canvas: the middle, or tucked against the left or right end.
fn anchor(align: Align) -> f32 {
    match align { Align::Left => EAR + COMPACT.0 / 2.0, Align::Center => MID, Align::Right => CW - EAR - COMPACT.0 / 2.0 }
}
/// Centre of a notch `w` wide: its left (or right) edge stays put as it grows, until the panel fills the canvas.
unsafe extern "system" fn collect_child(h: HWND, l: LPARAM) -> BOOL { (*(l.0 as *mut Vec<HWND>)).push(h); BOOL(1) }
static SLEEP_PENDING: AtomicBool = AtomicBool::new(false);
/// The resting floating capsule on screen (x, y, w, h physical px), so a reminder can grow out of it.
static CAPSULE_RECT: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);
pub fn capsule_rect() -> Option<(i32, i32, i32, i32)> { CAPSULE_RECT.lock().ok().and_then(|r| *r) }
/// The notch as it is on screen now, whatever its state (capsule, mini notch, panel): centre x and bottom y, physical px.
static NOTCH_GEOM: Mutex<Option<(i32, i32)>> = Mutex::new(None);
pub fn notch_geometry() -> Option<(i32, i32)> { NOTCH_GEOM.lock().ok().and_then(|g| *g) }
fn center(align: Align, w: f32) -> f32 { anchor(align).clamp(EAR + w / 2.0, (CW - EAR - w / 2.0).max(EAR + w / 2.0)) }
/// Notch scale, canvas origin and canvas size (px) on a monitor `m` with `dpi`.
fn layout(m: RECT, dpi: f32, align: Align, scale: f32) -> (f32, (i32, i32), (i32, i32)) {
    let width_dip = (m.right - m.left) as f32 / dpi;
    let fit = scale.min((width_dip - 24.0) / CW);
    let k = fit * dpi;
    let size = ((CW * k).ceil() as i32, (CH * k).ceil() as i32);
    let gap = (SIDE_GAP * dpi) as i32;
    let x = match align { Align::Left => m.left + gap, Align::Center => m.left + (m.right - m.left - size.0) / 2, Align::Right => m.right - size.0 - gap };
    (fit, (x.clamp(m.left, (m.right - size.0).max(m.left)), m.top), size)
}
/// Monitors as (id, label, bounds). The id is the device name; the primary one is also known as "primary".
pub fn monitors() -> Vec<(String, String, bool)> {
    unsafe extern "system" fn each(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<(String, String, bool)>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
            let name = String::from_utf16_lossy(&info.szDevice).trim_end_matches('\0').to_string();
            let r = info.monitorInfo.rcMonitor;
            let primary = info.monitorInfo.dwFlags & 1 != 0;
            let number: String = name.chars().filter(char::is_ascii_digit).collect();
            list.push((name, format!("Display {number} · {} × {}", r.right - r.left, r.bottom - r.top), primary));
        }
        TRUE
    }
    let mut list: Vec<(String, String, bool)> = Vec::new();
    unsafe { let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize)); }
    list.sort_by_key(|m| !m.2);
    list
}
/// The chosen monitor's bounds and DPI, or the primary monitor's when it is "primary" or has been unplugged.
unsafe fn monitor_of(id: &str) -> (RECT, f32) {
    let mut found = HMONITOR::default();
    if id != "primary" {
        unsafe extern "system" fn each(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let (want, out) = &mut *(data.0 as *mut (String, HMONITOR));
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(h, &mut info as *mut _ as *mut MONITORINFO).as_bool() && String::from_utf16_lossy(&info.szDevice).trim_end_matches('\0') == want { *out = h; }
            TRUE
        }
        let mut state = (id.to_string(), HMONITOR::default());
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut state as *mut _ as isize));
        found = state.1;
    }
    if found.is_invalid() { found = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY); }
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(found, &mut info);
    let (mut dx, mut dy) = (96u32, 96u32);
    let _ = GetDpiForMonitor(found, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
    (info.rcMonitor, dx as f32 / 96.0)
}
/// How long the pointer rests on the capsule before the mini notch comes back.

const WM_PEEK: u32 = WM_APP + 11;
const WM_VISIBLE: u32 = WM_APP + 12;
const WM_HIDDEN: u32 = WM_APP + 13;
const WM_TAB: u32 = WM_APP + 14;
const WM_COLLAPSE: u32 = WM_APP + 15;
const WM_DRAG: u32 = WM_APP + 16;
const WM_RELAYOUT: u32 = WM_APP + 17;
const WM_RESIZE: u32 = WM_APP + 18;
const WM_SEND: u32 = WM_APP + 19;
/// Opens the panel on the Agents tab (a shortcut or a notice's button).
const WM_AGENTS: u32 = WM_APP + 31;
const WM_NOTICE: u32 = WM_APP + 20;
const WM_REPLY_DONE: u32 = WM_APP + 33;
/// The panel's extra height (notch px x 100, at most TALL), for a view that fits its contents (an agent's question).
const WM_TALL: u32 = WM_APP + 34;
const WM_REDRAW: u32 = WM_APP + 30;
/// Another app's window came to the front (wParam: its HWND).
const WM_FOREGROUND: u32 = WM_APP + 32;

const NO_PREVIEW: u32 = u32::MAX;
static PREVIEW: AtomicU32 = AtomicU32::new(NO_PREVIEW);
const WM_MOUSELEAVE: u32 = 0x02A3;
const T_OUTSIDE: usize = 1;
const T_EDGE: usize = 3;
const T_TIMER: usize = 4;
const T_PREWARM: usize = 5;
/// How long a page loaded on hover waits for the click before it is thrown away.
const PREWARM: u32 = 4000;
/// A page stays suspended this long after the panel closed, so the next open shows it at once; then it is dropped and a
/// fresh one is warmed in its place (T_WARM). Asleep it holds ~6 MB of RAM (WebView2 suspended, working set trimmed), and a
/// cold start costs ~0.7 s of CPU across six new processes right while the notch opens: that was the stutter.
const KEEP_WARM: u32 = 3 * 60 * 60 * 1000;
/// Builds the page in the background and puts it to sleep: after start (WARM_AFTER), and after a drop.
const T_WARM: usize = 9;
const WARM_AFTER: u32 = 6000;
/// Checks of a background-built page (every WARM_POLL) before giving up; the hover/click path still works without it.
const WARM_POLL: u32 = 500;
static WARM_TRIES: AtomicU32 = AtomicU32::new(0);
const T_DROP: usize = 6;
const T_TRIM: usize = 7;
const T_NOTICE: usize = 8;

static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
static ANIMATING: AtomicBool = AtomicBool::new(false);
static PLAYING: AtomicBool = AtomicBool::new(false);
/// The panel page has loaded (it may still be hidden), and it is on screen.
static PAGE_READY: AtomicBool = AtomicBool::new(false);
static PAGE_SHOWN: AtomicBool = AtomicBool::new(false);
/// Whether the notch still wants the page, and whether one is being built (a build that finishes unwanted is destroyed).
static WANT_PAGE: AtomicBool = AtomicBool::new(false);
static BUILDING: AtomicBool = AtomicBool::new(false);
/// A page window exists (tracked here: a destroyed window can still be found by label for a moment).
static PAGE_ALIVE: AtomicBool = AtomicBool::new(false);
/// The page has been opened at least once (a page only loaded on hover is dropped soon; an opened one is kept warm).
static PAGE_USED: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// The running focus timer's end (ms since epoch), set by the panel page; saved so a restart still chimes.
static DEADLINE: Mutex<Option<i64>> = Mutex::new(None);
static DEADLINE_FILE: OnceLock<PathBuf> = OnceLock::new();

/// The native notch is the default; `--web-notch` or SORA_NATIVE_NOTCH=0 brings back the all-web one.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !std::env::args().any(|a| a == "--web-notch") && std::env::var("SORA_NATIVE_NOTCH").as_deref() != Ok("0"))
}
pub fn hwnd() -> Option<HWND> { let raw = HWND_RAW.load(Ordering::SeqCst); (raw != 0).then(|| HWND(raw as _)) }
fn post(msg: u32, wp: usize) { if let Some(h) = hwnd() { unsafe { let _ = PostMessageW(h, msg, WPARAM(wp), LPARAM(0)); } } }
/// Shortcut, tray "Open", second launch, timer alert, the end of a capsule dwell: show the mini notch.
pub fn peek() { post(WM_PEEK, 0) }
/// The visibility policy (hide when maximized, Game Mode, the shortcut): into the capsule (or out of view) and back.
pub fn set_visible(visible: bool) { post(if visible { WM_VISIBLE } else { WM_HIDDEN }, 0) }
/// A file drag from Explorer started (opens the Tray) or ended.
pub fn drag_offer() { post(WM_DRAG, 1) }
pub fn drag_ended() { post(WM_DRAG, 0) }
/// Something needs an answer in Send (the phone offers files): the panel opens on the Send tab.
pub fn show_send() { post(WM_SEND, 0) }
pub fn show_agents() { post(WM_AGENTS, 0) }

/// Settings changed the notch's position, display or size.
pub fn relayout() { PREVIEW.store(NO_PREVIEW, Ordering::SeqCst); post(WM_RELAYOUT, 0) }
/// While the Size slider is dragged: the notch follows it (None: back to the saved size). Nothing is saved here.
pub fn preview_size(size: Option<u32>) { PREVIEW.store(size.map_or(NO_PREVIEW, |s| s.min(100)), Ordering::SeqCst); post(WM_RESIZE, 0) }

// ---- the panel page (commands from ui/app.js in panel mode)
/// The tab the panel page shows (as it last said through panel_view).
static CURRENT_TAB: Mutex<String> = Mutex::new(String::new());
/// Whether that tab is on screen now (the panel is open and shows it).
/// "agents-question" (the tall Agents panel) counts as "agents", "meeting-detail" as "meeting", "board-preview" as "board".
pub fn tab_visible(name: &str) -> bool { PAGE_SHOWN.load(Ordering::SeqCst) && CURRENT_TAB.lock().map_or(false, |t| t.as_str() == name || t.strip_prefix(name).is_some_and(|r| r.starts_with('-'))) }
/// The panel page has loaded. It is shown by the notch once the panel is (nearly) open: it may have been loaded on hover.
#[tauri::command]
pub fn panel_ready() { if !WANT_PAGE.load(Ordering::SeqCst) { return; } PAGE_READY.store(true, Ordering::SeqCst); if let Some(h) = hwnd() { unsafe { let _ = PostMessageW(h, WM_NULL, WPARAM(0), LPARAM(0)); } } with_app_animate(); }
fn with_app_animate() { ANIMATING.store(true, Ordering::SeqCst); }
/// Escape or a collapse inside the page.
#[tauri::command]
pub fn panel_close() { post(WM_COLLAPSE, 0) }
/// The page switched views: the glow under it follows.
/// A view that fits its contents (an agent's question): the panel grows by `px` notch px over its normal height (0..TALL).
#[tauri::command]
pub fn panel_tall(px: f64) { post(WM_TALL, (px.clamp(0.0, TALLEST as f64) * 100.0) as usize) }
/// A file dialog opens or closes: on Windows the notch stays as it is.
pub fn dialog(_open: bool) {}
#[tauri::command]
pub fn panel_view(name: String) { if let Ok(mut t) = CURRENT_TAB.lock() { *t = name.clone(); } post(WM_TAB, match name.as_str() { "music" => 1, "shelf" => 2, "send" => 3, "history" | "meeting-detail" => 4, "agents-question" | "board-preview" | "send-sheet" => 5, n if n.ends_with("-editor") => 4, _ => 0 }) }
/// The page started, changed or paused the focus timer (end in ms since epoch, None when paused or reset).
#[tauri::command]
pub fn timer_deadline(end: Option<i64>) { set_deadline(end) }
/// The meeting recorder runs: a red dot stays on the notch (open or closed) until it stops.
static RECORDING: AtomicBool = AtomicBool::new(false);
pub fn set_recording(on: bool) { RECORDING.store(on, Ordering::SeqCst); post(WM_REDRAW, 0) }
fn set_deadline(end: Option<i64>) {
    if let Ok(mut d) = DEADLINE.lock() { *d = end; }
    if let Some(f) = DEADLINE_FILE.get() { let _ = std::fs::write(f, serde_json::to_vec(&end).unwrap_or_default()); }
}
fn now_ms() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }

pub fn start(app: &tauri::AppHandle) {
    let _ = APP.set(app.clone());
    if let Ok(dir) = app.path().app_data_dir() {
        let file = dir.join("timer-deadline.json");
        if let Some(end) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Option<i64>>(&b).ok()).flatten() { if let Ok(mut d) = DEADLINE.lock() { *d = Some(end); } }
        let _ = DEADLINE_FILE.set(file);
    }
    std::thread::spawn(|| unsafe { run_window() });
    // The mascot dances while music plays: ask Windows' media session now and then.
    std::thread::spawn(|| loop {
        let playing = crate::media_read(false).ok().flatten().map_or(false, |m| m.playing);
        PLAYING.store(playing, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1500));
    });
}

/// Gives the sleeping page's memory back to Windows: empties the working set of Sora's own WebView2 processes
/// (the browser process started by sora.exe and everything it started). Pages come back from the page file on the next open.
fn trim_webview() {
    use windows::Win32::System::{Diagnostics::ToolHelp::*, ProcessStatus::K32EmptyWorkingSet, Threading::*};
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return };
        let mut all = Vec::new();
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok { all.push((e.th32ProcessID, e.th32ParentProcessID)); ok = Process32NextW(snap, &mut e).is_ok(); }
        let _ = CloseHandle(snap);
        let mut ours = vec![std::process::id()];
        let mut i = 0;
        while i < ours.len() { let p = ours[i]; ours.extend(all.iter().filter(|(_, parent)| *parent == p).map(|(id, _)| *id)); i += 1; }
        for pid in ours.into_iter().skip(1) {
            if let Ok(h) = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) { let _ = K32EmptyWorkingSet(h); let _ = CloseHandle(h); }
        }
    }
}

// ---------------------------------------------------------------- the chime (the web page's three soft notes, C5 E5 G5)
fn chime_wav() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| {
        let rate = 44_100u32;
        let len = (rate as f32 * 1.1) as usize;
        let mut samples = vec![0f32; len];
        for (i, hz) in [523.25f32, 659.25, 783.99].iter().enumerate() {
            let start = (i as f32 * 0.17 * rate as f32) as usize;
            for n in 0..(0.7 * rate as f32) as usize {
                let t = n as f32 / rate as f32;
                // 20 ms rise to 0.08, then an exponential fall to 0.001 at 0.65 s.
                let gain = if t < 0.02 { 0.08 * t / 0.02 } else { 0.08 * (0.001f32 / 0.08).powf(((t - 0.02) / 0.63).min(1.0)) };
                if let Some(s) = samples.get_mut(start + n) { *s += gain * (std::f32::consts::TAU * hz * t).sin(); }
            }
        }
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF"); wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes()); wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes()); wav.extend_from_slice(&(rate * 2).to_le_bytes()); wav.extend_from_slice(&2u16.to_le_bytes()); wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data"); wav.extend_from_slice(&(data.len() as u32).to_le_bytes()); wav.extend_from_slice(&data);
        wav
    })
}
fn chime() { unsafe { let _ = PlaySoundW(windows::core::PCWSTR(chime_wav().as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT); } }

// ---------------------------------------------------------------- the mascot (ui/mascot.js, same numbers)
#[derive(Clone, Copy, Default)]
struct Face { x: f32, y: f32, roll: f32, sx: f32, sy: f32, open: f32, smile: f32, hum: f32, asym: f32, wink: f32 }
impl Face {
    fn rest() -> Self { Self { sx: 1.0, sy: 1.0, open: 1.0, smile: 9.0, ..Default::default() } }
    fn get(&self, i: usize) -> f32 { [self.x, self.y, self.roll, self.sx, self.sy, self.open, self.smile, self.hum, self.asym, self.wink][i] }
    fn set(&mut self, i: usize, v: f32) { *[&mut self.x, &mut self.y, &mut self.roll, &mut self.sx, &mut self.sy, &mut self.open, &mut self.smile, &mut self.hum, &mut self.asym, &mut self.wink][i] = v; }
}
fn pulse(t: f32, center: f32, width: f32) -> f32 { (-((t - center) / width).powi(2)).exp() }
fn smooth(v: f32) -> f32 { let v = v.clamp(0.0, 1.0); v * v * (3.0 - 2.0 * v) }
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
struct Note { born: f32, side: f32 }
/// How long the start-up greeting's wink lasts.
const WINK: f32 = 3.2;
struct Mascot {
    face: Face, vel: Face, clock: f32, blink_at: f32, blink_start: f32, hover: bool, px: f32, py: f32, awake_until: Option<Instant>,
    notes: Vec<Note>, last_note: f32, note_alpha: f32,
    /// When the wink started (clock), until when nothing else may move (the greeting), when the pop-in started, and hidden before it.
    wink_at: Option<f32>, calm_until: f32, pop_at: Option<f32>, hidden: bool,
}
impl Mascot {
    fn new() -> Self { Self { face: Face::rest(), vel: Face::default(), clock: 0.0, blink_at: 2.8, blink_start: -10.0, hover: false, px: 0.0, py: 0.0, awake_until: None, notes: Vec::new(), last_note: -10.0, note_alpha: 0.0, wink_at: None, calm_until: 0.0, pop_at: None, hidden: true } }
    fn wink(&mut self) { self.wink_at = Some(self.clock); }
    /// Only the slow idle motion is playing (no pointer, music, wink or pop-in).
    fn idle(&self, music: bool) -> bool {
        !self.hidden && !self.hover && !music && self.awake_until.map_or(true, |t| Instant::now() >= t)
            && self.wink_at.map_or(true, |s| self.clock - s >= WINK) && self.pop_at.map_or(true, |s| self.clock - s >= 0.6) && self.clock >= self.calm_until
    }
    /// Start-up: the mascot pops in and winks; nothing else moves until the greeting is over.
    fn greet(&mut self) { self.hidden = false; self.pop_at = Some(self.clock); self.calm_until = self.clock + 3.3; self.wink(); }
    /// Pop-in (scale, y offset, opacity): CSS mascot-pop, .6 s cubic-bezier(.34,1.56,.5,1), opaque at 55%.
    fn pop(&self) -> (f32, f32, f32) {
        if self.hidden { return (0.5, 10.0, 0.0); }
        let Some(at) = self.pop_at else { return (1.0, 0.0, 1.0) };
        let t = ((self.clock - at) / 0.6).clamp(0.0, 1.0);
        let e = bezier(0.34, 1.56, 0.5, 1.0, t);
        (0.5 + 0.5 * e, 10.0 * (1.0 - e), (t / 0.55).min(1.0))
    }
    /// One step of the rig; returns the eyelid opening (1 open, 0 closed by a blink).
    fn step(&mut self, dt: f32, music: bool) -> f32 {
        let dt = dt.min(0.034);
        self.clock += dt;
        let celebration = self.wink_at.map(|s| self.clock - s).filter(|c| *c < WINK);
        let notifying = celebration.is_some();
        let calm = notifying || self.clock < self.calm_until;
        let attentive = !calm && (self.hover || self.awake_until.map_or(false, |t| Instant::now() < t));
        let listening = music && !attentive && !calm;
        let t = self.clock % 14.0;
        self.notes_step(dt, listening);
        if self.clock > self.blink_at { self.blink_start = self.clock; self.blink_at = self.clock + 3.2 + rand() * 3.8; }
        let b = self.clock - self.blink_start;
        let blink = if b < 0.0 || b > 0.26 { 0.0 } else if b < 0.08 { (b / 0.08 * std::f32::consts::FRAC_PI_2).sin() } else if b < 0.115 { 1.0 } else { ((b - 0.115) / 0.145 * std::f32::consts::FRAC_PI_2).cos() };
        let c = self.clock;
        let mut target = Face { y: if calm { 0.0 } else { (c * 1.1).sin() * 0.7 }, smile: 8.0, ..Face::rest() };
        if listening {
            // Anticipation, two accented nods, a relaxed sway, humming.
            let nod = pulse(t, 1.2, 0.22) + 0.8 * pulse(t, 1.85, 0.19) + pulse(t, 8.1, 0.23) + 0.65 * pulse(t, 8.7, 0.18);
            let anticipate = pulse(t, 0.85, 0.22) + pulse(t, 7.75, 0.22);
            let sway = (c * 2.25).sin() * (std::f32::consts::PI * t / 14.0).sin().powi(2);
            let hum = pulse(t, 5.1, 0.8);
            target = Face { x: sway * 4.5, y: nod * 4.3 - anticipate * 2.4 + (c * 4.5).sin() * 0.7, roll: sway * 7.0 + (c * 4.5).sin() * 1.5, sx: 1.0 + nod * 0.07 - anticipate * 0.025, sy: 1.0 - nod * 0.08 + anticipate * 0.06, open: 0.0, smile: 10.0 + nod * 3.0 - hum * 7.0, hum, asym: 0.0, wink: 0.0 };
        } else if attentive {
            target.x = self.px * 7.0; target.y = self.py * 3.0 - 1.0; target.roll = self.px * 3.0; target.open = 1.05; target.smile = 11.0; target.sy = 1.025; target.asym = self.px * 0.05;
        } else if !calm {
            let glance = (c * 0.46).sin();
            target.x = glance * 2.0; target.roll = glance * 0.7; target.asym = pulse(t, 9.0, 0.6) * 0.12;
        }
        if let Some(cel) = celebration {
            // A small bounce, a smile and a one-eye wink that eases in and out.
            let blend = smooth(cel / 0.22) * smooth((WINK - cel) / 0.55);
            let (bounce, settle) = (pulse(cel, 0.35, 0.18), pulse(cel, 0.85, 0.3));
            let greeting = Face { x: 2.0 + self.px * 2.0, y: -1.5 - 2.0 * bounce + settle, roll: -4.0 - 3.0 * bounce + self.px, sx: 1.0 + 0.025 * bounce, sy: 1.0 - 0.035 * bounce, open: 1.0, smile: 11.0 + 2.0 * bounce, hum: 0.0, asym: 0.0, wink: 0.0 };
            for i in 0..9 { target.set(i, lerp(target.get(i), greeting.get(i), blend)); }
            target.wink = smooth((cel - 0.12) / 0.16) * smooth((2.25 - cel) / 0.3);
        }
        for i in 0..10 {
            let (k, d) = match i { 9 => (420.0, 36.0), 5 => (260.0, 32.0), _ => (170.0, 23.0) };
            let v = self.vel.get(i) + (k * (target.get(i) - self.face.get(i)) - d * self.vel.get(i)) * dt;
            self.vel.set(i, v);
            self.face.set(i, self.face.get(i) + v * dt);
        }
        if listening || notifying { 1.0 } else { 1.0 - blink }
    }
    fn notes_step(&mut self, dt: f32, show: bool) {
        self.note_alpha += ((show as u8 as f32) - self.note_alpha) * (1.0 - (-dt * 10.0).exp());
        if !show { self.last_note = self.clock - 3.4; if self.note_alpha < 0.02 { self.notes.clear(); } }
        if show && self.clock - self.last_note >= 3.4 { self.last_note = self.clock; self.notes.push(Note { born: self.clock, side: if rand() > 0.5 { 1.0 } else { -1.0 } }); }
        let clock = self.clock;
        self.notes.retain(|n| clock - n.born < 4.6);
        while self.notes.len() > 2 { self.notes.remove(0); }
    }
}
/// The 7 points of one eye: a smile curve that closes into a dot and opens upward (two cubic segments);
/// `wink` morphs it into a ">" chevron.
fn eye_points(cx: f32, cy: f32, open: f32, tilt: f32, wink: f32) -> [(f32, f32); 7] {
    let amount = open.clamp(0.0, 1.0);
    let curve = 1.0 - smooth((amount / 0.4).min(1.0));
    let rise = smooth(((amount - 0.4) / 0.6).max(0.0));
    let (x, y, lean) = (10.0 * curve, 10.0 * rise, tilt * curve);
    let p = [(cx - x, cy - y + lean), (cx - x * 0.5, cy + 8.0 * curve), (cx + x * 0.5, cy + 8.0 * curve), (cx + x, cy + y - lean)];
    let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (a, b, c) = (mid(p[0], p[1]), mid(p[1], p[2]), mid(p[2], p[3]));
    let (d, e) = (mid(a, b), mid(b, c));
    let normal = [p[0], a, d, mid(d, e), e, c, p[3]];
    let chevron = [(cx - 9.0, cy - 12.5), (cx - 3.0, cy - 8.33), (cx + 3.0, cy - 4.17), (cx + 9.0, cy), (cx + 3.0, cy + 4.17), (cx - 3.0, cy + 8.33), (cx - 9.0, cy + 12.5)];
    let w = wink.clamp(0.0, 1.0);
    let mut out = normal;
    for i in 0..7 { out[i] = (lerp(normal[i].0, chevron[i].0, w), lerp(normal[i].1, chevron[i].1, w)); }
    out
}

// ---------------------------------------------------------------- window state
#[derive(Clone, Copy, PartialEq)]
enum Tab { Home, Music, Shelf, Send }
struct Ui {
    hwnd: HWND, app: tauri::AppHandle, d2d: ID2D1Factory, rt: ID2D1DCRenderTarget, stroke: ID2D1StrokeStyle,
    mem: HDC, dib: HBITMAP, old: HGDIOBJ, origin: (i32, i32), dpi: f32, fit: f32, size: (i32, i32), align: Align,
    /// Panel opening (0 mini notch, 1 panel), being in view (0 above the screen, 1 shown), shrunk into the edge capsule (1).
    open: Spring, shown: Spring, edge: Spring, want_shown: bool,
    /// A Home menu is open: the panel grows down by TALL (0 normal, 1 tall; more for a view that sets its own height).
    tall: Spring,
    /// The page window's height now, in pixels (see fit_page_height).
    page_h: Cell<u32>,
    mascot: Mascot, tab: Tab, tracking: bool, last: Option<Instant>, lid: f32,
    bbox: (i32, i32, i32, i32), buttons_down: bool, peek: Spring, dwell: Option<(Instant, f32)>, greet_pending: bool, greet_at: Option<Instant>, drawn: Option<Instant>,
    note: Option<ID2D1PathGeometry>,
    /// The reminder being shown (if any), its spring (0 gone, 1 full size) and what it is drawn with.
    notice: Option<Notice>, nspring: Spring, rspring: Spring, dw: IDWriteFactory, fonts: Option<IDWriteFontCollection>, family: Vec<u16>,
    notice_icons: Vec<Option<ID2D1PathGeometry>>, app_logos: Vec<Option<ID2D1PathGeometry>>, glyphs: [Option<ID2D1PathGeometry>; 4], notice_picture: Option<ID2D1Bitmap>,
    /// Notices behind the one on show; they show as cards peeking out under it.
    queue: Vec<Notice>,
    /// The other app's window that was in front when the panel opened (where dragged files come from).
    before: isize,
    /// The water reminder's +1 button: pointer on it, pressed in (0 up, 1 down), and when a glass was counted.
    water_hot: bool, press: Spring, drank_at: Option<Instant>,
    /// The pointer is on a chat card (it stays while hovered; one Sora can answer shows Reply), and a reply being typed in it.
    notice_hot: bool, reply: Option<ReplyBox>,
    /// Where the pointer last was over the notch (notch px), for its cursor (I-beam on the reply field, hand on send).
    mouse: (f32, f32),
}
thread_local! { static UI: RefCell<Option<Ui>> = RefCell::new(None); }
fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> { UI.with(|u| u.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f))) }

const NOTE: &str = "M5 14V0L14-2V11M5 3L14 1M5 14c0 4-7 4-7 0s7-4 7 0M14 11c0 4-7 4-7 0s7-4 7 0";

unsafe fn run_window() {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let Some(app) = APP.get().cloned() else { return };
    let instance = GetModuleHandleW(None).unwrap_or_default();
    let class = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance.into(), lpszClassName: w!("SoraNotch"), hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
    RegisterClassW(&class);
    let Ok(hwnd) = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW, w!("SoraNotch"), w!("Sora"), WS_POPUP, 0, 0, 10, 10, None, None, instance, None) else { return };
    let Ok(ui) = Ui::new(hwnd, app) else { let _ = DestroyWindow(hwnd); return };
    UI.with(|u| *u.borrow_mut() = Some(ui));
    HWND_RAW.store(hwnd.0 as isize, Ordering::SeqCst);
    // The panel page is built and put to sleep shortly after start, so the first open is not a WebView2 cold start.
    let _ = SetTimer(hwnd, T_WARM, WARM_AFTER, None);
    // Lives as long as this thread (the notch's lifetime); Sora's own windows are left out.
    windows::Win32::UI::Accessibility::SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
    with_ui(|ui| {
        ui.place();
        if crate::desktop::edge_enabled(&ui.app) { ui.shown = Spring::new(1.0); ui.edge = Spring::new(1.0); }
        // With the edge handle on, Sora starts as the capsule and stays one until the first click.
        if crate::desktop::edge_enabled(&ui.app) { crate::desktop::mark_hidden(&ui.app); ui.hide(); } else { ui.reveal(); }
        let _ = SetTimer(ui.hwnd, T_TIMER, 250, None); });
    let mut msg = MSG::default();
    loop {
        if ANIMATING.load(Ordering::SeqCst) {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() { if msg.message == WM_QUIT { return; } let _ = TranslateMessage(&msg); DispatchMessageW(&msg); }
            if !with_ui(|ui| ui.frame()).unwrap_or(false) { ANIMATING.store(false, Ordering::SeqCst); }
            if DwmFlush().is_err() { std::thread::sleep(Duration::from_millis(8)); }
        } else {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() { return; }
            let _ = TranslateMessage(&msg); DispatchMessageW(&msg);
        }
    }
}
unsafe extern "system" fn on_foreground(_: windows::Win32::UI::Accessibility::HWINEVENTHOOK, _: u32, fg: HWND, _: i32, _: i32, _: u32, _: u32) { post(WM_FOREGROUND, fg.0 as usize) }
/// The same top-level window, or both the desktop (it shows as Progman or WorkerW).
unsafe fn same_place(a: HWND, b: HWND) -> bool {
    if b.0.is_null() { return false; }
    let (a, b) = (GetAncestor(a, GA_ROOT), GetAncestor(b, GA_ROOT));
    let desktop = |h: HWND| { let mut c = [0u16; 32]; let n = GetClassNameW(h, &mut c).max(0) as usize; matches!(String::from_utf16_lossy(&c[..n]).as_str(), "Progman" | "WorkerW") };
    a == b || desktop(a) && desktop(b)
}
/// The notch's measures (see Ui::shape) from its springs: open (mini notch to panel), tall (a Home menu), edge (1 = the capsule)
/// and peek (a pointer resting on the capsule). An opening panel always hangs from the top edge with its wings, also straight
/// out of the capsule: it grows with the faster opening spring instead of waiting for the capsule's own growth to land.
fn notch_shape(open: f32, tall: f32, edge: f32, peek: f32) -> (f32, f32, f32, f32, f32, f32) {
    let (w, h, r) = (lerp(COMPACT.0, PANEL.0, open), lerp(COMPACT.1, PANEL.1 + TALL * tall, open), lerp(COMPACT.2, PANEL.2, open));
    let progress = 1.0 - edge.clamp(0.0, 1.0);
    let attach = smooth(open / 0.3);
    let grow = smooth(progress).max(open.clamp(0.0, 1.0));
    let swell = peek.clamp(0.0, 1.0) * (1.0 - grow);
    let (w, h) = (lerp(CAPSULE.0, w, grow) + PEEK.0 * swell, lerp(CAPSULE.1, h, grow) + PEEK.1 * swell);
    let contact = smooth((progress - 0.32) / 0.38).max(attach);
    let seam = smooth((progress - 0.70) / 0.24).max(attach);
    let ear = EAR * smooth((progress - 0.94) / 0.06).max(attach);
    // A bottom corner never rounds past half the height (early in a growth out of the capsule the panel's radius would).
    (w, h, lerp(h / 2.0, r, seam).min(h / 2.0), CAPSULE.2 * (1.0 - contact), ear, h / 2.0 * (1.0 - seam))
}
fn lo(l: LPARAM) -> f32 { (l.0 & 0xffff) as i16 as f32 }
fn hi(l: LPARAM) -> f32 { ((l.0 >> 16) & 0xffff) as i16 as f32 }
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let handled = match msg {
        WM_PEEK | WM_VISIBLE => with_ui(|ui| ui.reveal()).map(|_| 0),
        WM_HIDDEN => with_ui(|ui| ui.hide()).map(|_| 0),
        // 4: one of Home's menus, shown in a taller panel.
        WM_REDRAW => with_ui(|ui| { ui.drawn = None; ui.animate(); }).map(|_| 0),
        WM_TAB => with_ui(|ui| { ui.tab = [Tab::Home, Tab::Music, Tab::Shelf, Tab::Send, Tab::Home][wp.0.min(4)]; match wp.0 { 4 => ui.tall.set(1.0), 5 => {}, _ => ui.tall.set(0.0) }; ui.animate(); }).map(|_| 0),
        // 5 above: the view sets its own height (panel_tall).
        WM_TALL => with_ui(|ui| { ui.tall.set((wp.0 as f32 / 100.0 / TALL).clamp(0.0, TALLEST / TALL)); ui.animate(); }).map(|_| 0),
        WM_COLLAPSE => with_ui(|ui| ui.collapse()).map(|_| 0),
        WM_FOREGROUND => with_ui(|ui| ui.foreground_changed(HWND(wp.0 as _))).map(|_| 0),
        // A drag that ends without a drop leaves the tray open (an outside click or another app closes it), never a timer.
        WM_DRAG => with_ui(|ui| if wp.0 == 1 { ui.drag_offer() }).map(|_| 0),
        WM_TIMER => with_ui(|ui| ui.timer(wp.0)).map(|_| 0),
        WM_MOUSEMOVE => with_ui(|ui| { let (x, y) = ui.design(lo(lp), hi(lp)); ui.mouse_move(x, y); }).map(|_| 0),
        WM_MOUSELEAVE => with_ui(|ui| { ui.tracking = false; ui.water_hot = false; ui.press.set(0.0); ui.notice_hover(false); ui.leave(); ui.animate(); }).map(|_| 0),
        WM_LBUTTONDOWN => with_ui(|ui| { if ui.water_hot { ui.press.set(1.0); ui.animate(); } }).map(|_| 0),
        WM_LBUTTONUP => with_ui(|ui| { let (x, y) = ui.design(lo(lp), hi(lp)); ui.click(x, y); }).map(|_| 0),
        // A hand over the water +1 button: it is a button, not the card.
        WM_SETCURSOR => with_ui(|ui| ui.cursor()).flatten().map(|c| { SetCursor(LoadCursorW(None, c).unwrap_or_default()); 1 }),
        WM_KEYDOWN => with_ui(|ui| if !ui.reply_key(wp.0 as u16) && VIRTUAL_KEY(wp.0 as u16) == VK_ESCAPE { ui.collapse(); }).map(|_| 0),
        WM_CHAR => with_ui(|ui| ui.reply_char(wp.0 as u16)).filter(|used| *used).map(|_| 0),
        WM_REPLY_DONE => with_ui(|ui| ui.reply_done()).map(|_| 0),
        WM_DPICHANGED => with_ui(|ui| { ui.place(); ui.animate(); }).map(|_| 0),
        WM_SEND => with_ui(|ui| ui.show_send()).map(|_| 0),
        WM_AGENTS => with_ui(|ui| unsafe { crate::desktop::want_tab(&ui.app, "agents"); ui.reveal(); if ui.open.to < 0.5 { ui.expand(); } else { let _ = ui.app.emit_to("panel", "panel-tab", "agents"); } ui.animate(); }).map(|_| 0),

        WM_NOTICE => with_ui(|ui| ui.start_notice()).map(|_| 0),
        WM_RELAYOUT => with_ui(|ui| ui.relayout()).map(|_| 0),
        WM_RESIZE => with_ui(|ui| { ui.place(); ui.drawn = None; ui.animate(); }).map(|_| 0),
        WM_CLOSE => Some(0),
        // Windows is signing out or shutting down (Sora may not get to quit): chat apps get their popups back.
        WM_ENDSESSION => { if wp.0 != 0 { crate::chat_notices::restore_banners(); } Some(0) }
        _ => None,
    };
    match handled { Some(v) => LRESULT(v), None => DefWindowProcW(hwnd, msg, wp, lp) }
}

impl Ui {
    unsafe fn new(hwnd: HWND, app: tauri::AppHandle) -> windows::core::Result<Self> {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        // Software rendering: the notch is small, and a Direct3D device would cost tens of MB.
        let props = D2D1_RENDER_TARGET_PROPERTIES { r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE, pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 0.0, dpiY: 0.0, usage: D2D1_RENDER_TARGET_USAGE_NONE, minLevel: D2D1_FEATURE_LEVEL_DEFAULT };
        let rt = d2d.CreateDCRenderTarget(&props)?;
        let stroke = d2d.CreateStrokeStyle(&D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, dashCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, miterLimit: 10.0, dashStyle: D2D1_DASH_STYLE_SOLID, dashOffset: 0.0 }, None)?;
        let note = path_geometry(&d2d, &parse_path(NOTE));
        rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let fonts = crate::draw::poppins(&dw);
        let family = crate::draw::wide(if fonts.is_some() { "Poppins" } else { "Segoe UI" }).into_iter().chain([0]).collect();
        let notice_icons = NOTICE_KINDS.iter().map(|k| path_geometry(&d2d, &parse_path(k.1))).collect();
        let app_logos = MARKS.iter().map(|m| fill_geometry(&d2d, &parse_path(m.path))).collect();
        let glyphs = [path_geometry(&d2d, &parse_path(PERSON)), path_geometry(&d2d, &parse_path(GROUP)), path_geometry(&d2d, &parse_path(CHECK)), path_geometry(&d2d, &parse_path(SEND_ARROW))];
        Ok(Self {
            notice: None, nspring: Spring::new(0.0), rspring: Spring::new(1.0), dw, fonts, family, notice_icons, app_logos, glyphs, notice_picture: None, queue: Vec::new(), before: 0, water_hot: false, notice_hot: false, reply: None, mouse: (-1.0, -1.0), press: Spring::new(0.0), drank_at: None,
            hwnd, app, d2d, rt, stroke, mem: CreateCompatibleDC(None), dib: HBITMAP::default(), old: HGDIOBJ::default(), origin: (0, 0), dpi: 1.0, fit: 0.85, size: (0, 0), align: Align::Center,
            open: Spring::new(0.0), shown: Spring::new(0.0), edge: Spring::new(0.0), want_shown: true, tall: Spring::new(0.0), page_h: Cell::new(0),
            mascot: Mascot::new(), tab: Tab::Home, tracking: false, last: None, lid: 1.0, bbox: (0, 0, 0, 0), buttons_down: false, peek: Spring::new(0.0), dwell: None, greet_pending: true, greet_at: None, drawn: None, note,
        })
    }
    /// The top edge of the chosen display (Settings → Notch), at the chosen position and size.
    unsafe fn place(&mut self) {
        let (position, display, mut size) = crate::desktop::notch_layout(&self.app);
        if PREVIEW.load(Ordering::SeqCst) != NO_PREVIEW { size = PREVIEW.load(Ordering::SeqCst); }
        let (m, dpi) = monitor_of(&display);
        self.dpi = dpi;
        self.align = Align::parse(&position);
        (self.fit, self.origin, self.size) = layout(m, dpi, self.align, size_scale(size));
        if !self.dib.is_invalid() { SelectObject(self.mem, self.old); let _ = DeleteObject(self.dib); }
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: self.size.0, biHeight: -self.size.1, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(self.mem, &info, DIB_RGB_COLORS, &mut bits, None, 0) { self.dib = dib; self.old = SelectObject(self.mem, dib); }
        let _ = self.rt.BindDC(self.mem, &RECT { left: 0, top: 0, right: self.size.0, bottom: self.size.1 });
        self.rt.SetDpi(96.0 * self.dpi, 96.0 * self.dpi);
    }
    fn animate(&self) { ANIMATING.store(true, Ordering::SeqCst); }
    /// A new reminder: only while the notch is on screen (as the capsule, mini notch or panel); else the caller shows its own window.
    unsafe fn start_notice(&mut self) {
        let incoming: Vec<Notice> = NOTICE_IN.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
        // Dropped when the notch is not on screen: their callers then show the separate window.
        if !self.want_shown { return; }
        for n in incoming {
            if self.merge_notice(&n) { let _ = n.done.send(()); continue; }
            if n.kind == RECEIVE && n.target != "ls-busy" && self.notice.as_ref().is_some_and(|c| c.target == "ls-busy") {
                if let Some(c) = self.notice.as_mut() { c.hold_ms = c.at.elapsed().as_millis() as u32; }
                let _ = SetTimer(self.hwnd, T_NOTICE, 1, None);
                self.queue.insert(0, n);
                continue;
            }
            if self.notice.is_none() { self.show_notice(n); }
            else if self.queue.len() < 4 { self.queue.push(n); self.shorten_notice(); }
            else { let _ = n.done.send(()); }
        }
    }
    /// A second message from the sender on show joins that card (under the first, newest last, three at most) and the
    /// time starts over, instead of a new card.
    unsafe fn merge_notice(&mut self, n: &Notice) -> bool {
        let same = self.notice.as_ref().map_or(false, |c| c.kind == CHAT && n.kind == CHAT && c.label == n.label && c.title == n.title) && self.nspring.to > 0.5;
        if !same { return false; }
        // The size on show now, taken while the card is still in place (without it the size falls back to a reminder's).
        let on_show = self.notice_size();
        let Some(mut c) = self.notice.take() else { return false };
        let mut lines: Vec<String> = c.detail.split(MSG_SEP).map(str::to_string).collect();
        lines.push(n.detail.clone());
        let (keep, dropped) = keep_recent(lines);
        c.dropped = dropped;
        c.detail = keep.join(&MSG_SEP.to_string());
        // The card stays; only its words and ring move: the size glides to the new one, the new line comes in, the ring refills.
        let shown = (on_show, 1.0 - c.at.elapsed().as_secs_f32() / (c.hold_ms as f32 / 1000.0));
        c.from = shown.0;
        c.refill = shown.1.clamp(0.0, 1.0);
        c.merged = Instant::now();
        c.at = Instant::now();
        c.hold_ms = if self.queue.is_empty() { NOTICE_HOLD } else { QUEUED_HOLD };
        c.size = self.chat_size(&c);
        let hold = c.hold_ms;
        self.notice = Some(c);
        self.rspring = Spring::new(0.0);
        self.rspring.set(1.0);
        let _ = SetTimer(self.hwnd, T_NOTICE, hold, None);
        self.animate();
        true
    }
    /// Others are waiting: the one on show does not hold for the full time.
    unsafe fn shorten_notice(&mut self) {
        let Some(n) = self.notice.as_mut() else { return };
        // LocalSend's question and its transfer stay until answered or done; the others wait.
        if receive_rows(n) { return; }
        let age = n.at.elapsed().as_millis() as u32;
        if n.hold_ms > age + QUEUED_HOLD { n.hold_ms = age + QUEUED_HOLD; let _ = SetTimer(self.hwnd, T_NOTICE, QUEUED_HOLD, None); }
    }
    unsafe fn show_notice(&mut self, mut n: Notice) {
        // Taking the place of the one on show: the same card glides from that size to this one's, nothing on top of it.
        let on_show = self.notice.is_some().then(|| self.notice_size());
        if let Some(old) = self.notice.take() { let _ = old.done.send(()); }
        // The picture as a Direct2D bitmap of this render target (premultiplied BGRA from draw::decode).
        self.notice_picture = n.picture.take().and_then(|p| {
            let props = D2D1_BITMAP_PROPERTIES { pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 96.0, dpiY: 96.0 };
            self.rt.CreateBitmap(D2D_SIZE_U { width: p.width, height: p.height }, Some(p.bgra.as_ptr() as _), p.width * 4, &props).ok()
        });
        n.at = Instant::now();
        n.born = n.at;
        self.water_hot = false; self.press = Spring::new(0.0); self.drank_at = None;
        self.reply = None;
        n.size = match n.kind { CHAT => self.chat_size(&n), 5 if n.target.starts_with("agent-ask:") => (NOTICE.0 + 104.0, NOTICE.1, NOTICE.2),
            RECEIVE if n.target.starts_with("ls-ask:") => (NOTICE.0 + 60.0, NOTICE.1 + 40.0, NOTICE.2), RECEIVE if n.target == "ls-busy" => (NOTICE.0 + 60.0, NOTICE.1 + 10.0, NOTICE.2), _ => NOTICE };
        n.from = on_show.unwrap_or(n.size);
        n.swapped = on_show.is_some();
        self.rspring = Spring::new(if n.swapped { 0.0 } else { 1.0 });
        self.rspring.set(1.0);
        n.hold_ms = if self.queue.is_empty() { NOTICE_HOLD } else { QUEUED_HOLD };
        // A minute to answer LocalSend (it then refuses by itself); the bar stays until the transfer's last notice.
        if n.kind == RECEIVE && n.target.starts_with("ls-ask:") { n.hold_ms = 62_000; } else if n.kind == RECEIVE && n.target == "ls-busy" { n.hold_ms = 86_400_000; }
        let (clickable, hold) = (true, n.hold_ms);
        self.notice = Some(n);
        self.nspring.set(1.0);
        // Every notice can be clicked, also on the floating capsule (which otherwise lets clicks through).
        if clickable { self.click_through(false); }
        let _ = SetTimer(self.hwnd, T_NOTICE, hold, None);
        self.animate();
    }
    /// A chat card as wide as its words need (the header too) and as tall as the lines the message takes.
    unsafe fn chat_size(&self, n: &Notice) -> (f32, f32, f32) {
        let metrics = |l: &IDWriteTextLayout| { let mut m = DWRITE_TEXT_METRICS::default(); let _ = l.GetMetrics(&mut m); m };
        let (name, group, lines) = chat_words(&n.title, &n.detail);
        let mut head = self.notice_text(&name, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 400.0).map_or(120.0, |l| metrics(&l).width);
        if let Some(g) = &group { head += self.notice_text(&format!(" – {g}"), 15.0, DWRITE_FONT_WEIGHT_NORMAL, 400.0).map_or(80.0, |l| metrics(&l).width); }
        // Room for "Reply" (shown on hover) or "now" in the top right corner, beside the name.
        let label = if crate::whatsapp::is_reply_target(&n.target) { 44.0 } else { 30.0 };
        let text_w = CHAT_WIDTH.1 - CHAT_TEXT_X - CHAT_PAD;
        // Several messages: one line each; one message: as many lines as it takes (up to two).
        let many = lines.len() > 1;
        let words = if many { lines.iter().filter_map(|p| self.notice_text(p.trim_start_matches(MINE), 14.0, DWRITE_FONT_WEIGHT_NORMAL, text_w)).map(|l| metrics(&l).width).fold(0.0, f32::max) }
            else { self.notice_lines(&lines[0], 14.0, text_w, CHAT_LINES).map_or(0.0, |l| metrics(&l).width) };
        let w = (CHAT_TEXT_X + head + 14.0 + label + CHAT_PAD).max(CHAT_TEXT_X + words + CHAT_PAD).clamp(CHAT_WIDTH.0, CHAT_WIDTH.1).ceil();
        let replying = self.reply.as_ref().is_some_and(|r| r.key == n.target);
        let max_lines = if replying { CHAT_LINES + 1 } else { CHAT_LINES };
        let h = if many { lines.len() as f32 * CHAT_ROW } else { self.notice_lines(&lines[0], 14.0, w - CHAT_TEXT_X - CHAT_PAD, max_lines).map_or(18.0, |l| metrics(&l).height) };
        if replying { return (w, (CHAT_MSG_Y + h.ceil()).max(CHAT_PAD + CHAT_PIC) + REPLY_GAP + REPLY_FIELD + CHAT_PAD, CHAT_NOTICE.2); }
        // The name row, the message, the bottom padding; never shorter than the picture with its padding.
        (w, (CHAT_MSG_Y + h.ceil() + CHAT_PAD - 2.0).max(CHAT_NOTICE.1), CHAT_NOTICE.2)
    }
    /// The pointer came onto a chat card or left it: Reply shows while it is there, and the card stays (2.5 s more after).
    unsafe fn notice_hover(&mut self, on: bool) {
        if on == self.notice_hot { return; }
        self.notice_hot = on;
        if self.reply.is_none() && self.notice.as_ref().is_some_and(|n| !receive_rows(n)) { if on { let _ = KillTimer(self.hwnd, T_NOTICE); } else { let _ = SetTimer(self.hwnd, T_NOTICE, 2500, None); } }
        self.animate();
    }
    /// Reply in the card's top right corner (shown while the card is hovered).
    fn on_reply_label(&self, x: f32, y: f32) -> bool {
        // A click there always comes after the pointer arrived (and showed Reply); no need to ask for the hover too.
        if !self.notice.as_ref().is_some_and(|n| n.kind == CHAT && crate::whatsapp::is_reply_target(&n.target)) { return false; }
        let Some((cx, py, ..)) = self.notice_pill() else { return false };
        let size = self.notice_size();
        let edge = cx - size.0 / 2.0 + size.0 - CHAT_PAD;
        x >= edge - 58.0 && x <= edge + 8.0 && y >= py + CHAT_NAME_Y - 8.0 && y <= py + CHAT_NAME_Y + 26.0
    }
    /// The reply field and its send button (notch px): left, top, right, bottom of each, at the card's full size.
    fn reply_rects(&self) -> Option<((f32, f32, f32, f32), (f32, f32, f32, f32))> {
        self.reply.as_ref()?;
        let (cx, py, ..) = self.notice_pill()?;
        let size = self.notice_size();
        let (left, edge) = (cx - size.0 / 2.0 + CHAT_PAD, cx - size.0 / 2.0 + size.0 - CHAT_PAD);
        let (bottom, send_left) = (py + size.1 - CHAT_PAD, edge - REPLY_FIELD);
        Some(((left, bottom - REPLY_FIELD, send_left - REPLY_SEND_GAP, bottom), (send_left, bottom - REPLY_FIELD, edge, bottom)))
    }
    fn in_rect((l, t, r, b): (f32, f32, f32, f32), (x, y): (f32, f32), pad: f32) -> bool { x >= l - pad && x <= r + pad && y >= t - pad && y <= b + pad }
    /// The pointer's shape: an I-beam on the reply field, a hand on send, on Reply, and on the water +1.
    fn cursor(&self) -> Option<windows::core::PCWSTR> {
        if let Some((field, send)) = self.reply_rects() {
            if Self::in_rect(send, self.mouse, 3.0) { return Some(IDC_HAND); }
            if Self::in_rect(field, self.mouse, 2.0) { return Some(IDC_IBEAM); }
        }
        if self.water_hot || (self.notice_hot && self.reply.is_none() && self.on_reply_label(self.mouse.0, self.mouse.1)) { return Some(IDC_HAND); }
        None
    }
    /// The card grows (or shrinks back) to its new size: with the reply field, or without it.
    unsafe fn resize_notice(&mut self) {
        let on_show = self.notice_size();
        let Some(mut c) = self.notice.take() else { return };
        c.from = on_show;
        self.notice = Some(c);
        let size = self.notice.as_ref().map(|c| self.chat_size(c));
        if let (Some(c), Some(size)) = (self.notice.as_mut(), size) { c.size = size; }
        self.rspring = Spring::new(0.0);
        self.rspring.set(1.0);
    }
    /// Reply was clicked: the card holds, grows a field (and a third line of message), and takes the keyboard.
    unsafe fn start_reply(&mut self) {
        let Some(n) = self.notice.as_ref() else { return };
        let (name, ..) = chat_words(&n.title, &n.detail);
        self.reply = Some(ReplyBox { key: n.target.clone(), name, text: Vec::new(), caret: 0, high: None, sending: false, sent_at: None, outgoing: String::new(), error: String::new(), typed_at: None, blink: Instant::now(), opened: Instant::now() });
        // It stays while answered; untouched and empty for REPLY_IDLE, it folds.
        let _ = SetTimer(self.hwnd, T_NOTICE, REPLY_IDLE, None);
        self.resize_notice();
        self.click_through(false);
        let _ = SetForegroundWindow(self.hwnd);
        let _ = SetFocus(self.hwnd);
        self.animate();
    }
    /// Esc, or an empty reply left behind: the field goes, and the card folds after `hold` ms.
    unsafe fn end_reply(&mut self, hold: u32) {
        if let Some(r) = self.reply.take() { if r.typed_at.is_some() { crate::whatsapp::chat_typing(r.key, false); } }
        self.resize_notice();
        let _ = SetTimer(self.hwnd, T_NOTICE, hold, None);
        self.animate();
    }
    /// A typed character (WM_CHAR). True when a reply took it.
    fn reply_char(&mut self, c: u16) -> bool {
        let Some(r) = self.reply.as_mut() else { return false };
        if r.sending { return true; }
        let put = |r: &mut ReplyBox, ch: char| { if r.text.len() < 4096 { r.text.insert(r.caret, ch); r.caret += 1; } };
        match c {
            0x08 => { if r.caret > 0 { r.caret -= 1; r.text.remove(r.caret); } }
            0xD800..=0xDBFF => { r.high = Some(c); return true; }
            0xDC00..=0xDFFF => { if let Some(ch) = r.high.take().and_then(|h| char::decode_utf16([h, c]).next()).and_then(|x| x.ok()) { put(r, ch); } }
            c if c >= 0x20 && c != 0x7f => { if let Some(ch) = char::from_u32(c as u32) { put(r, ch); } }
            // Enter, Esc and Ctrl codes: reply_key has them.
            _ => return true,
        }
        r.error.clear();
        r.blink = Instant::now();
        // "typing…" for the other side, at most every 4 s, only while there are words.
        if !r.text.is_empty() && r.typed_at.map_or(true, |t| t.elapsed() > Duration::from_secs(4)) { r.typed_at = Some(Instant::now()); crate::whatsapp::chat_typing(r.key.clone(), true); }
        self.animate();
        true
    }
    /// A key (WM_KEYDOWN) while a reply is typed: Enter sends, Esc cancels, arrows/Home/End move, Delete, Ctrl+V pastes.
    unsafe fn reply_key(&mut self, vk: u16) -> bool {
        if self.reply.is_none() { return false; }
        let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
        match VIRTUAL_KEY(vk) {
            VK_ESCAPE => { self.end_reply(2500); return true; }
            VK_RETURN => { self.send_reply(); return true; }
            _ => {}
        }
        let Some(r) = self.reply.as_mut() else { return false };
        if r.sending { return true; }
        match VIRTUAL_KEY(vk) {
            VK_LEFT => r.caret = r.caret.saturating_sub(1),
            VK_RIGHT => r.caret = (r.caret + 1).min(r.text.len()),
            VK_HOME => r.caret = 0,
            VK_END => r.caret = r.text.len(),
            VK_DELETE => { if r.caret < r.text.len() { r.text.remove(r.caret); } }
            VIRTUAL_KEY(0x56) if ctrl => {
                // Ctrl+V: the clipboard's text, one line.
                if let Some(t) = crate::board::board_clip_text() {
                    for ch in t.chars().map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }) { if r.text.len() >= 4096 { break; } r.text.insert(r.caret, ch); r.caret += 1; }
                }
            }
            _ => return true,
        }
        r.blink = Instant::now();
        self.animate();
        true
    }
    /// Enter: the words go to WhatsApp or Telegram off this thread; reply_done hears the answer.
    fn send_reply(&mut self) {
        let h = self.hwnd.0 as isize;
        let Some(r) = self.reply.as_mut() else { return };
        let text: String = r.text.iter().collect();
        if r.sending || text.trim().is_empty() { return; }
        r.sending = true;
        r.outgoing = text.trim().to_string();
        r.error.clear();
        let key = r.key.clone();
        tauri::async_runtime::spawn(async move {
            let res = if let Some(c) = key.strip_prefix("wa:") { crate::whatsapp::wa_send(c.to_string(), text).await }
                else if let Some(c) = key.strip_prefix("tg:") { crate::telegram::tg_send(c.to_string(), text).await }
                else { Err("Sora cannot answer this chat.".into()) };
            if let Ok(mut d) = REPLY_DONE.lock() { *d = Some(res); }
            unsafe { let _ = PostMessageW(HWND(h as _), WM_REPLY_DONE, WPARAM(0), LPARAM(0)); }
        });
        self.animate();
    }
    /// A sent reply joins the card's lines (the oldest leaves at the top when there are three), like a message coming in.
    unsafe fn add_mine(&mut self, text: &str) {
        let on_show = self.notice_size();
        let Some(c) = self.notice.as_mut() else { return };
        let mut lines: Vec<String> = c.detail.split(MSG_SEP).map(str::to_string).collect();
        lines.push(format!("{MINE}{}", text.replace(MSG_SEP, " ")));
        let (keep, dropped) = keep_recent(lines);
        c.dropped = dropped;
        c.detail = keep.join(&MSG_SEP.to_string());
        c.merged = Instant::now();
        c.from = on_show;
        let size = self.notice.as_ref().map(|c| self.chat_size(c));
        if let (Some(c), Some(size)) = (self.notice.as_mut(), size) { c.size = size; }
        self.rspring = Spring::new(0.0);
        self.rspring.set(1.0);
    }
    /// Sent: "Sent", then the card folds. Not sent: the reason in red, the words kept.
    unsafe fn reply_done(&mut self) {
        let res = REPLY_DONE.lock().ok().and_then(|mut d| d.take());
        let Some(r) = self.reply.as_mut() else { return };
        r.sending = false;
        match res {
            // Sent: "Sent" for a moment, and the field is empty again for the next one; the card stays (REPLY_IDLE).
            Some(Ok(())) => { let mine = std::mem::take(&mut r.outgoing); self.add_mine(&mine); let Some(r) = self.reply.as_mut() else { return }; r.sent_at = Some(Instant::now()); r.text.clear(); r.caret = 0; r.typed_at = None; r.blink = Instant::now(); let _ = SetTimer(self.hwnd, T_NOTICE, REPLY_IDLE, None); }
            Some(Err(e)) => r.error = e,
            None => {}
        }
        self.animate();
    }
    unsafe fn end_notice(&mut self) {
        self.reply = None;
        if let Some(n) = self.notice.take() { let _ = n.done.send(()); }
        self.nspring = Spring::new(0.0);
        // The next one in line grows out again.
        if !self.queue.is_empty() { let next = self.queue.remove(0); self.show_notice(next); return; }
        // The capsule goes back to letting clicks through.
        if self.edge.to > 0.5 && self.dwell.is_none() { self.click_through(true); }
    }
    /// A point (notch px) on a reminder that opens something when clicked.
    fn on_notice(&self, x: f32, y: f32) -> bool {
        let clickable = self.notice.is_some();
        clickable && self.notice_pill().map_or(false, |(cx, py, pw, ph, ..)| (x - cx).abs() <= pw / 2.0 && (py..=py + ph).contains(&y))
    }
    /// The water reminder's +1 button (notch px): right end of the pill, vertically centred.
    fn water_button(&self) -> Option<(f32, f32, f32, f32)> {
        let (cx, py, _, ph, ..) = self.notice_pill()?;
        let size = self.notice_size();
        let right = cx - size.0 / 2.0 + size.0 - 24.0;
        Some((right - 30.0, py + ph / 2.0 - 18.0, right, py + ph / 2.0 + 18.0))
    }
    /// The permission request an agent notice asks about (its Allow / Deny buttons answer it).
    fn agent_ask(&self) -> Option<String> { self.notice.as_ref().filter(|n| n.kind == 5).and_then(|n| n.target.strip_prefix("agent-ask:").map(str::to_string)) }
    /// Deny and Allow on an agent's permission notice (notch px): the right end of the pill, Allow last.
    fn agent_buttons(&self) -> Option<((f32, f32, f32, f32), (f32, f32, f32, f32))> {
        let (cx, py, _, ph, ..) = self.notice_pill()?;
        let size = self.notice_size();
        let (right, cy) = (cx - size.0 / 2.0 + size.0 - 24.0, py + ph / 2.0);
        Some(((right - 58.0 - 8.0 - 56.0, cy - 15.0, right - 58.0 - 8.0, cy + 15.0), (right - 58.0, cy - 15.0, right, cy + 15.0)))
    }
    /// Deny and Accept under the words of LocalSend's question (notch px), side by side across the card.
    fn receive_buttons(&self) -> Option<(String, (f32, f32, f32, f32), (f32, f32, f32, f32))> {
        let id = self.notice.as_ref().filter(|n| n.kind == RECEIVE).and_then(|n| n.target.strip_prefix("ls-ask:"))?.to_string();
        let (cx, py, ..) = self.notice_pill()?;
        let size = self.notice_size();
        let (l, r, t) = (cx - size.0 / 2.0 + 16.0, cx + size.0 / 2.0 - 16.0, py + NOTICE.1 - 4.0);
        Some((id, (l, t, cx - 4.0, t + 30.0), (cx + 4.0, t, r, t + 30.0)))
    }
    fn on_water_button(&self, x: f32, y: f32) -> bool {
        self.water_button().map_or(false, |(l, t, r, b)| x >= l - 6.0 && x <= r + 6.0 && y >= t - 6.0 && y <= b + 6.0)
    }
    /// The reminder's pill: centre x, top, width, height, corner and opacity (notch px), and how far it has grown (0..1).
    /// As the capsule it is the capsule itself, growing out of it and folding back into it; under the mini notch or the
    /// panel it hangs a gap below; in between (the capsule opening or closing) it slides between the two places.
    fn notice_size(&self) -> (f32, f32, f32) {
        // Each notice's size is set when it shows (a permission notice is wider: its Deny and Allow sit beside the words);
        // after a merge, a reply or a swap it glides from the size it had.
        match self.notice.as_ref() { Some(n) => { let k = smooth(self.rspring.x.clamp(0.0, 1.0)); (lerp(n.from.0, n.size.0, k), lerp(n.from.1, n.size.1, k), lerp(n.from.2, n.size.2, k)) }
            None => NOTICE }
    }
    fn notice_pill(&self) -> Option<(f32, f32, f32, f32, f32, f32, f32)> {
        self.notice.as_ref()?;
        let (w, h, _, top, ..) = self.shape();
        let a = smooth(self.edge.x.clamp(0.0, 1.0));
        let n = self.nspring.x;
        let size = self.notice_size();
        let (pw, ph) = (lerp(lerp(64.0, w, a), size.0, n), lerp(lerp(10.0, h, a), size.1, n));
        let cx = center(self.align, w).clamp(EAR + pw / 2.0, (CW - EAR - pw / 2.0).max(EAR + pw / 2.0));
        let y = lerp(top + h + NOTICE_GAP, top, a);
        let alpha = (n.clamp(0.0, 1.0) + a * (1.0 - n.clamp(0.0, 1.0))).min(1.0);
        Some((cx, y, pw.max(0.0), ph.max(0.0), (ph / 2.0).min(size.2).max(0.0), alpha, n.clamp(0.0, 1.0)))
    }
    /// Client px of the window (which covers only the drawn area) to notch px.
    fn design(&self, x: f32, y: f32) -> (f32, f32) { let k = self.fit * self.dpi; ((x + self.bbox.0 as f32) / k, (y + self.bbox.1 as f32) / k - self.slide()) }
    /// The current notch: width, height, bottom radius, top offset, wing size and top corner radius.
    /// One spring opens the panel, another shrinks the mini notch into the capsule; every measure follows them together.
    fn shape(&self) -> (f32, f32, f32, f32, f32, f32) { notch_shape(self.open.x, self.tall.x, self.edge.x, self.peek.x) }
    fn slide(&self) -> f32 { let (_, h, _, top, _, _) = self.shape(); -(1.0 - self.shown.x.clamp(0.0, 1.0)) * (h + top + 16.0) }
    unsafe fn click_through(&self, on: bool) {
        // Never while a clickable reminder is out.
        let on = on && self.notice.is_none();
        let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
        let want = if on { ex | WS_EX_TRANSPARENT.0 as isize } else { ex & !(WS_EX_TRANSPARENT.0 as isize) };
        if want != ex { SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, want); }
    }

    // ---- showing, hiding, the capsule
    unsafe fn reveal(&mut self) {
        if crate::desktop::edge_enabled(&self.app) && self.shown.to < 0.5 {
            self.shown = Spring::new(1.0);
            self.edge = Spring::new(1.0);
        }
        self.want_shown = true; self.shown.set(1.0); self.edge.set(0.0); self.dwell = None; self.peek.set(0.0);
        let _ = KillTimer(self.hwnd, T_EDGE);
        self.click_through(false);
        // Floating notch on: a click outside the mini notch sends it back into the capsule (the click that opened it is not one).
        if crate::desktop::edge_enabled(&self.app) {
            self.buttons_down = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|k| GetAsyncKeyState(k.0 as i32) < 0);
            let _ = SetTimer(self.hwnd, T_OUTSIDE, 16, None);
        }
        self.animate();
    }
    unsafe fn hide(&mut self) {
        self.collapse();
        let _ = KillTimer(self.hwnd, T_OUTSIDE);
        if crate::desktop::edge_enabled(&self.app) {
            // Hidden with the edge handle on: shrink into a click-through capsule and watch for a resting pointer.
            if self.shown.to < 0.5 { self.shown = Spring::new(1.0); self.edge = Spring::new(1.0); }
            self.want_shown = true; self.edge.set(1.0); self.peek.set(0.0); self.click_through(true);
            let _ = SetTimer(self.hwnd, T_EDGE, 40, None);
        } else {
            self.dwell = None; let _ = KillTimer(self.hwnd, T_EDGE);
            self.want_shown = false; self.shown.set(0.0);
        }
        self.animate();
    }
    /// The pointer rests on the area the mini notch would cover: the capsule only peeks (eyes blink); a click brings Sora back.
    unsafe fn edge_tick(&mut self) {
        if self.edge.to < 0.5 { let _ = KillTimer(self.hwnd, T_EDGE); return; }
        let mut c = POINT::default();
        let held = GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0;
        // A press while the notch is already growing under the pointer is a click on it, not a drag: it keeps the hover.
        let inside = (!held || self.dwell.is_some()) && GetCursorPos(&mut c).is_ok() && {
            let k = self.fit * self.dpi;
            let (x, y) = ((c.x - self.origin.0) as f32 / k, (c.y - self.origin.1) as f32 / k);
            // The capsule itself (plus 5 px), and later the notch it is growing into.
            let (w, h, _, top, ..) = self.shape();
            (x - center(self.align, w)).abs() <= w / 2.0 + 5.0 && (-2.0..=top + h + 5.0).contains(&y)
        };
        match (inside, self.dwell) {
            (true, None) => {
                // The pointer rests on the capsule: it starts to grow at once, the little eyes appear, the panel page starts loading,
                // and the window takes clicks (a click now opens the panel without waiting for the growth to finish).
                self.dwell = Some((Instant::now(), self.edge.x));
                self.peek.set(1.0);
                self.click_through(false);
                self.mascot.hidden = false; self.mascot.awake_until = Some(Instant::now() + Duration::from_secs(4));
                self.mascot.blink_at = self.mascot.clock + 0.35; self.mascot.blink_start = -10.0;
                let _ = KillTimer(self.hwnd, T_PREWARM);
                self.open_page();
            }
            // Hovering only peeks (the capsule with blinking eyes); a click opens the mini notch.
            (true, Some(_)) => return,
            (false, Some(_)) => { self.dwell = None; self.peek.set(0.0); self.edge.set(1.0); self.click_through(true); let _ = SetTimer(self.hwnd, T_PREWARM, PREWARM, None); }
            (false, None) => return,
        }
        self.animate();
    }

    // ---- input
    fn face_rect(&self) -> (f32, f32, f32, f32) { let c = anchor(self.align); (c - COMPACT.0 / 2.0, 0.0, c + COMPACT.0 / 2.0, COMPACT.1) }
    unsafe fn mouse_move(&mut self, x: f32, y: f32) {
        if !self.tracking { let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 }; let _ = TrackMouseEvent(&mut t); self.tracking = true; }
        let hot = self.drank_at.is_none() && self.nspring.to > 0.5 && self.notice.as_ref().is_some_and(|n| n.kind == 3) && self.on_water_button(x, y);
        if hot != self.water_hot { self.water_hot = hot; if !hot { self.press.set(0.0); } }
        self.mouse = (x, y);
        let card = self.nspring.to > 0.5 && self.notice.as_ref().is_some_and(|n| n.kind == CHAT) && self.on_notice(x, y);
        self.notice_hover(card);
        if self.open.to < 0.5 && self.edge.to < 0.5 {
            let (l, t, r, b) = self.face_rect();
            if x >= l && x <= r && y >= t && y <= b {
                if !self.mascot.hover {
                    self.mascot.hover = true; self.mascot.blink_at = self.mascot.clock + 3.0; self.mascot.blink_start = -10.0;
                    // Likely to be clicked: start loading the panel page now, so it is ready when the panel opens.
                    let _ = KillTimer(self.hwnd, T_PREWARM);
                    self.open_page();
                }
                self.mascot.px = ((x - anchor(self.align)) / (COMPACT.0 / 2.0)).clamp(-1.0, 1.0);
                self.mascot.py = ((y - COMPACT.1 / 2.0) / (COMPACT.1 / 2.0)).clamp(-1.0, 1.0);
            } else { self.leave(); }
        }
        self.animate();
    }
    fn leave(&mut self) {
        if self.mascot.hover {
            self.mascot.hover = false; self.mascot.awake_until = Some(Instant::now() + Duration::from_secs(5)); self.mascot.px = 0.0; self.mascot.py = 0.0;
            // Not clicked: the page loaded on hover goes away a little later.
            if self.open.to < 0.5 { unsafe { let _ = SetTimer(self.hwnd, T_PREWARM, PREWARM, None); } }
        }
    }
    /// Hides the page and asks WebView2 to suspend it and use little memory. Kept for KEEP_WARM, then dropped.
    fn sleep_page(&self) {
        let Some(w) = self.app.get_webview_window("panel") else { PAGE_SHOWN.store(false, Ordering::SeqCst); return };
        if !PAGE_USED.load(Ordering::SeqCst) { self.close_page(); return; }
        // The page stays visible, cut to the notch's shape each frame (clip_page), while its contents shrink and fade; finish_sleep hides it after.
        SLEEP_PENDING.store(true, Ordering::SeqCst);
        let _ = w.emit("panel-hide", ());
        unsafe { let _ = SetTimer(self.hwnd, T_DROP, KEEP_WARM, None); let _ = SetTimer(self.hwnd, T_TRIM, 1500, None); }
    }
    /// The panel has shrunk away: now hide the page and suspend it.
    fn finish_sleep(&self) {
        SLEEP_PENDING.store(false, Ordering::SeqCst);
        PAGE_SHOWN.store(false, Ordering::SeqCst);
        let Some(w) = self.app.get_webview_window("panel") else { return };
        let _ = w.hide();
        self.clip_page(true);
        let _ = w.with_webview(|wv| unsafe {
            let c = wv.controller();
            let _ = c.SetIsVisible(false);
            if let Ok(core) = c.CoreWebView2() {
                if let Ok(c19) = core.cast::<ICoreWebView2_19>() { let _ = c19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW); }
                if let Ok(c3) = core.cast::<ICoreWebView2_3>() { let _ = c3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())))); }
            }
        });
    }
    /// The page is part of the notch: only the part inside the notch's current shape shows, ears included (the page is an
    /// ear wider on each side, so the music cover can reach into them). Applied on this thread in the same frame as the
    /// shape, so nothing trails behind it; an unchanged shape is not applied again.
    /// The page's window is only as tall as the panel is going to be, not as tall as it can ever get: its transparent part below the
    /// panel would still catch the clicks meant for what lies under it (the Open button of a file dialog). It grows at once when the
    /// panel is about to grow and shrinks once the panel has settled.
    fn fit_page_height(&self) {
        let Some(w) = self.app.get_webview_window("panel") else { return };
        let px = self.dpi * self.fit;
        let want = ((PANEL.1 + TALL * self.tall.to) * px).round() as u32 + 2;
        let now = self.page_h.get();
        let target = if self.tall.busy() { want.max(now) } else { want };
        if target == now { return; }
        self.page_h.set(target);
        let _ = w.set_size(tauri::PhysicalSize::new(((PANEL.0 + 2.0 * EAR) * px).round() as u32, target));
    }
    fn clip_page(&self, full: bool) {
        let Some(w) = self.app.get_webview_window("panel") else { return };
        let Ok(h) = w.hwnd() else { return };
        let hw = HWND(h.0 as _);
        // The region goes on WebView2's own child window: on the top-level window it makes Windows paint a classic title bar.
        unsafe {
            let mut kids: Vec<HWND> = Vec::new();
            let _ = EnumChildWindows(hw, Some(collect_child), LPARAM(&mut kids as *mut Vec<HWND> as isize));
            let Ok(mut last) = CLIP.lock() else { return };
            if full { for k in &kids { SetWindowRgn(*k, HRGN::default(), false); } *last = None; return; }
            let (sw, sh, r, _, ear, _) = self.shape();
            let (k, c, x0) = (self.fit * self.dpi, center(self.align, sw), MID - PANEL.0 / 2.0 - EAR);
            let (l, rt, b, d, e) = (((c - sw / 2.0 - x0) * k).round() as i32, ((c + sw / 2.0 - x0) * k).round() as i32 + 1, (sh * k).round() as i32 + 1, ((r * 2.0 * k).round() as i32).max(2), (ear * k).round() as i32);
            if *last == Some([l, rt, b, d, e]) { return; }
            *last = Some([l, rt, b, d, e]);
            for kid in &kids {
                // Starts above the window so the top corners stay off-screen; only the bottom corners are round. The ears are
                // squares beside the top: the page draws their curve itself.
                let rgn = CreateRoundRectRgn(l, -d, rt, b, d, d);
                if e > 0 {
                    for (a, z) in [(l - e, l + 1), (rt - 1, rt + e)] { let ear = CreateRectRgn(a, 0, z, e); CombineRgn(rgn, rgn, ear, RGN_OR); let _ = DeleteObject(ear); }
                }
                SetWindowRgn(*kid, rgn, false);
            }
        }
    }
    /// Wakes a sleeping page (normal memory, running scripts) before it is shown.
    fn wake_page(&self) {
        let Some(w) = self.app.get_webview_window("panel") else { return };
        let _ = w.with_webview(|wv| unsafe {
            let c = wv.controller();
            if let Ok(core) = c.CoreWebView2() {
                if let Ok(c19) = core.cast::<ICoreWebView2_19>() { let _ = c19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL); }
                if let Ok(c3) = core.cast::<ICoreWebView2_3>() { let _ = c3.Resume(); }
            }
            let _ = c.SetIsVisible(true);
        });
    }
    fn close_page(&self) {
        WANT_PAGE.store(false, Ordering::SeqCst);
        PAGE_USED.store(false, Ordering::SeqCst);
        PAGE_READY.store(false, Ordering::SeqCst); PAGE_SHOWN.store(false, Ordering::SeqCst);
        PAGE_ALIVE.store(false, Ordering::SeqCst);
        if let Some(w) = self.app.get_webview_window("panel") { let _ = w.destroy(); }
    }
    /// Shows the loaded page as soon as the panel starts opening; its contents then fade in.
    fn show_page_when_open(&self) {
        // Early: clip_page cuts it to the growing shape, and its contents grow and fade in with the notch.
        if self.open.to > 0.5 && self.open.x > 0.2 && PAGE_READY.load(Ordering::SeqCst) && !PAGE_SHOWN.swap(true, Ordering::SeqCst) {
            PAGE_USED.store(true, Ordering::SeqCst);
            if let Some(w) = self.app.get_webview_window("panel") {
                let _ = w.show(); let _ = w.set_focus();
                let _ = w.emit("panel-show", ());
            }
        }
    }
    unsafe fn click(&mut self, x: f32, y: f32) {
        // A notice: a chat opens what it is about, the water one counts a glass on its +1 button, any other opens
        // the notch; the notice folds away.
        if self.on_notice(x, y) {
            if let Some((_, send)) = self.reply_rects() {
                let _ = SetForegroundWindow(self.hwnd); let _ = SetFocus(self.hwnd);
                if Self::in_rect(send, (x, y), 3.0) { self.send_reply(); }
                return;
            }
            if self.on_reply_label(x, y) { self.start_reply(); return; }
            if let Some((id, deny, accept)) = self.receive_buttons() {
                let hit = |(l, t, r, b): (f32, f32, f32, f32)| x >= l - 3.0 && x <= r + 3.0 && y >= t - 3.0 && y <= b + 3.0;
                let pick = if hit(accept) { Some(true) } else if hit(deny) { Some(false) } else { None };
                if let Some(accept) = pick {
                    if self.notice.as_ref().is_some_and(|n| n.born.elapsed() >= Duration::from_millis(600)) {
                        let _ = KillTimer(self.hwnd, T_NOTICE);
                        self.nspring.set(0.0);
                        self.animate();
                        crate::lan_receive::decide(&id, accept);
                    }
                }
                return;
            }
            if self.notice.as_ref().is_some_and(|n| n.target == "ls-busy") { return; }
            // Allow / Deny on an agent's permission notice answers it right there (not in the first moments: a click aimed
            // at something else must not land on Allow); the rest of the card opens the Agents tab.
            if let (Some(id), Some((deny, allow))) = (self.agent_ask(), self.agent_buttons()) {
                let hit = |(l, t, r, b): (f32, f32, f32, f32)| x >= l - 3.0 && x <= r + 3.0 && y >= t - 3.0 && y <= b + 3.0;
                let pick = if hit(allow) { Some(true) } else if hit(deny) { Some(false) } else { None };
                if let Some(allow) = pick {
                    if self.notice.as_ref().is_some_and(|n| n.born.elapsed() >= Duration::from_millis(600)) {
                        let _ = KillTimer(self.hwnd, T_NOTICE);
                        self.nspring.set(0.0);
                        self.animate();
                        crate::agents::decide_from_notice(&id, allow);
                    }
                    return;
                }
            }
            // +1 glass: the button springs back, fills green with a tick, and the notice folds a moment later.
            if self.notice.as_ref().map_or(false, |n| n.kind == 3) && self.on_water_button(x, y) {
                if self.drank_at.is_none() {
                    self.drank_at = Some(Instant::now());
                    self.water_hot = false;
                    self.press.set(0.0);
                    crate::health::health_drank();
                    let _ = SetTimer(self.hwnd, T_NOTICE, 520, None);
                    self.animate();
                }
                return;
            }
            // Every notice opens its own tab in Sora: agent and recorder notices theirs, a chat card the Chat history on its
            // side ("cat:ai" / "cat:social"), a linked WhatsApp / Telegram chat with its reply box ("wa:" / "tg:").
            let route = self.notice.as_ref().and_then(|n| match n.kind {
                5 => Some(("agents", None)),
                6 => Some(("meeting", None)),
                RECEIVE => Some(("shelf", None)),
                CHAT if crate::whatsapp::is_reply_target(&n.target) => Some(("history", Some(n.target.clone()))),
                // An AI app's card (Claude, ChatGPT/Codex...) is about its agent: the Agents tab while that is on.
                CHAT if app_info(&n.label).1 == "ai" && crate::desktop::preferences(&self.app).feature_agents => Some(("agents", None)),
                CHAT => Some(("history", Some(format!("cat:{}", app_info(&n.label).1)))),
                _ => None });
            let target = if route.is_some() { String::new() } else { self.notice.as_ref().map(|n| n.target.clone()).filter(|t| !t.starts_with("agent-ask:")).unwrap_or_default() };
            if let Some((_, Some(key))) = &route { crate::whatsapp::want_reply(&self.app, key); }
            let offer = route.map(|r| r.0);
            let _ = KillTimer(self.hwnd, T_NOTICE);
            self.nspring.set(0.0);
            self.animate();
            if !target.is_empty() { crate::chat_notices::open(target); }
            // With the floating notch on, out of the capsule and straight into the panel (not only the mini notch).
            else if let Some(tab) = offer { crate::desktop::want_tab(&self.app, tab); if self.edge.to > 0.5 { self.dwell = None; self.reveal(); crate::desktop::reveal(&self.app); } self.expand(); }
            else if self.edge.to > 0.5 { self.dwell = None; self.reveal(); crate::desktop::reveal(&self.app); }
            else { self.expand(); }
            return;
        }
        // Clicked while the capsule is still growing (or just reached): open now, no waiting for the animation.
        let (w, h, _, top, ..) = self.shape();
        if self.edge.x > 0.02 && self.edge.to > 0.5 && self.open.to < 0.5 && (x - center(self.align, w)).abs() <= w / 2.0 + 5.0 && y <= top + h + 5.0 {
            self.dwell = None;
            self.reveal(); crate::desktop::reveal(&self.app);
            return;
        }
        let (l, t, r, b) = self.face_rect();
        if self.open.to < 0.5 && self.edge.to < 0.5 && x >= l && x <= r && y >= t && y <= b { self.expand(); }
    }
    unsafe fn expand(&mut self) {
        if self.open.to < 0.5 { let fg = GetForegroundWindow(); self.before = if !fg.0.is_null() && crate::desktop::foreign_window(fg) { fg.0 as isize } else { 0 }; }
        self.open.set(1.0);
        let _ = KillTimer(self.hwnd, T_PREWARM);
        self.leave();
        let _ = KillTimer(self.hwnd, T_PREWARM);
        let _ = SetForegroundWindow(self.hwnd);
        let _ = SetTimer(self.hwnd, T_OUTSIDE, 16, None);
        self.buttons_down = true;
        // Opened again while the page was still shrinking: let it grow back (only a real open, not a hover).
        // SetForegroundWindow above put the notch over the still-visible page: bring the page back on top.
        if SLEEP_PENDING.swap(false, Ordering::SeqCst) {
            if let Some(w) = self.app.get_webview_window("panel") { let _ = w.set_focus(); let _ = w.emit("panel-show", ()); }
        }
        self.open_page();
        self.animate();
    }
    unsafe fn collapse(&mut self) {
        if self.open.to < 0.5 { return; }
        self.open.set(0.0);
        self.tall.set(0.0);
        // A hold belongs to the open panel: the next one starts without it (outside clicks close it again).
        crate::desktop::tray_release();
        // With the floating notch the mini notch keeps watching for an outside click (back into the capsule).
        if !crate::desktop::edge_enabled(&self.app) { let _ = KillTimer(self.hwnd, T_OUTSIDE); }
        // The page is hidden and suspended (its scripts stop, memory is trimmed); it comes back instantly on the next open.
        self.sleep_page();
        self.animate();
    }
    /// Another app came to the front (a click, Alt+Tab, the taskbar): the open panel folds into the mini notch, also while
    /// the tray waits for the next file. Not while a file is dragged in or out, nor for Sora's own windows or the switcher; while
    /// the tray (or Send in use) holds the panel, going back to the window the files came from is not a switch either.
    unsafe fn foreground_changed(&mut self, fg: HWND) {
        // Another app came to the front during an empty reply: it is dropped (with words in it, it waits for a click back).
        if self.reply.as_ref().is_some_and(|r| r.text.is_empty() && !r.sending) && !fg.0.is_null() && crate::desktop::foreign_window(fg) { self.end_reply(2500); }
        if self.open.to < 0.5 || fg.0.is_null() || !crate::desktop::foreign_window(fg) { return; }
        if crate::desktop::DRAG_ACTIVE.load(Ordering::SeqCst) || crate::drag_out::dragging_out(&self.app) { return; }
        if crate::desktop::tray_held() && (self.tab == Tab::Send || same_place(fg, HWND(self.before as _))) { return; }
        self.collapse();
    }
    /// Position, display or size changed in Settings: close the panel and its page (built for the old place), then move.
    unsafe fn relayout(&mut self) {
        self.collapse();
        self.close_page();
        let _ = SetTimer(self.hwnd, T_WARM, WARM_AFTER, None);
        self.place();
        self.drawn = None;
        self.animate();
    }
    unsafe fn show_send(&mut self) {
        self.tab = Tab::Send;
        self.reveal();
        if self.open.to < 0.5 { self.expand(); } else { let _ = self.app.emit_to("panel", "panel-tab", "send"); }
        self.animate();
    }

    /// A file drag from Explorer: the Tray opens (out of the capsule if needed) so the file can be dropped on it.
    unsafe fn drag_offer(&mut self) {
        self.tab = Tab::Shelf;
        self.reveal();
        if self.open.to < 0.5 { self.expand(); }
        // Also for a warm page that still shows the previous tab (the event waits for it to resume).
        let _ = self.app.emit_to("panel", "panel-tab", "shelf");
    }
    /// Builds the panel page over the open panel (720x234 notch px at the notch scale, with room below for a Home menu's taller panel), hidden until it has drawn.
    fn open_page(&self) {
        WANT_PAGE.store(true, Ordering::SeqCst);
        unsafe { let _ = KillTimer(self.hwnd, T_DROP); }
        if PAGE_ALIVE.load(Ordering::SeqCst) { if !SLEEP_PENDING.load(Ordering::SeqCst) { self.wake_page(); } return; }
        if BUILDING.swap(true, Ordering::SeqCst) { return; }
        let k = self.fit;
        // Physical pixels: the chosen display may not have the primary display's scale.
        let px = self.dpi * k;
        // An ear wider on each side than the panel (see clip_page).
        let (x, y) = (self.origin.0 + ((MID - PANEL.0 / 2.0 - EAR) * px).round() as i32, self.origin.1);
        let (pw, ph) = (((PANEL.0 + 2.0 * EAR) * px).round() as u32, ((PANEL.1 + TALLEST) * px).round() as u32);
        self.page_h.set(ph);
        let tab = match self.tab { Tab::Home => "home", Tab::Music => "music", Tab::Shelf => "shelf", Tab::Send => "send" };
        let url = format!("index.html?panel=1&scale={k:.4}&tab={tab}");
        let app = self.app.clone();
        // Built off this thread: Tauri creates windows through its own event loop.
        std::thread::spawn(move || {
            // The previous page may still be on its way out under the same label: try again for a moment.
            let mut built = None;
            for _ in 0..20 {
                if !WANT_PAGE.load(Ordering::SeqCst) { break; }
                if app.get_webview_window("panel").is_none() {
                    built = tauri::WebviewWindowBuilder::new(&app, "panel", tauri::WebviewUrl::App(url.clone().into())).title("Sora").inner_size(((PANEL.0 + 2.0 * EAR) * k) as f64, ((PANEL.1 + TALLEST) * k) as f64)
                        .decorations(false).transparent(true).shadow(false).resizable(false).always_on_top(true).skip_taskbar(true).visible(false).build().ok();
                    if let Some(w) = &built { let _ = w.set_position(tauri::PhysicalPosition::new(x, y)); let _ = w.set_size(tauri::PhysicalSize::new(pw, ph)); break; }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            // Closed (or the hover went away) while it was being built: do not leave it behind.
            if let Some(w) = built { if WANT_PAGE.load(Ordering::SeqCst) { PAGE_ALIVE.store(true, Ordering::SeqCst); } else { let _ = w.destroy(); } }
            BUILDING.store(false, Ordering::SeqCst);
        });
    }
    unsafe fn timer(&mut self, id: usize) {
        match id {
            // The panel stays open until a click lands outside it (the click itself still goes to the other app);
            // not while a file is dragged or the tray waits for the next file.
            T_OUTSIDE => {
                let down = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON].iter().any(|k| GetAsyncKeyState(k.0 as i32) < 0);
                if down && !self.buttons_down && !crate::desktop::tray_held() && !crate::desktop::DRAG_ACTIVE.load(Ordering::SeqCst) {
                    let mut c = POINT::default();
                    if GetCursorPos(&mut c).is_ok() {
                        let k = self.fit * self.dpi;
                        let (x, y) = ((c.x - self.origin.0) as f32 / k, (c.y - self.origin.1) as f32 / k);
                        let (w, h, ..) = self.shape();
                        if ((x - center(self.align, w)).abs() > w / 2.0 || y > h) && !self.on_notice(x, y) {
                            // Panel open: back to the mini notch. Mini notch (floating notch on): back into the capsule.
                            if self.open.to > 0.5 { self.collapse(); }
                            else if self.edge.to < 0.5 && crate::desktop::edge_enabled(&self.app) { crate::desktop::hide_now(&self.app); }
                        }
                    }
                }
                self.buttons_down = down;
            }
            T_EDGE => self.edge_tick(),
            T_PREWARM => { let _ = KillTimer(self.hwnd, id); if self.open.to < 0.5 && !PAGE_USED.load(Ordering::SeqCst) { self.close_page(); } }
            T_TRIM => { let _ = KillTimer(self.hwnd, id); if self.open.to < 0.5 { std::thread::spawn(trim_webview); } }
            T_DROP => { let _ = KillTimer(self.hwnd, id); if self.open.to < 0.5 { self.close_page(); let _ = SetTimer(self.hwnd, T_WARM, WARM_AFTER, None); } }
            // Background warm-up: build the page while the notch is closed, then put it to sleep as if it had been used.
            T_WARM => {
                if self.open.to > 0.5 || PAGE_SHOWN.load(Ordering::SeqCst) || SLEEP_PENDING.load(Ordering::SeqCst) { let _ = KillTimer(self.hwnd, id); WARM_TRIES.store(0, Ordering::SeqCst); return; }
                if !PAGE_ALIVE.load(Ordering::SeqCst) && !BUILDING.load(Ordering::SeqCst) {
                    self.open_page();
                    WARM_TRIES.store(0, Ordering::SeqCst);
                    let _ = SetTimer(self.hwnd, T_WARM, WARM_POLL, None);
                } else if PAGE_READY.load(Ordering::SeqCst) {
                    let _ = KillTimer(self.hwnd, id);
                    WARM_TRIES.store(0, Ordering::SeqCst);
                    PAGE_USED.store(true, Ordering::SeqCst);
                    let _ = KillTimer(self.hwnd, T_PREWARM);
                    self.finish_sleep();
                    let _ = SetTimer(self.hwnd, T_DROP, KEEP_WARM, None);
                    let _ = SetTimer(self.hwnd, T_TRIM, 1500, None);
                } else if WARM_TRIES.fetch_add(1, Ordering::SeqCst) > 40 {
                    // Not ready after ~20 s: leave it to the hover/click path.
                    let _ = KillTimer(self.hwnd, id);
                    WARM_TRIES.store(0, Ordering::SeqCst);
                } else { let _ = SetTimer(self.hwnd, T_WARM, WARM_POLL, None); }
            }
            // The reminder has been read: it folds back (into the capsule, or away under the notch).
            T_NOTICE => {
                let _ = KillTimer(self.hwnd, id);
                // Not while the pointer is on the card (leaving restarts it). While answering it stays, until the field has
                // been empty and untouched for REPLY_IDLE (after a send, or never typed in); then it folds.
                if self.notice_hot && self.reply.is_none() { return; }
                if let Some(r) = self.reply.as_ref() {
                    let idle = r.blink.elapsed().as_millis() as u32;
                    if r.sending || !r.text.is_empty() || idle < REPLY_IDLE { let _ = SetTimer(self.hwnd, T_NOTICE, REPLY_IDLE.saturating_sub(idle).max(1000), None); return; }
                    if r.typed_at.is_some() { crate::whatsapp::chat_typing(r.key.clone(), false); }
                }
                self.reply = None;
                // With others waiting the next one takes the same place at once; else the pill folds away.
                if self.queue.is_empty() { self.nspring.set(0.0); }
                else { let next = self.queue.remove(0); self.show_notice(next); }
                self.animate();
            }
            // The focus timer: chime and the floating reminder (no mascot wink), even while the panel page is closed.
            T_TIMER => {
                // The reply field's caret blinks.
                if self.reply.is_some() { self.animate(); }
                let due = DEADLINE.lock().ok().and_then(|d| *d).map_or(false, |end| now_ms() >= end);
                if due {
                    set_deadline(None);
                    chime();
                    crate::notices::timer_finished(&self.app);
                }
            }
            _ => {}
        }
    }

    // ---- frames
    unsafe fn frame(&mut self) -> bool {
        let now = Instant::now();
        let dt = self.last.map_or(1.0 / 60.0, |t| now.saturating_duration_since(t).as_secs_f32()).clamp(0.001, 0.05);
        self.last = Some(now);
        // Opening: a soft spring that barely overshoots; sliding and the capsule: calmer ones.
        self.open.step_with(dt, 19.0, 0.9);
        self.tall.step_with(dt, 17.0, 0.92);
        self.shown.step_with(dt, 10.0, 1.0);
        self.peek.step_with(dt, 16.0, 0.85);
        // Soft, with a little give: the reminder swells out of the notch and settles.
        self.nspring.step_with(dt, 8.5, 0.78);
        self.rspring.step_with(dt, 9.0, 0.85);
        // Pressed in fast, out with a small bounce.
        self.press.step_with(dt, 34.0, 0.5);
        if self.notice.is_some() && self.nspring.to < 0.5 && !self.nspring.busy() { self.end_notice(); }
        if let Some((since, start)) = self.dwell {
            let _ = since;
            self.edge.x = start;
            self.edge.v = 0.0;
        } else {
            self.edge.step_with(dt, 9.0, 0.95);
        }
        self.lid = self.mascot.step(dt, PLAYING.load(Ordering::SeqCst));
        // The start-up greeting waits until the mini notch has landed (and is not the capsule).
        // From the capsule it starts the moment the click begins the growth (no idle beat); at plain start-up it waits for the landing.
        if self.greet_pending && self.shown.x > 0.98 && self.open.to < 0.5 && self.edge.to < 0.5 {
            if self.edge.x < 0.02 { self.greet_pending = false; self.greet_at = Some(now + Duration::from_millis(140)); }
            else if self.edge.x < 0.9 { self.greet_pending = false; self.greet_at = Some(now); }
        }
        if self.greet_at.map_or(false, |t| now >= t) { self.greet_at = None; self.mascot.greet(); }
        if !self.want_shown && self.shown.x <= 0.001 && !self.shown.busy() {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            self.end_notice();
            if let Ok(mut r) = CAPSULE_RECT.lock() { *r = None; }
            if let Ok(mut g) = NOTCH_GEOM.lock() { *g = None; }
            self.last = None;
            return false;
        }
        self.show_page_when_open();
        if PAGE_SHOWN.load(Ordering::SeqCst) {
            if SLEEP_PENDING.load(Ordering::SeqCst) && self.open.x < 0.12 { self.finish_sleep(); }
            else if self.open.busy() || self.shown.busy() || self.tall.busy() || self.open.to > 0.5 { self.fit_page_height(); self.clip_page(false); }
        }
        // Opening, closing, sliding and the capsule run at the display's rate; the face alone needs at most ~60 fps,
        // and its slow idle glance ~30 fps. Springs still move every refresh, only drawing is spaced out.
        let moving = self.open.busy() || self.tall.busy() || self.shown.busy() || self.edge.busy() || self.peek.busy() || self.nspring.busy() || self.rspring.busy() || self.press.busy() || self.drank_at.is_some_and(|t| t.elapsed() < Duration::from_millis(400)) || self.mascot.hover;
        let budget = if moving { 0 } else if self.mascot.idle(PLAYING.load(Ordering::SeqCst)) { 31 } else { 15 };
        if self.drawn.map_or(true, |t| now.saturating_duration_since(t) >= Duration::from_millis(budget)) { self.render(); self.drawn = Some(now); }
        // The capsule at rest (from the springs, not the last drawn frame, which the frame budget may skip).
        // Visually landed is enough (the springs' last fraction of a pixel would only delay a reminder joining it).
        let resting = self.edge.to > 0.5 && self.edge.x > 0.97 && self.shown.to > 0.5 && self.shown.x > 0.99 && self.peek.to < 0.5 && self.peek.x < 0.02;
        if let Ok(mut r) = CAPSULE_RECT.lock() {
            let (w, h, _, top, ..) = self.shape();
            let k = self.fit * self.dpi;
            *r = resting.then(|| (self.origin.0 + ((center(self.align, w) - w / 2.0) * k).round() as i32, self.origin.1 + (top * k).round() as i32, (w * k).round() as i32, (h * k).round() as i32));
            if let Ok(mut g) = NOTCH_GEOM.lock() {
                *g = Some((self.origin.0 + (center(self.align, w) * k).round() as i32, self.origin.1 + ((top + h + self.slide()) * k).round() as i32));
            }
        }
        // Nothing moves while the panel sits open (the page covers it) or while resting as the capsule: stop drawing.
        let settled_open = self.open.to > 0.5 && self.open.x > 0.999 && !self.open.busy() && !self.tall.busy() && !self.shown.busy() && (PAGE_SHOWN.load(Ordering::SeqCst) || !PAGE_ALIVE.load(Ordering::SeqCst) && !BUILDING.load(Ordering::SeqCst));
        let settled_capsule = self.edge.x > 0.999 && !self.edge.busy() && self.dwell.is_none() && !self.shown.busy() && !self.peek.busy();
        // A reminder on screen keeps drawing (its countdown ring), at the idle rate.
        if (settled_open || settled_capsule) && self.notice.is_none() { self.last = None; return false; }
        true
    }
    fn brush(&self, hex: u32, a: f32) -> Option<ID2D1SolidColorBrush> { unsafe { self.rt.CreateSolidColorBrush(&color(hex, a), None).ok() } }
    unsafe fn shape_geometry(&self) -> Option<ID2D1PathGeometry> {
        let (w, h, r, top, ear, tr) = self.shape();
        let mid = center(self.align, w);
        let (l, rt) = (mid - w / 2.0, mid + w / 2.0);
        let g = self.d2d.CreatePathGeometry().ok()?;
        let s = g.Open().ok()?;
        let arc = |x: f32, y: f32, rad: f32, cw: bool| D2D1_ARC_SEGMENT { point: pt(x, y), size: D2D_SIZE_F { width: rad, height: rad }, rotationAngle: 0.0, sweepDirection: if cw { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE }, arcSize: D2D1_ARC_SIZE_SMALL };
        // Top edge with the inverted corner wings (or, as the capsule, rounded top corners), straight sides, rounded bottom.
        if ear > 0.05 {
            s.BeginFigure(pt(l - ear, top), D2D1_FIGURE_BEGIN_FILLED);
            s.AddLine(pt(rt + ear, top));
            s.AddArc(&arc(rt, top + ear, ear, false));
        } else {
            s.BeginFigure(pt(l + tr, top), D2D1_FIGURE_BEGIN_FILLED);
            s.AddLine(pt(rt - tr, top));
            if tr > 0.05 { s.AddArc(&arc(rt, top + tr, tr, true)); }
        }
        s.AddLine(pt(rt, top + h - r));
        if r > 0.05 { s.AddArc(&arc(rt - r, top + h, r, true)); }
        s.AddLine(pt(l + r, top + h));
        if r > 0.05 { s.AddArc(&arc(l, top + h - r, r, true)); }
        if ear > 0.05 {
            s.AddLine(pt(l, top + ear));
            s.AddArc(&arc(l - ear, top, ear, false));
        } else {
            s.AddLine(pt(l, top + tr));
            if tr > 0.05 { s.AddArc(&arc(l + tr, top, tr, true)); }
        }
        s.EndFigure(D2D1_FIGURE_END_CLOSED);
        s.Close().ok()?;
        Some(g)
    }
    unsafe fn render(&mut self) {
        if self.dib.is_invalid() { return; }
        let (w, h, _, top, ..) = self.shape();
        let k = self.fit * self.dpi;
        let dy = self.slide();
        // Only the part that has something on it is drawn and handed to Windows.
        let mid = center(self.align, w);
        let pill = self.notice_pill();
        let (mut left, mut right, mut bottom) = (mid - w / 2.0 - EAR - 2.0, mid + w / 2.0 + EAR + 2.0, top + h);
        if let Some((cx, y, pw, ph, ..)) = pill { left = left.min(cx - pw / 2.0 - 2.0); right = right.max(cx + pw / 2.0 + 2.0); bottom = bottom.max(y + ph); }
        let bx0 = (left * k).floor().max(0.0) as i32;
        let bx1 = (right * k).ceil().min(self.size.0 as f32) as i32;
        let by1 = (((bottom + dy).max(1.0) + 3.0) * k).ceil().clamp(1.0, self.size.1 as f32) as i32;
        self.bbox = (bx0, 0, (bx1 - bx0).max(1), by1);
        let rt = self.rt.clone();
        rt.BeginDraw();
        let clip = rect(bx0 as f32 / self.dpi, 0.0, bx1 as f32 / self.dpi, by1 as f32 / self.dpi);
        let identity = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };
        rt.SetTransform(&identity);
        rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        rt.Clear(Some(&color(0, 0.0)));
        let base = mul(translate(0.0, dy), scale(self.fit, self.fit));
        rt.SetTransform(&base);
        if let (Some(g), Some(b)) = (self.shape_geometry(), self.brush(BLACK, 1.0)) {
            rt.FillGeometry(&g, &b, None);
        }
        if let Some(pl) = pill { self.draw_notice_shape(pl); }
        // The face fades out as the panel opens and as the notch shrinks into the capsule.
        // The face fades in (small at first) as the capsule grows into the mini notch, and out as the panel opens.
        // Just the little eyes show up right when the pointer settles on the capsule (inside it, small), grow and slide down with it;
        // the mouth, the full mascot, only comes in when it is nearly the mini notch.
        let grown = 1.0 - self.edge.x.clamp(0.0, 1.0);
        // Peeking: the eyes keep to the capsule's size (a little space above and below) and sit in its middle.
        let g = smooth(grown);
        let pk = self.peek.x.clamp(0.0, 1.0);
        // A reminder joined with the capsule stands in for it: no face peeking out above it.
        let joined = pill.map_or(0.0, |p| p.6 * smooth(self.edge.x.clamp(0.0, 1.0)));
        let face = (1.0 - smooth(self.open.x / 0.5)) * smooth(grown / 0.45).max(pk) * (1.0 - joined);
        let size = 0.3 + 0.07 * pk * (1.0 - g) + 0.7 * smooth(grown / 0.9);
        let to_mid = top + h / 2.0 - (35.0 - 4.0 * size) * (154.0 / 160.0) + 1.69;
        let lift = -19.0 * (1.0 - g);
        let dy_face = lift + (to_mid - lift) * pk * (1.0 - g);
        if face > 0.01 { self.draw_mascot(base, face, size, dy_face, smooth((grown - 0.55) / 0.4)); }
        if let Some(pl) = pill { rt.SetTransform(&base); self.draw_notice_content(pl); }
        // Recording: a red dot at the notch's right end, always on while the recorder runs.
        if RECORDING.load(Ordering::SeqCst) {
            rt.SetTransform(&base);
            let r = (h * 0.16).clamp(3.0, 5.0);
            if let Some(b) = self.brush(0xff453a, 1.0) { rt.FillEllipse(&D2D1_ELLIPSE { point: pt(mid + w / 2.0 - r * 3.2, top + h / 2.0), radiusX: r, radiusY: r }, &b); }
        }
        rt.SetTransform(&identity);
        rt.PopAxisAlignedClip();
        let _ = rt.EndDraw(None, None);
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: (self.shown.x.clamp(0.0, 1.0) * 255.0) as u8, AlphaFormat: AC_SRC_ALPHA as u8 };
        let pos = POINT { x: self.origin.0 + bx0, y: self.origin.1 };
        let size = SIZE { cx: self.bbox.2, cy: self.bbox.3 };
        let src = POINT { x: bx0, y: 0 };
        let _ = UpdateLayeredWindow(self.hwnd, None, Some(&pos), Some(&size), self.mem, Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA);
        if !IsWindowVisible(self.hwnd).as_bool() { let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE); }
    }
    /// The reminder's pill, in the notch's black, and while it is near the notch (the capsule opening or closing with
    /// it out) a neck between the two with soft inner corners, so they flow together and apart as one shape.
    unsafe fn draw_notice_shape(&self, (cx, y, pw, ph, r, alpha, _): (f32, f32, f32, f32, f32, f32, f32)) {
        let Some(black) = self.brush(BLACK, alpha) else { return };
        self.rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(cx - pw / 2.0, y, cx + pw / 2.0, y + ph), radiusX: r, radiusY: r }, &black);
        let (w, h, nr, top, ..) = self.shape();
        let (nb, mid) = (top + h, center(self.align, w));
        let gap = y - nb;
        let j = smooth((smooth(self.edge.x.clamp(0.0, 1.0)) - 0.15) / 0.55);
        if j < 0.02 || gap < 0.5 { return; }
        let (flat_n, flat_p) = ((w / 2.0 - nr).max(0.0), (pw / 2.0 - r).max(0.0));
        let nw = flat_n.min(flat_p) * 2.0 * 0.85 * j;
        if nw < 1.0 { return; }
        let f1 = (10.0 * j).min(gap / 2.0).min(flat_n - nw / 2.0).max(0.0);
        let f2 = (10.0 * j).min(gap / 2.0).min(flat_p - nw / 2.0).max(0.0);
        let (l, rr) = (mid - nw / 2.0, mid + nw / 2.0);
        let Ok(g) = self.d2d.CreatePathGeometry() else { return };
        let Ok(s) = g.Open() else { return };
        let q = |c: (f32, f32), e: (f32, f32)| D2D1_QUADRATIC_BEZIER_SEGMENT { point1: pt(c.0, c.1), point2: pt(e.0, e.1) };
        s.BeginFigure(pt(l - f1, nb - 0.5), D2D1_FIGURE_BEGIN_FILLED);
        s.AddLine(pt(rr + f1, nb - 0.5));
        s.AddQuadraticBezier(&q((rr, nb), (rr, nb + f1)));
        s.AddLine(pt(rr, y - f2));
        s.AddQuadraticBezier(&q((rr, y), (rr + f2, y + 0.5)));
        s.AddLine(pt(l - f2, y + 0.5));
        s.AddQuadraticBezier(&q((l, y), (l, y - f2)));
        s.AddLine(pt(l, nb + f1));
        s.AddQuadraticBezier(&q((l, nb), (l - f1, nb - 0.5)));
        s.EndFigure(D2D1_FIGURE_END_CLOSED);
        if s.Close().is_ok() { self.rt.FillGeometry(&g, &black, None); }
    }
    unsafe fn notice_text(&self, s: &str, size: f32, weight: DWRITE_FONT_WEIGHT, max: f32) -> Option<IDWriteTextLayout> {
        let f = self.dw.CreateTextFormat(windows::core::PCWSTR(self.family.as_ptr()), self.fonts.as_ref(), weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us")).ok()?;
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&f) {
            let _ = f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign);
        }
        self.dw.CreateTextLayout(&crate::draw::wide(s), &f, max, 40.0).ok()
    }
    /// Text that wraps within `w` and holds at most `lines` lines, ending in an ellipsis when it is longer.
    unsafe fn notice_lines(&self, s: &str, size: f32, w: f32, lines: u32) -> Option<IDWriteTextLayout> {
        let f = self.dw.CreateTextFormat(windows::core::PCWSTR(self.family.as_ptr()), self.fonts.as_ref(), DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us")).ok()?;
        let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
        let mut text: Vec<char> = s.chars().collect();
        loop {
            let l = self.dw.CreateTextLayout(&crate::draw::wide(&text.iter().collect::<String>()), &f, w, 200.0).ok()?;
            let mut m = DWRITE_TEXT_METRICS::default();
            if l.GetMetrics(&mut m).is_err() || m.lineCount <= lines || text.len() < 6 { return Some(l); }
            if text.last() == Some(&'…') { text.pop(); }
            let keep = (text.len() as f32 * 0.88) as usize;
            text.truncate(keep.min(text.len() - 1));
            while text.last().map_or(false, |c| c.is_whitespace()) { text.pop(); }
            text.push('…');
        }
    }
    /// Badge, kind, title, detail and the countdown ring; they come in once the pill has nearly grown, and leave first.
    unsafe fn draw_notice_content(&self, (cx, y, pw, ph, _, alpha, n): (f32, f32, f32, f32, f32, f32, f32)) {
        let Some(note) = self.notice.as_ref() else { return };
        let a = smooth((n - 0.72) / 0.28) * alpha;
        // After a swap the new words come in as the card settles into its size.
        let a = if note.swapped { a * smooth((note.born.elapsed().as_secs_f32() - 0.08) / 0.24) } else { a };
        if a < 0.01 { return; }
        let rt = &self.rt;
        let mut base = Matrix3x2::default();
        rt.GetTransform(&mut base);
        rt.PushAxisAlignedClip(&rect(cx - pw / 2.0, y, cx + pw / 2.0, y + ph), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        // A notice that takes the place of another fades its words in; the pill stays.
        let a = a * smooth((note.born.elapsed().as_secs_f32() / 0.3).min(1.0));
        let size = self.notice_size();
        // LocalSend's question and transfer keep their words in the top row; the buttons or the bar sit under them.
        let (left, cy) = (cx - size.0 / 2.0, if receive_rows(note) { y + NOTICE.1 / 2.0 } else { y + ph / 2.0 });
        let chat = note.kind == CHAT;
        // A 24 x 24 outline centred on (mx, my), `d` px wide, drawn as a line or filled; a disc; a tile.
        let place = |mx: f32, my: f32, d: f32| { let k = d / 24.0; rt.SetTransform(&mul(mul(translate(-12.0, -12.0), scale(k, k)), mul(translate(mx, my), base))); };
        let line = |g: &Option<ID2D1PathGeometry>, mx: f32, my: f32, d: f32, hex: u32, w: f32| { if let (Some(g), Some(b)) = (g, self.brush(hex, a)) { place(mx, my, d); rt.DrawGeometry(g, &b, w, &self.stroke); rt.SetTransform(&base); } };
        let logo = |g: &Option<ID2D1PathGeometry>, mx: f32, my: f32, d: f32, hex: u32| { if let (Some(g), Some(b)) = (g, self.brush(hex, a)) { place(mx, my, d); rt.FillGeometry(g, &b, None); rt.SetTransform(&base); } };
        let disc = |mx: f32, my: f32, r: f32, hex: u32, k: f32| { if let Some(b) = self.brush(hex, a * k) { rt.FillEllipse(&D2D1_ELLIPSE { point: pt(mx, my), radiusX: r, radiusY: r }, &b); } };
        let tile = |mx: f32, my: f32, d: f32, r: f32, hex: u32| { if let Some(b) = self.brush(hex, a) { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(mx - d / 2.0, my - d / 2.0, mx + d / 2.0, my + d / 2.0), radiusX: r, radiusY: r }, &b); } };
        // The time the notice stays: a green ring that runs down, from the top clockwise.
        let countdown = |mx: f32, my: f32, r: f32| {
            let base = (1.0 - note.at.elapsed().as_secs_f32() / (note.hold_ms as f32 / 1000.0)).clamp(0.0, 1.0);
            let frac = lerp(note.refill, base, smooth((note.merged.elapsed().as_secs_f32() / 0.45).min(1.0)));
            let dot = D2D1_ELLIPSE { point: pt(mx, my), radiusX: r, radiusY: r };
            if let Some(b) = self.brush(0xffffff, 0.12 * a) { rt.DrawEllipse(&dot, &b, 3.0, None); }
            let Some(b) = self.brush(0x4cd37b, a) else { return };
            if frac > 0.999 { rt.DrawEllipse(&dot, &b, 3.0, None); return; }
            if frac <= 0.004 { return; }
            let ang = -std::f32::consts::FRAC_PI_2 + frac * std::f32::consts::TAU;
            let Ok(g) = self.d2d.CreatePathGeometry() else { return };
            let Ok(s) = g.Open() else { return };
            s.BeginFigure(pt(mx, my - r), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddArc(&D2D1_ARC_SEGMENT { point: pt(mx + r * ang.cos(), my + r * ang.sin()), size: D2D_SIZE_F { width: r, height: r }, rotationAngle: 0.0, sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE, arcSize: if frac > 0.5 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL } });
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            if s.Close().is_ok() { rt.DrawGeometry(&g, &b, 3.0, &self.stroke); }
        };
        let app = if chat { mark_index(&note.label) } else { None };
        let (mut value, mut detail) = (None, note.detail.clone());
        if !chat && (note.kind == 1 || note.kind == 4) { let (d, v) = split_clock(&note.detail, if note.kind == 4 { "Alarm" } else { "Event" }); detail = d; value = v; }
        let busy = (note.kind == RECEIVE && note.target == "ls-busy").then(crate::lan_receive::status);
        if let Some((l, f)) = &busy { if !l.is_empty() { detail = l.clone(); } value = Some(format!("{}%", (f * 100.0).round() as i32)); }
        let right = left + size.0 - 24.0;
        if chat {
            // Compact, like a phone's banner: the contact's or group's picture (a contact or group glyph when there is
            // none) with the app's logo as a badge, or, for an app that is no messenger, its logo on a tile; beside it the
            // name, the message under it, and "now" (or Reply, for a chat Sora can answer) in the corner. No app line.
            let (d, mx, my) = (CHAT_PIC, left + CHAT_PAD + CHAT_PIC / 2.0, y + CHAT_PAD + CHAT_PIC / 2.0);
            let person = app.map_or(true, |i| MARKS[i].person);
            if let Some(i) = app.filter(|_| !person) {
                tile(mx, my, d, 14.0, MARKS[i].color);
                logo(&self.app_logos[i], mx, my, 25.0, MARKS[i].fg);
            } else {
                let picture = self.notice_picture.as_ref().and_then(|bmp| {
                    let px = bmp.GetPixelSize();
                    let props = D2D1_BITMAP_BRUSH_PROPERTIES { extendModeX: D2D1_EXTEND_MODE_CLAMP, extendModeY: D2D1_EXTEND_MODE_CLAMP, interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR };
                    let brush = rt.CreateBitmapBrush(bmp, Some(&props), None).ok()?;
                    let s = d / px.width.min(px.height).max(1) as f32;
                    brush.SetTransform(&mul(scale(s, s), translate(mx - px.width as f32 * s / 2.0, my - px.height as f32 * s / 2.0)));
                    brush.SetOpacity(a);
                    Some(brush)
                });
                if let Some(b) = picture { rt.FillEllipse(&D2D1_ELLIPSE { point: pt(mx, my), radiusX: d / 2.0, radiusY: d / 2.0 }, &b); }
                else {
                    disc(mx, my, d / 2.0, 0x23252b, 1.0);
                    line(&self.glyphs[looks_like_group(&note.detail) as usize], mx, my, 25.0, 0xb4b4bb, 1.5);
                }
                if let Some(i) = app {
                    let (bx, by) = (mx + d / 2.0 - 4.0, my + d / 2.0 - 4.0);
                    disc(bx, by, 10.5, BLACK, 1.0);
                    tile(bx, by, 16.0, 5.0, MARKS[i].color);
                    logo(&self.app_logos[i], bx, by, 9.5, MARKS[i].fg);
                }
            }
            let x = left + CHAT_TEXT_X;
            let edge = left + size.0 - CHAT_PAD;
            // Top right: "now"; while the card is hovered, Reply (blue) for a chat Sora can answer; while answering, how the
            // reply is doing (nothing, Sending…, Sent, or why it was not sent).
            let answering = self.reply.as_ref().filter(|r| r.key == note.target);
            let reply = self.notice_hot && crate::whatsapp::is_reply_target(&note.target);
            let (corner, corner_size, corner_weight, corner_hex): (&str, f32, DWRITE_FONT_WEIGHT, u32) = match answering {
                Some(r) if r.sent_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1600)) && r.text.is_empty() => ("Sent", 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 0x4cd37b),
                Some(r) if r.sending => ("Sending…", 12.0, DWRITE_FONT_WEIGHT_NORMAL, 0x636368),
                Some(r) if !r.error.is_empty() => (r.error.as_str(), 12.0, DWRITE_FONT_WEIGHT_NORMAL, 0xff6961),
                Some(_) => ("", 12.0, DWRITE_FONT_WEIGHT_NORMAL, 0x636368),
                None if reply => ("Reply", 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 0x4f9dff),
                // Quieter than the message, so the words lead.
                None => ("now", 12.0, DWRITE_FONT_WEIGHT_NORMAL, 0x636368),
            };
            let mut corner_w = 0.0;
            if let (false, Some(l), Some(b)) = (corner.is_empty(), self.notice_text(corner, corner_size, corner_weight, if answering.is_some() { 170.0 } else { 60.0 }), self.brush(corner_hex, a)) {
                let mut m = DWRITE_TEXT_METRICS::default();
                corner_w = if l.GetMetrics(&mut m).is_ok() { m.width } else { 36.0 };
                rt.DrawTextLayout(pt(edge - corner_w, y + CHAT_NAME_Y + 2.0), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
            // The name in white; for a group, the group's name in grey after it.
            let (name, group, lines) = chat_words(&note.title, &note.detail);
            let room = (edge - corner_w - 14.0 - x).max(40.0);
            let mut name_w = 0.0;
            if let (Some(l), Some(b)) = (self.notice_text(&name, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, if group.is_some() { room * 0.62 } else { room }), self.brush(0xf4f4f6, a)) {
                let mut m = DWRITE_TEXT_METRICS::default();
                name_w = if l.GetMetrics(&mut m).is_ok() { m.width } else { 0.0 };
                rt.DrawTextLayout(pt(x, y + CHAT_NAME_Y), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
            }
            if let Some(g) = &group {
                if let (Some(l), Some(b)) = (self.notice_text(&format!(" – {g}"), 15.0, DWRITE_FONT_WEIGHT_NORMAL, (room - name_w).max(20.0)), self.brush(0x8e8e93, a)) { rt.DrawTextLayout(pt(x + name_w, y + CHAT_NAME_Y), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
            }
            // The message: grey words under the name, up to two lines; several messages, one line each (up to three).
            if lines.len() > 1 {
                let k = smooth((note.merged.elapsed().as_secs_f32() / 0.4).min(1.0));
                // A fourth message: everything moves up a row, the first leaves at the top as the newest comes in below.
                let scroll = !note.dropped.is_empty() && k < 1.0;
                let shift = if scroll { (1.0 - k) * CHAT_ROW } else { 0.0 };
                let strip = |p: &str| -> String { match (group.as_ref(), group_line(p)) { (Some(_), Some((w, m))) if w == name => m.to_string(), _ => p.to_string() } };
                rt.PushAxisAlignedClip(&rect(x - 2.0, y + CHAT_MSG_Y - 1.0, edge + 2.0, y + CHAT_MSG_Y + lines.len().max(CHAT_MERGE) as f32 * CHAT_ROW + 1.0), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                // One row: someone else's words from the left in grey, one's own from the right in light blue.
                let row = |p: &str, top: f32, hex: u32, alpha: f32| {
                    let mine = p.starts_with(MINE);
                    let words = p.trim_start_matches(MINE);
                    let Some(l) = self.notice_text(words, 14.0, DWRITE_FONT_WEIGHT_NORMAL, if mine { (edge - x) * 0.82 } else { edge - x }) else { return };
                    let Some(b) = self.brush(if mine { 0xd2e3ff } else { hex }, alpha) else { return };
                    let w = if mine { let mut m = DWRITE_TEXT_METRICS::default(); if l.GetMetrics(&mut m).is_ok() { m.width } else { 0.0 } } else { 0.0 };
                    rt.DrawTextLayout(pt(if mine { edge - w } else { x }, top), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                };
                if scroll { row(&strip(&note.dropped), y + CHAT_MSG_Y - CHAT_ROW + shift, 0x7c7c82, a * (1.0 - k)); }
                for (i, p) in lines.iter().enumerate() {
                    let newest = i + 1 == lines.len();
                    // The newest line comes in and fades up (rising a little, or with the scroll); the earlier ones are there.
                    let (kk, rise) = if newest { (k, if scroll { 0.0 } else { (1.0 - k) * 9.0 }) } else { (1.0, 0.0) };
                    row(p, y + CHAT_MSG_Y + i as f32 * CHAT_ROW + shift + rise, if newest { 0xb0b0b6 } else { 0x7c7c82 }, a * kk);
                }
                rt.PopAxisAlignedClip();
            } else if let (Some(l), Some(b)) = (self.notice_lines(&lines[0], 14.0, edge - x, if answering.is_some() { CHAT_LINES + 1 } else { CHAT_LINES }), self.brush(0xb0b0b6, a)) { rt.DrawTextLayout(pt(x, y + CHAT_MSG_Y), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
            // The reply field across the card's foot (no outline), with a round send button beside it. It comes in once the
            // card has started to grow: it fades up from 10 px lower and widens from 86 %, the button following a moment later.
            if let Some(r) = answering {
                let t = r.opened.elapsed().as_secs_f32();
                let (k, kb) = (smooth((t / REPLY_IN).min(1.0)), smooth(((t - 0.07) / REPLY_IN).clamp(0.0, 1.0)));
                let lift = (1.0 - k) * 10.0;
                let fb = y + size.1 - CHAT_PAD + lift;
                let ft = fb - REPLY_FIELD;
                let (fl, send_l) = (left + CHAT_PAD, edge - REPLY_FIELD);
                let fr = fl + (send_l - REPLY_SEND_GAP - fl) * (0.86 + 0.14 * k);
                let field = D2D1_ROUNDED_RECT { rect: rect(fl, ft, fr, fb), radiusX: REPLY_FIELD / 2.0, radiusY: REPLY_FIELD / 2.0 };
                if let Some(bg) = self.brush(0x1c1c1f, a * k) { rt.FillRoundedRectangle(&field, &bg); }
                // Send: blue once there are words, grey before; while sending it dims.
                let words: String = r.text.iter().collect();
                let ready = !words.trim().is_empty() && !r.sending;
                let (bc, br) = (send_l + REPLY_FIELD / 2.0, REPLY_FIELD / 2.0 * (0.6 + 0.4 * kb));
                let by = (ft + fb) / 2.0 + (1.0 - kb) * 4.0;
                disc(bc, by, br, if ready { 0x4f9dff } else { 0x2a2a2e }, kb * if r.sending { 0.6 } else { 1.0 });
                line(&self.glyphs[3], bc, by, 17.0 * (0.6 + 0.4 * kb), if ready { 0xffffff } else { 0x8e8e93 }, 2.0);
                let (tx, room) = (fl + 15.0, fr - fl - 30.0);
                let ty = ft + (REPLY_FIELD - 19.0) / 2.0;
                rt.PushAxisAlignedClip(&rect(tx - 2.0, ft, tx + room + 2.0, fb), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                if words.is_empty() {
                    if let (Some(l), Some(b)) = (self.notice_text(&format!("Reply to {}", r.name), 13.5, DWRITE_FONT_WEIGHT_NORMAL, room), self.brush(0x636368, a * k)) { rt.DrawTextLayout(pt(tx, ty), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
                }
                let layout = self.notice_text(if words.is_empty() { " " } else { &words }, 13.5, DWRITE_FONT_WEIGHT_NORMAL, 100_000.0);
                // The caret's place in the words (UTF-16), and the words slid left when they run past the field.
                let caret_x = layout.as_ref().map_or(0.0, |l| {
                    let units = r.text[..r.caret].iter().map(|c| c.len_utf16()).sum::<usize>() as u32;
                    let (mut px, mut py, mut m) = (0.0f32, 0.0f32, DWRITE_HIT_TEST_METRICS::default());
                    if words.is_empty() || l.HitTestTextPosition(units, BOOL(0), &mut px, &mut py, &mut m).is_err() { 0.0 } else { px }
                });
                let shift = (caret_x - room).max(0.0);
                if let (false, Some(l), Some(b)) = (words.is_empty(), layout.as_ref(), self.brush(0xf4f4f6, a * k)) { rt.DrawTextLayout(pt(tx - shift, ty), l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
                if k > 0.99 && !r.sending && (r.blink.elapsed().as_millis() / 530) % 2 == 0 {
                    if let Some(b) = self.brush(0x4f9dff, a) { rt.FillRectangle(&rect(tx + caret_x - shift, ty + 1.0, tx + caret_x - shift + 1.5, ty + 18.0), &b); }
                }
                rt.PopAxisAlignedClip();
            }
        } else {
            // A reminder: a big bare icon, the words (small grey line over a bold title), and on the right the time,
            // or a ring that runs down with the time the reminder stays.
            // An agent's notice: the agent's own mark on its tile (Claude, OpenAI for Codex, Gemini...), as a chat card's;
            // an agent without one (OpenCode, Cursor...) its initial on a grey tile. Every other reminder: its bare icon.
            if note.kind == 5 {
                match mark_index(&note.title) {
                    Some(i) => { tile(left + 36.0, cy, 40.0, 12.0, MARKS[i].color); logo(&self.app_logos[i], left + 36.0, cy, 22.0, MARKS[i].fg); }
                    None => {
                        tile(left + 36.0, cy, 40.0, 12.0, 0x2c2c30);
                        let initial: String = note.title.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
                        if let (Some(l), Some(b)) = (self.notice_text(&initial, 18.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 40.0), self.brush(0xf4f4f6, a)) {
                            let mut m = DWRITE_TEXT_METRICS::default(); let _ = l.GetMetrics(&mut m);
                            rt.DrawTextLayout(pt(left + 36.0 - m.width / 2.0, cy - m.height / 2.0), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        }
                    }
                }
            } else { line(self.notice_icons.get(note.kind).unwrap_or(&None), left + 36.0, cy, 38.0, 0xf4f4f6, 1.35); }
            let x = left + 66.0;
            let mut right_w = 0.0;
            if let Some(v) = value.as_deref() {
                if let (Some(l), Some(b)) = (self.notice_text(v, 22.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, 90.0), self.brush(0xf4f4f6, a)) {
                    let mut m = DWRITE_TEXT_METRICS::default();
                    right_w = if l.GetMetrics(&mut m).is_ok() { m.width } else { 56.0 };
                    rt.DrawTextLayout(pt(right - right_w, cy - 15.0), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
            } else if note.kind == 5 && note.target.starts_with("agent-ask:") {
                // An agent asks for permission: Deny and Allow, right there.
                if let Some((deny, allow)) = self.agent_buttons() {
                    let dy = cy - (self.notice_pill().map_or(cy, |p| p.1 + p.3 / 2.0));
                    for ((l, t, r, b), text, bg, fg) in [(deny, "Deny", 0x2c2c2e, 0xf4f4f6), (allow, "Allow", 0xf4f4f6, 0x050506)] {
                        if let Some(br) = self.brush(bg, a) { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(l, t + dy, r, b + dy), radiusX: 15.0, radiusY: 15.0 }, &br); }
                        if let (Some(lay), Some(br)) = (self.notice_text(text, 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, r - l), self.brush(fg, a)) {
                            let mut m = DWRITE_TEXT_METRICS::default();
                            let (w, h) = if lay.GetMetrics(&mut m).is_ok() { (m.width, m.height) } else { (30.0, 16.0) };
                            rt.DrawTextLayout(pt((l + r - w) / 2.0, (t + b - h) / 2.0 + dy), &lay, &br, D2D1_DRAW_TEXT_OPTIONS_NONE);
                        }
                    }
                }
                right_w = 58.0 + 8.0 + 56.0;
            } else if note.kind == 3 {
                // A glass counted without opening the notch: the ring runs down the hold time, a plus sits in it.
                // Under the pointer it lights up, pressed it sinks in; once counted it fills green with a tick.
                let c = (right - 13.0, cy);
                if let Some(t) = self.drank_at {
                    let f = smooth((t.elapsed().as_secs_f32() / 0.22).min(1.0));
                    disc(c.0, c.1, 14.5 * (0.8 + 0.2 * f), 0x4cd37b, f);
                    line(&self.glyphs[2], c.0, c.1, 26.0 * (0.7 + 0.3 * f), 0x050506, 2.4);
                } else {
                    let k = 1.0 - 0.18 * self.press.x;
                    if self.water_hot || self.press.x > 0.01 { disc(c.0, c.1, 15.5 * k, 0x4cd37b, 0.14 + 0.2 * self.press.x); }
                    countdown(c.0, c.1, 13.0 * k);
                    if let Some(b) = self.brush(0x4cd37b, a) {
                        let s = 5.0 * k;
                        rt.DrawLine(pt(c.0 - s, c.1), pt(c.0 + s, c.1), &b, 1.8, &self.stroke);
                        rt.DrawLine(pt(c.0, c.1 - s), pt(c.0, c.1 + s), &b, 1.8, &self.stroke);
                    }
                }
                right_w = 29.0;
            } else {
                countdown(right - 13.0, cy, 13.0);
                right_w = 29.0;
            }
            let max = size.0 - 66.0 - 24.0 - right_w - 12.0;
            if let (Some(l), Some(b)) = (self.notice_text(&detail, 12.0, DWRITE_FONT_WEIGHT_NORMAL, max), self.brush(0x8e8e93, a)) { rt.DrawTextLayout(pt(x, cy - 23.0), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
            if let (Some(l), Some(b)) = (self.notice_text(&note.title, 17.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, max), self.brush(0xf4f4f6, a)) { rt.DrawTextLayout(pt(x, cy - 6.0), &l, &b, D2D1_DRAW_TEXT_OPTIONS_NONE); }
            // LocalSend: Deny and Accept across the foot (Accept white, as on the preview), or the transfer's bar.
            if let Some((_, deny, accept)) = self.receive_buttons() {
                let dy = y - self.notice_pill().map_or(y, |p| p.1);
                for ((l, t, r, bt), text, bg, fg) in [(deny, "Deny", 0x2c2c2e, 0xe9e9ee), (accept, "Accept", 0xffffff, 0x0b0b0c)] {
                    if let Some(br) = self.brush(bg, a) { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(l, t + dy, r, bt + dy), radiusX: 15.0, radiusY: 15.0 }, &br); }
                    if let (Some(lay), Some(br)) = (self.notice_text(text, 14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, r - l), self.brush(fg, a)) {
                        let mut m = DWRITE_TEXT_METRICS::default();
                        let (w, h) = if lay.GetMetrics(&mut m).is_ok() { (m.width, m.height) } else { (40.0, 18.0) };
                        rt.DrawTextLayout(pt((l + r - w) / 2.0, (t + bt - h) / 2.0 + dy), &lay, &br, D2D1_DRAW_TEXT_OPTIONS_NONE);
                    }
                }
            }
            if let Some((_, f)) = busy {
                let (l, r, t) = (left + 20.0, left + size.0 - 20.0, y + NOTICE.1 - 1.0);
                if let Some(br) = self.brush(0xffffff, 0.12 * a) { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(l, t, r, t + 4.0), radiusX: 2.0, radiusY: 2.0 }, &br); }
                if let Some(br) = self.brush(0x4cd37b, a) { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(l, t, l + (r - l) * f.clamp(0.0, 1.0), t + 4.0), radiusX: 2.0, radiusY: 2.0 }, &br); }
            }
        }
        rt.PopAxisAlignedClip();
    }
    /// The face: ui/mascot.js drew it in a 160x70 SVG shown at 154x68, centred on the mini notch.
    unsafe fn draw_mascot(&self, base: Matrix3x2, alpha: f32, grow: f32, dy: f32, mouth: f32) {
        let (pop, pop_y, pop_a) = self.mascot.pop();
        let pop = pop * grow;
        let alpha = alpha * pop_a;
        if alpha < 0.01 { return; }
        let k = 154.0 / 160.0;
        let svg = mul(mul(mul(mul(translate(-80.0, -35.0), scale(pop, pop)), translate(80.0, 35.0 + pop_y)), scale(k, k)), mul(translate(anchor(self.align) - 77.0, dy - 2.0 + (68.0 - 70.0 * k) / 2.0), base));
        let f = self.mascot.face;
        let rig = mul(mul(mul(mul(translate(-80.0, -34.0), scale(f.sx, f.sy)), rotate(f.roll)), translate(80.0 + f.x, 34.0 + f.y)), svg);
        let Some(white) = self.brush(0xffffff, alpha) else { return };
        let lid = 0.24 + 0.76 * self.lid;
        for (cx, open, tilt, wink) in [(42.0, f.open + f.asym, f.roll * 0.07, f.wink), (118.0, f.open - f.asym, -f.roll * 0.07, 0.0)] {
            let p = eye_points(cx, 31.0, open, tilt, wink);
            let Ok(g) = self.d2d.CreatePathGeometry() else { continue };
            let Ok(s) = g.Open() else { continue };
            s.BeginFigure(pt(p[0].0, p[0].1), D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(p[1].0, p[1].1), point2: pt(p[2].0, p[2].1), point3: pt(p[3].0, p[3].1) });
            s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(p[4].0, p[4].1), point2: pt(p[5].0, p[5].1), point3: pt(p[6].0, p[6].1) });
            s.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = s.Close();
            self.rt.SetTransform(&mul(mul(mul(translate(0.0, -31.0), scale(1.0, lid)), translate(0.0, 31.0)), rig));
            let w = wink.clamp(0.0, 1.0);
            self.rt.DrawGeometry(&g, &white, (9.0 + 5.0 * open.clamp(0.0, 1.0)) * (1.0 - w) + 9.0 * w, &self.stroke);
        }
        let half = 16.0 - 7.0 * f.hum;
        if let Ok(g) = self.d2d.CreatePathGeometry() {
            if let Ok(s) = g.Open() {
                s.BeginFigure(pt(80.0 - half, 39.0), D2D1_FIGURE_BEGIN_HOLLOW);
                s.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(80.0 - half * 0.6, 39.0 + f.smile), point2: pt(80.0 + half * 0.6, 39.0 + f.smile), point3: pt(80.0 + half, 39.0) });
                s.EndFigure(D2D1_FIGURE_END_OPEN);
                let _ = s.Close();
                self.rt.SetTransform(&rig);
                if let Some(m) = self.brush(0xffffff, alpha * mouth) { self.rt.DrawGeometry(&g, &m, 3.8 + f.hum * 1.2, &self.stroke); }
            }
        }
        // Music notes float up beside the face while it listens.
        if let Some(note) = &self.note {
            let m = &self.mascot;
            for n in &m.notes {
                let age = m.clock - n.born;
                let u = age / 4.6;
                let a = (age / 0.5).min(1.0).min((4.6 - age) / 1.2).max(0.0) * 0.7 * m.note_alpha * alpha;
                let Some(b) = self.brush(0xececee, a) else { continue };
                let t = mul(mul(scale(0.65, 0.65), rotate(n.side * (12.0 - 24.0 * u))), translate(80.0 + n.side * (86.0 + (u * 4.0).sin() * 3.0), 62.0 - u * 53.0));
                self.rt.SetTransform(&mul(t, svg));
                self.rt.DrawGeometry(note, &b, 2.6, &self.stroke);
            }
        }
        self.rt.SetTransform(&base);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_card_keeps_three_of_theirs_and_three_of_ones_own() {
        let mine = |s: &str| format!("{}{s}", super::MINE);
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        // Replying never pushes out their three.
        let (k, d) = super::keep_recent([v(&["a", "b", "c"]), vec![mine("x")]].concat());
        assert_eq!(k.len(), 4); assert!(d.is_empty());
        // Their fourth: their first goes, sliding out at the top.
        let (k, d) = super::keep_recent([v(&["a", "b", "c"]), vec![mine("x")], v(&["d"])].concat());
        assert_eq!(k, [v(&["b", "c"]), vec![mine("x")], v(&["d"])].concat()); assert_eq!(d, "a");
        // One's own fourth: one's oldest goes; it was not the top line, so nothing slides.
        let (k, d) = super::keep_recent([v(&["a"]), vec![mine("1"), mine("2"), mine("3"), mine("4")]].concat());
        assert_eq!(k, [v(&["a"]), vec![mine("2"), mine("3"), mine("4")]].concat()); assert!(d.is_empty());
    }
    use super::*;
    #[test]
    fn notices_find_their_app_logo_and_kind() {
        let name = |s: &str| mark_index(s).map(|i| MARKS[i].name);
        assert_eq!(name("WhatsApp"), Some("WhatsApp"));
        assert_eq!(name("web.whatsapp.com"), Some("WhatsApp"), "a browser notice carries the site");
        assert_eq!(name("Microsoft Teams"), Some("Teams"));
        assert_eq!(name("mail.google.com"), Some("Gmail"));
        assert_eq!(name("x.com"), Some("X"));
        assert_eq!(name("Microsoft Edge"), None);
        assert_eq!(name("Online Store"), None, "whole words only");
        assert!(MARKS.iter().all(|m| !m.path.is_empty() && m.path.starts_with('M')));
        assert!(MARKS.iter().find(|m| m.name == "WhatsApp").unwrap().person && !MARKS.iter().find(|m| m.name == "Instagram").unwrap().person);
    }
    #[test]
    fn a_group_notice_and_a_reminder_time_are_told_apart() {
        assert!(looks_like_group("Sam: on my way") && !looks_like_group("On my way") && !looks_like_group(": nothing"));
        assert_eq!(split_clock("Starts at 09:30", "Event"), ("Event".to_string(), Some("09:30".to_string())));
        assert_eq!(split_clock("09:30 · Bring the report", "Event"), ("Bring the report".to_string(), Some("09:30".to_string())));
        assert_eq!(split_clock("It's 07:00", "Alarm"), ("Alarm".to_string(), Some("07:00".to_string())));
        assert_eq!(split_clock("Time for a break.", "x"), ("Time for a break.".to_string(), None));
    }
    #[test]
    fn eyes_close_into_a_dot_open_upward_and_wink() {
        let open = eye_points(42.0, 31.0, 1.0, 0.0, 0.0);
        assert!((open[0].0 - 42.0).abs() < 0.01 && open[0].1 < open[6].1, "fully open: a vertical stroke");
        let shut = eye_points(42.0, 31.0, 0.0, 0.0, 0.0);
        assert!(shut[0].0 < 33.0 && shut[6].0 > 51.0 && shut[3].1 > 31.0, "closed: a smile curve");
        let wink = eye_points(42.0, 31.0, 1.0, 0.0, 1.0);
        assert_eq!((wink[3].0, wink[0].0, wink[6].0), (51.0, 33.0, 33.0), "winking: a > chevron");
    }
    #[test]
    fn the_rig_settles_listens_and_greets() {
        let mut m = Mascot::new();
        m.hidden = false;
        for _ in 0..300 { m.step(1.0 / 60.0, false); }
        assert!(m.face.open > 0.9 && m.face.smile > 7.0, "awake at rest");
        for _ in 0..240 { m.step(1.0 / 60.0, true); }
        assert!(m.face.open < 0.2, "eyes closed while listening to music");
        assert!(m.note_alpha > 0.9 && !m.notes.is_empty(), "notes float up");
        m.greet();
        let mut peak: f32 = 0.0;
        for _ in 0..90 { m.step(1.0 / 60.0, true); peak = peak.max(m.face.wink); }
        assert!(peak > 0.9 && m.face.open > 0.8, "the greeting winks with the music playing (calm wins)");
        for _ in 0..240 { m.step(1.0 / 60.0, false); }
        assert!(m.face.wink < 0.05, "and the wink goes away");
        assert_eq!(m.pop(), (1.0, 0.0, 1.0));
    }
    #[test]
    fn the_notch_hangs_from_the_left_centre_or_right_and_grows_inward() {
        let screen = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
        for align in [Align::Left, Align::Center, Align::Right] {
            let (fit, origin, size) = layout(screen, 1.0, align, 0.85);
            assert_eq!(fit, 0.85);
            assert!(origin.0 >= 0 && origin.0 + size.0 <= 1920 && origin.1 == 0, "{align:?} stays on the screen");
            // The panel fills the canvas whatever the position, the mini notch does not move while it grows.
            assert_eq!(center(align, PANEL.0), MID);
            let (small, big) = (center(align, COMPACT.0), center(align, 500.0));
            assert_eq!(small, anchor(align));
            assert!((small - COMPACT.0 / 2.0 - (big - 250.0)).abs() < 0.01 || align == Align::Center || align == Align::Right, "left edge fixed");
            assert!(((small + COMPACT.0 / 2.0) - (big + 250.0)).abs() < 0.01 || align != Align::Right, "right edge fixed");
        }
        let (_, left, size) = layout(screen, 1.0, Align::Left, 0.85);
        let (_, right, _) = layout(screen, 1.0, Align::Right, 0.85);
        assert_eq!((left.0, right.0 + size.0), (20, 1900));
        assert_eq!(layout(screen, 1.0, Align::Center, 0.85).1 .0, (1920 - size.0) / 2);
    }
    #[test]
    fn a_second_display_and_a_narrow_one_are_respected() {
        let second = RECT { left: 1920, top: 0, right: 1920 + 2560, bottom: 1440 };
        let (fit, origin, size) = layout(second, 1.5, Align::Right, 0.64);
        assert_eq!(fit, 0.64);
        assert_eq!(origin.0 + size.0, 1920 + 2560 - 30, "20 dip gap at 150%");
        let narrow = RECT { left: 0, top: 0, right: 600, bottom: 800 };
        let (fit, origin, size) = layout(narrow, 1.0, Align::Left, 0.85);
        assert!(fit < 0.85 && origin.0 >= 0 && origin.0 + size.0 <= 600, "shrinks to fit instead of running off the screen");
        assert!((size_scale(0) - 0.64).abs() < 1e-6 && (size_scale(100) - 0.85).abs() < 1e-6 && size_scale(500) == size_scale(100));
        assert!(size_scale(0) < size_scale(50) && size_scale(50) < size_scale(100), "in between is in between");
    }
    #[test]
    fn the_tray_opened_from_the_capsule_hangs_from_the_top_with_its_wings() {
        // At rest: the capsule floats, round, without wings; the mini notch and the panel hang from the top with them.
        let (w, _, _, top, ear, _) = notch_shape(0.0, 0.0, 1.0, 0.0);
        assert!((w - CAPSULE.0).abs() < 0.01 && top > 0.0 && ear == 0.0);
        let (w, _, _, top, ear, tr) = notch_shape(0.0, 0.0, 0.0, 0.0);
        assert!((w - COMPACT.0).abs() < 0.01 && top == 0.0 && ear == EAR && tr == 0.0);
        // Capsule straight to the Tray: once the panel is under way it is attached with full wings, whatever the capsule spring says.
        for edge in [1.0, 0.8, 0.5, 0.2] {
            for open in [0.3, 0.5, 0.8, 1.0, 1.05] {
                let (w, h, r, top, ear, tr) = notch_shape(open, 0.0, edge, 0.0);
                assert!(top == 0.0 && ear == EAR && tr == 0.0, "edge {edge} open {open}");
                assert!(w > CAPSULE.0 && h > CAPSULE.1 && r > 0.0 && r <= h / 2.0 && ear + r <= h, "edge {edge} open {open}: {w}x{h} r{r}");
            }
        }
        // Every frame of that growth is a drawable outline: the wing and the bottom corner fit beside each other.
        for i in 0..=100 { let (_, h, r, _, ear, tr) = notch_shape(i as f32 / 100.0, 0.0, 1.0, 0.0); assert!(r <= h / 2.0 + 1e-4 && (ear + r <= h + 1e-4 || ear < 0.05) && tr >= 0.0, "open {i}%"); }
        // The full panel is the same from every start.
        assert_eq!(notch_shape(1.0, 0.0, 0.6, 0.0), notch_shape(1.0, 0.0, 0.0, 0.0));
    }
    #[test]
    fn the_chime_is_a_short_wav() {
        let wav = chime_wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert!(wav.len() > 44 + 44_100 * 2 && wav.len() < 44 + 44_100 * 3);
        assert!(wav[44..].chunks_exact(2).any(|s| i16::from_le_bytes([s[0], s[1]]).abs() > 1500), "audible");
    }
}
