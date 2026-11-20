//! The macOS side of whatsapp (whatsapp.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) const HELPER: &str = "sora-wa";
pub(super) fn modules_dir() -> Option<PathBuf> { Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/com.nook.desktop/modules")) }
/// An unpacked program needs its execute bit on macOS.
pub(super) fn make_runnable(p: &std::path::Path) { use std::os::unix::fs::PermissionsExt; if p.file_name().is_some_and(|n| n == HELPER || n == "sora-tg") { let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)); } }
/// The manifest only ships Windows helpers: macOS runs the one built into Sora.
pub(super) fn updated_helper() -> Option<PathBuf> { None }
