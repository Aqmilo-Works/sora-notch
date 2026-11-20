//! The Windows side of telegram (telegram.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

pub(super) const TG_HELPER: &str = "sora-tg.exe";
pub(super) const TDLIB: &str = "tdjson.dll";
/// A pack fetched from the website. Not inside the telegram folder: signing out deletes that one.
pub(super) fn saved_pack() -> Option<PathBuf> { Some(PathBuf::from(std::env::var_os("APPDATA")?).join("com.nook.desktop").join("tg-key.json")) }
