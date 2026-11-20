//! What differs per operating system. Each backend file exposes the same functions with the same signatures, and the rest of
//! Sora calls only these. Windows is the shipping backend; macOS is being ported (docs/MACOS-AUDIT.md).
//! `cfg(win)` comes from build.rs. The Windows side is in win/platform.rs, the macOS side in mac/platform.rs.
#[cfg(win)]
#[path = "win/platform.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/platform.rs"]
mod os;
pub(crate) use os::*;
