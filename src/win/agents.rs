//! The Windows side of agents (agents.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::{HSTRING, PCWSTR, PWSTR}, Win32::{Foundation::{CloseHandle, BOOL, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, TRUE}, Security::{Authorization::{ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1}, GetTokenInformation, TokenLogonSid, TokenUser, TOKEN_GROUPS, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER}, Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX}, System::{Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS}, Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PeekNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT}, Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION}}, UI::WindowsAndMessaging::{EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, GWL_EXSTYLE, SW_RESTORE, WS_EX_TOOLWINDOW}}};
/// The user a process runs as, as a SID string ("S-1-5-21-…").
pub(super) unsafe fn process_sid(process: HANDLE) -> Option<String> {
    let mut token = HANDLE::default();
    OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
    let mut len = 0u32;
    let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
    let mut buf = vec![0u8; len.max(64) as usize];
    let got = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), buf.len() as u32, &mut len);
    let _ = CloseHandle(token);
    got.ok()?;
    let user = &*(buf.as_ptr() as *const TOKEN_USER);
    let mut s = PWSTR::null();
    ConvertSidToStringSidW(user.User.Sid, &mut s).ok()?;
    // shortcut: the LocalAlloc'd string is not freed (a few bytes per relay, once per server start).
    s.to_string().ok()
}
pub(super) fn my_sid() -> Option<String> { static SID: OnceLock<Option<String>> = OnceLock::new(); SID.get_or_init(|| unsafe { process_sid(GetCurrentProcess()) }).clone() }
/// This logon session's SID ("S-1-5-5-x-y"), which only processes of this sign-in carry.
pub(super) unsafe fn logon_sid() -> Option<String> {
    let mut token = HANDLE::default();
    OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
    let mut len = 0u32;
    let _ = GetTokenInformation(token, TokenLogonSid, None, 0, &mut len);
    let mut buf = vec![0u8; len.max(64) as usize];
    let got = GetTokenInformation(token, TokenLogonSid, Some(buf.as_mut_ptr().cast()), buf.len() as u32, &mut len);
    let _ = CloseHandle(token);
    got.ok()?;
    let g = &*(buf.as_ptr() as *const TOKEN_GROUPS);
    if g.GroupCount == 0 { return None; }
    let mut s = PWSTR::null();
    ConvertSidToStringSidW(g.Groups[0].Sid, &mut s).ok()?;
    s.to_string().ok().filter(|s| s.starts_with("S-1-5-5-"))
}
pub(super) unsafe fn pipe_security() -> Option<SECURITY_ATTRIBUTES> {
    let sddl = HSTRING::from(pipe_sddl(&my_sid()?, logon_sid().as_deref()));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut sd, None).ok()?;
    // The descriptor lives as long as the server (it is made once).
    Some(SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: BOOL(0) })
}
/// Whether the process serving this pipe is Sora (an image with this program's file name) run by this same user (the
/// relay asks before it sends an event or waits for a decision; a pipe squatted by someone else gets nothing). The file
/// name, not the whole path (S-01): the hook command an agent runs can name another copy of Sora than the one running
/// (a copy beside the build, a moved install), and an exact path check silently dropped every event then.
pub(super) unsafe fn server_is_sora(pipe: HANDLE) -> bool {
    let mut pid = 0u32;
    if GetNamedPipeServerProcessId(pipe, &mut pid).is_err() || pid == 0 { return false; }
    let Ok(p) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let image = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok().map(|_| String::from_utf16_lossy(&buf[..len as usize]));
    let sid = process_sid(p);
    let _ = CloseHandle(p);
    let me = std::env::current_exe().ok().map(|e| e.display().to_string());
    same_program(image.as_deref(), me.as_deref()) && sid.is_some() && sid == my_sid()
}
/// Reads until a newline, the end of the stream (the writer closed), an error, or `max` bytes.
pub(super) unsafe fn read_line(h: HANDLE, max: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let mut read = 0u32;
        if ReadFile(h, Some(&mut chunk), Some(&mut read), None).is_err() || read == 0 { break; }
        let got = &chunk[..read as usize];
        buf.extend_from_slice(got);
        if got.contains(&b'\n') || buf.len() > max { break; }
    }
    buf
}
/// Sends one event line to the running Sora and, when a decision is wanted, waits for the answer line.
pub(super) fn send_to_sora(line: &str, wants_reply: bool, fallback: Option<String>) -> Option<String> {
    unsafe {
        let name = HSTRING::from(pipe_name());
        if !WaitNamedPipeW(&name, 300).as_bool() { return None; }
        let Ok(h) = CreateFileW(&name, GENERIC_READ.0 | GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) else { return None };
        if h == INVALID_HANDLE_VALUE { return None; }
        // Not Sora (or not this user's): send nothing, the agent goes on as if Sora were closed.
        if !server_is_sora(h) { let _ = CloseHandle(h); return None; }
        let _ = WriteFile(h, Some(line.as_bytes()), None, None);
        let mut answer = None;
        if wants_reply {
            // Sora decides within 108 s and closes the pipe; this is only the last resort if it hangs.
            let late = fallback.clone();
            thread::spawn(move || { thread::sleep(Duration::from_secs(RELAY_WAIT_S)); if let Some(out) = late { print!("{out}"); } std::process::exit(0); });
            // Answered in the terminal: the agent ends the shell that ran this relay, and the relay leaves with it, so the
            // card goes from the notch at once (Coucou's rule).
            watch_parent(fallback);
            answer = Some(String::from_utf8_lossy(&read_line(h, 64 * 1024)).into_owned());
        }
        let _ = CloseHandle(h);
        answer
    }
}
/// Ends this relay (with the agent's default answer) when the process that started it exits.
pub(super) fn watch_parent(default: Option<String>) {
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};
    let Some(&pid) = ancestors().first() else { return };
    let Ok(h) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else { return };
    let raw = h.0 as isize;
    thread::spawn(move || unsafe {
        let _ = WaitForSingleObject(HANDLE(raw as _), INFINITE);
        if let Some(out) = default { print!("{out}"); let _ = std::io::Write::flush(&mut std::io::stdout()); }
        std::process::exit(0);
    });
}
/// The pids of this process's parents, nearest first (without explorer.exe, which owns every desktop window).
pub(super) fn ancestors() -> Vec<u32> {
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return vec![] };
        let mut parent: HashMap<u32, (u32, String)> = HashMap::new();
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let name = String::from_utf16_lossy(&e.szExeFile).trim_end_matches('\0').to_lowercase();
            parent.insert(e.th32ProcessID, (e.th32ParentProcessID, name));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
        let (mut chain, mut p) = (Vec::new(), std::process::id());
        while let Some((pp, _)) = parent.get(&p) {
            if *pp == 0 || chain.contains(pp) || chain.len() >= 12 { break; }
            if parent.get(pp).map_or(true, |(_, n)| n != "explorer.exe") { chain.push(*pp); }
            p = *pp;
        }
        chain
    }
}
/// Brings the window of the terminal or editor an agent runs in to the front. False when none is found.
pub(super) fn focus_chain(chain: &[u32]) -> bool {
    struct Found { pids: Vec<u32>, best: Option<(usize, HWND)> }
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        let s = &mut *(l.0 as *mut Found);
        if !IsWindowVisible(h).as_bool() || GetWindowTextLengthW(h) == 0 || (GetWindowLongW(h, GWL_EXSTYLE) as u32) & WS_EX_TOOLWINDOW.0 != 0 { return TRUE; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if let Some(i) = s.pids.iter().position(|p| *p == pid) { if s.best.map_or(true, |(b, _)| i < b) { s.best = Some((i, h)); } }
        TRUE
    }
    let mut s = Found { pids: chain.to_vec(), best: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut s as *mut _ as isize));
        let Some((_, h)) = s.best else { return false };
        if IsIconic(h).as_bool() { let _ = ShowWindow(h, SW_RESTORE); }
        SetForegroundWindow(h).as_bool()
    }
}
/// Starts the pipe server once, when the Agents feature is on (at start-up or when it is switched on).
pub fn start(app: &AppHandle) {
    if !crate::desktop::preferences(app).feature_agents || STARTED.swap(true, Ordering::SeqCst) { return; }
    crate::agent_extras::start_hotkeys();
    watch_codex(app.clone());
    let app = app.clone();
    thread::spawn(move || unsafe {
        let name = HSTRING::from(pipe_name());
        let Some(sa) = pipe_security() else { *PIPE_ERROR.lock().unwrap() = "Could not secure the agent pipe.".into(); STARTED.store(false, Ordering::SeqCst); return };
        let mut first = true;
        loop {
            // The first instance claims the name: if another process already holds it, Sora stays off rather than share it.
            let open = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { FILE_FLAGS_AND_ATTRIBUTES(0) };
            let pipe = CreateNamedPipeW(&name, open, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 64 * 1024, 64 * 1024, 0, Some(&sa));
            if pipe == INVALID_HANDLE_VALUE {
                if first { *PIPE_ERROR.lock().unwrap() = "Another program holds Sora's agent pipe, so agent events are off.".into(); STARTED.store(false, Ordering::SeqCst); return; }
                thread::sleep(Duration::from_secs(5)); continue;
            }
            first = false;
            let connected = ConnectNamedPipe(pipe, None);
            // ERROR_PIPE_CONNECTED: the writer was quicker than us, which is fine.
            if connected.is_ok() || connected.as_ref().err().is_some_and(|e| e.code().0 as u32 == 0x8007_0217) {
                if SERVING.load(Ordering::SeqCst) >= MAX_CONNECTIONS { let _ = DisconnectNamedPipe(pipe); let _ = CloseHandle(pipe); continue; }
                SERVING.fetch_add(1, Ordering::SeqCst);
                // One thread per connection: a request waiting for the user must not hold up the other events.
                let (raw, app) = (pipe.0 as isize, app.clone());
                thread::spawn(move || { serve(&app, HANDLE(raw as _)); SERVING.fetch_sub(1, Ordering::SeqCst); });
            } else {
                let _ = CloseHandle(pipe);
            }
        }
    });
}
pub(super) fn serve(app: &AppHandle, pipe: HANDLE) {
    unsafe {
        let buf = read_line(pipe, MAX_EVENT);
        // Over the size limit: dropped whole (a cut JSON would not parse anyway).
        if buf.len() <= MAX_EVENT && feature_on(app) {
            if let Ok(e) = serde_json::from_slice::<Value>(&buf) {
                if e.is_object() { if can_decide(&e) && (e.get("sora_agent").and_then(Value::as_str) != Some("antigravity") || agy_gate(app, &e)) { hold(app, &pipe, e); } else { handle(app, &e); } }
            }
        }
        let _ = DisconnectNamedPipe(pipe);
        let _ = CloseHandle(pipe);
    }
}
/// The connection to one relay (a pipe instance).
pub(super) type Conn = HANDLE;
pub(super) fn alive(pipe: &Conn) -> bool { unsafe { PeekNamedPipe(*pipe, None, 0, None, None, None).is_ok() } }
/// Sends the decision line back to the relay that waits for it.
pub(super) fn write_reply(pipe: &Conn, line: &str) { unsafe { let _ = WriteFile(*pipe, Some(line.as_bytes()), None, None); let _ = windows::Win32::Storage::FileSystem::FlushFileBuffers(*pipe); } }
pub(super) const VS_CODE: &str = "code.cmd";
/// Shows a file selected in Explorer.
pub(super) fn reveal(file: &std::path::Path) -> Result<(), String> { std::process::Command::new("explorer.exe").arg(format!("/select,{}", file.display())).spawn().map(|_| ()).map_err(|e| e.to_string()) }
pub(super) const HOME_VAR: &str = "USERPROFILE";
