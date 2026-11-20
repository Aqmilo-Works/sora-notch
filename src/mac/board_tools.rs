//! The macOS side of board_tools (board_tools.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// A terminal for a copied command: Windows Terminal when this Windows has it (11), the old console otherwise (10). It opens in the
/// home folder and runs nothing: the command is on the clipboard, the person pastes it and presses Enter.
/// macOS: Terminal, in the home folder, running nothing.
#[tauri::command]
pub fn board_open_terminal() -> AppResult<String> {
    let home = std::env::var_os("HOME").ok_or("Home folder unavailable")?;
    std::process::Command::new("open").args(["-a", "Terminal"]).arg(home).spawn().map_err(err)?;
    Ok("Terminal".into())
}
