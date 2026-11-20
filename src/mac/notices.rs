//! The macOS side of notices (notices.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// The alarm sound on macOS: one of the system sounds, played once, without waiting.
pub(super) fn ring() { let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").spawn(); }
/// macOS windows do not fade in.
pub(super) fn no_fade(_window: &WebviewWindow) {}
/// While the reminder is up it hangs under the notch and moves with it (the mini notch, the open panel, hidden), above
/// the notch's own windows. There is no floating capsule to join on macOS.
pub(super) fn follow_notch(window: &WebviewWindow, start: (i32, i32), _attached: bool, width: i32, dpi: f64) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        // Over the notch (25) and its page (26).
        if let Ok(ns) = w.ns_window() { unsafe { let _: () = objc2::msg_send![ns as *mut objc2::runtime::AnyObject, setLevel: 27isize]; } }
    });
    let w = window.clone();
    thread::spawn(move || {
        // Where it is, its speed, and where it was last put (physical px).
        let (mut pos, mut vel, mut last) = ((start.0 as f64, start.1 as f64), (0.0f64, 0.0f64), start);
        // Until the reminder's window is gone.
        while w.is_visible().is_ok() {
            thread::sleep(Duration::from_millis(16));
            let Some(geom) = crate::notch_ui::notch_geometry() else { continue };
            let (to, _) = notice_place(width, dpi, geom, None);
            // The shape's own spring (mac/notch_ui.rs: stiffness 600, damping 44), so it hangs under the notch all the way.
            for _ in 0..4 {
                let dt = 0.004;
                vel.0 += (600.0 * (to.0 as f64 - pos.0) - 44.0 * vel.0) * dt;
                vel.1 += (600.0 * (to.1 as f64 - pos.1) - 44.0 * vel.1) * dt;
                pos = (pos.0 + vel.0 * dt, pos.1 + vel.1 * dt);
            }
            let at = (pos.0.round() as i32, pos.1.round() as i32);
            if at != last && w.set_position(PhysicalPosition::new(at.0, at.1)).is_ok() { last = at; }
        }
    });
}
