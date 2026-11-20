//! The macOS side of health (health.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports, unused_unsafe)]
use super::*;

pub(super) fn today() -> String { std::process::Command::new("date").arg("+%Y-%m-%d").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default() }
/// shortcut: a full-screen app or presentation does not hold the reminder on macOS; upgrade with the Focus state.
pub(super) fn busy() -> bool { false }
pub(super) fn idle_secs() -> u32 { crate::platform::idle_seconds().max(0.0) as u32 }
/// The app in front, by its name (not Sora itself).
pub(super) fn foreground_app() -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    let app = unsafe { NSWorkspace::sharedWorkspace().frontmostApplication() }?;
    if unsafe { app.processIdentifier() } as u32 == std::process::id() { return None; }
    unsafe { app.localizedName() }.map(|n| n.to_string()).filter(|n| !n.is_empty())
}
