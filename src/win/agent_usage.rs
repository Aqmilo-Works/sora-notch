//! The Windows side of agent_usage (agent_usage.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Antigravity's running language server: its CSRF token and the 127.0.0.1 ports it listens on (Win32 calls only, a few ms).
pub(super) fn agy_server() -> Option<(String, Vec<u16>)> {
    use windows::Win32::{Foundation::CloseHandle, System::{Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS}, Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION}}};
    let mut pids = vec![];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            if String::from_utf16_lossy(&e.szExeFile).trim_end_matches('\0').eq_ignore_ascii_case("language_server.exe") { pids.push(e.th32ProcessID); }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    for pid in pids {
        let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else { continue };
        let cmd = unsafe { command_line(h.0) };
        unsafe { let _ = CloseHandle(h); }
        let Some(cmd) = cmd.filter(|c| c.contains("antigravity")) else { continue };
        let Some(token) = agy_csrf(&cmd) else { continue };
        let ports = listening_ports(pid);
        if !ports.is_empty() { return Some((token, ports)); }
    }
    None
}
#[link(name = "ntdll")]
extern "system" { fn NtQueryInformationProcess(process: *mut std::ffi::c_void, class: u32, info: *mut std::ffi::c_void, len: u32, ret: *mut u32) -> i32; }
#[link(name = "iphlpapi")]
extern "system" { fn GetExtendedTcpTable(table: *mut std::ffi::c_void, size: *mut u32, order: i32, af: u32, class: u32, reserved: u32) -> u32; }
/// Another process's command line (ProcessCommandLineInformation, Windows 8.1 and later).
pub(super) unsafe fn command_line(process: *mut std::ffi::c_void) -> Option<String> {
    let mut len = 0u32;
    let _ = NtQueryInformationProcess(process, 60, std::ptr::null_mut(), 0, &mut len);
    if len == 0 || len > 1 << 20 { return None; }
    // A UNICODE_STRING (length in bytes, max, pointer) followed by the text it points into.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    if NtQueryInformationProcess(process, 60, buf.as_mut_ptr().cast(), len, &mut len) < 0 { return None; }
    let bytes = *(buf.as_ptr() as *const u16) as usize;
    let text = *(buf.as_ptr().cast::<u8>().add(8) as *const *const u16);
    if text.is_null() { return None; }
    Some(String::from_utf16_lossy(std::slice::from_raw_parts(text, bytes / 2)))
}
/// The IPv4 ports a process listens on at 127.0.0.1 (or on every address): GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER.
pub(super) fn listening_ports(pid: u32) -> Vec<u16> {
    let mut size = 0u32;
    unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, 2, 3, 0); }
    if size == 0 { return vec![]; }
    let mut buf = vec![0u32; (size as usize).div_ceil(4) + 64];
    size = (buf.len() * 4) as u32;
    if unsafe { GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut size, 0, 2, 3, 0) } != 0 { return vec![]; }
    listener_rows(&buf, pid)
}
/// Ends a process with its children (Codex's app server starts helpers).
pub(super) fn kill_tree(pid: u32) { let _ = crate::platform::no_window(Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()])).stdout(Stdio::null()).stderr(Stdio::null()).status(); }
pub(super) fn claude_exe() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if let Some(path) = std::env::var_os("PATH") { for d in std::env::split_paths(&path) { c.push(d.join("claude.exe")); c.push(d.join("claude.cmd")); } }
    if let Some(h) = std::env::var_os("USERPROFILE") { c.push(PathBuf::from(h).join(".local").join("bin").join("claude.exe")); }
    if let Some(a) = std::env::var_os("APPDATA") { c.push(PathBuf::from(a).join("npm").join("claude.cmd")); }
    c.into_iter().find(|p| p.is_file())
}
/// The paths a Codex CLI may live at: on PATH, npm's shim, or the Codex app's own copy (newest first).
pub(crate) fn codex_exe() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    if let Some(path) = std::env::var_os("PATH") { for d in std::env::split_paths(&path) { c.push(d.join("codex.exe")); c.push(d.join("codex.cmd")); } }
    if let Some(a) = std::env::var_os("APPDATA") { c.push(PathBuf::from(a).join("npm").join("codex.cmd")); }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        let bin = PathBuf::from(l).join("OpenAI").join("Codex").join("bin");
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&bin).into_iter().flatten().flatten()
            .map(|e| e.path().join("codex.exe")).filter(|p| p.is_file()).map(|p| (std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p)).collect();
        found.sort_by(|a, b| b.0.cmp(&a.0));
        c.extend(found.into_iter().map(|(_, p)| p));
    }
    c.into_iter().find(|p| p.is_file())
}
/// Claude Code's login, as it keeps it on Windows: ~/.claude/.credentials.json.
pub(super) fn claude_credentials() -> Option<Vec<u8>> { std::fs::read(claude_dir()?.join(".credentials.json")).ok() }
