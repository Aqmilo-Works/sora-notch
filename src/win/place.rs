//! The Windows side of place (place.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Windows' own position, when location is on for desktop apps. Accuracy is in metres.
pub(super) fn device_position() -> Option<(f64, f64, f64)> {
    use windows::{Devices::Geolocation::*, Foundation::TimeSpan, Win32::System::Com::*};
    unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
    if Geolocator::RequestAccessAsync().ok()?.get().ok()? != GeolocationAccessStatus::Allowed { return None; }
    let g = Geolocator::new().ok()?;
    g.SetDesiredAccuracy(PositionAccuracy::High).ok()?;
    // A position up to 2 minutes old is fine; never wait longer than 6 s for a new one.
    let pos = g.GetGeopositionAsyncWithAgeAndTimeout(TimeSpan::from(Duration::from_secs(120)), TimeSpan::from(Duration::from_secs(6))).ok()?.get().ok()?;
    let c = pos.Coordinate().ok()?;
    let p = c.Point().ok()?.Position().ok()?;
    Some((p.Latitude, p.Longitude, c.Accuracy().ok()?))
}
