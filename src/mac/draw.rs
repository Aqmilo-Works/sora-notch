//! draw on macOS (main.rs swaps this file in when `cfg(win)` is off). The drawing helpers serve only the native notch,
//! which macOS does not have yet (see mac/notch_ui.rs); only the picture type that notices pass around is needed.
#[allow(dead_code)]
pub struct Picture { pub width: u32, pub height: u32, pub bgra: Vec<u8> }
