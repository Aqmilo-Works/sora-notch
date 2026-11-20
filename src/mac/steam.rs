//! The macOS side of steam (steam.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Steam's folder on macOS.
pub(super) fn steam_dir() -> Option<PathBuf> { std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Steam")).filter(|p| p.join("steamapps").is_dir()) }
/// shortcut: no running game on macOS (Steam keeps it in registry.vdf there); upgrade by reading RunningAppID from that file.
#[tauri::command]
pub fn steam_running() -> u32 { 0 }
