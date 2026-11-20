//! The macOS side of telegram (telegram.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) const TG_HELPER: &str = "sora-tg";
pub(super) const TDLIB: &str = "libtdjson.dylib";
pub(super) fn saved_pack() -> Option<PathBuf> { Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/com.nook.desktop/tg-key.json")) }
