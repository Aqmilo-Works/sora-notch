//! The Windows side of board_tools (board_tools.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

#[tauri::command]
pub fn board_open_terminal() -> AppResult<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let home = std::env::var_os("USERPROFILE").map(std::path::PathBuf::from).filter(|p| p.is_dir());
    let mut wt = std::process::Command::new("wt.exe");
    if let Some(h) = &home { wt.arg("-d").arg(h); }
    if wt.spawn().is_ok() { return Ok("Windows Terminal".into()); }
    let mut cmd = std::process::Command::new("cmd.exe");
    cmd.arg("/k").creation_flags(CREATE_NEW_CONSOLE);
    if let Some(h) = &home { cmd.current_dir(h); }
    cmd.spawn().map_err(err)?;
    Ok("Command Prompt".into())
}
