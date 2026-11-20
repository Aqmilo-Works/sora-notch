#![cfg_attr(not(win), allow(unused_imports))]
use std::sync::Mutex;
use serde::Serialize;
// What differs per OS lives in win/focus.rs and mac/focus.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/focus.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/focus.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;
#[derive(Serialize)]
pub struct FocusStatus { active:bool, message:String }
