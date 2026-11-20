//! The Windows side of whatsapp (whatsapp.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) const HELPER: &str = "sora-wa.exe";
/// Where the packed modules are unpacked: Sora's data folder.
pub(super) fn modules_dir() -> Option<PathBuf> { Some(PathBuf::from(std::env::var_os("APPDATA")?).join("com.nook.desktop").join("modules")) }
pub(super) fn make_runnable(_p: &std::path::Path) {}
/// A newer helper from the signed manifest (chat_updates).
pub(super) fn updated_helper() -> Option<PathBuf> { crate::chat_updates::helper() }
