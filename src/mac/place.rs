//! The macOS side of place (place.rs), its `os` module. Everything else there is shared.
//! The device position comes from Core Location (Wi-Fi, as Maps uses), asked on this thread with its own run loop. The
//! first ask shows macOS's location prompt (Info.plist NSLocationWhenInUseUsageDescription); without permission the
//! IP address's city is used, as on Windows.
#![allow(unused_imports, unused_unsafe)]
use super::*;
use std::{sync::Mutex, time::Instant};
use objc2::{define_class, msg_send, rc::Retained, runtime::ProtocolObject, AllocAnyThread, DefinedClass};
use objc2_core_location::{CLLocation, CLLocationManager, CLLocationManagerDelegate};
use objc2_foundation::{NSArray, NSDate, NSError, NSObject, NSObjectProtocol, NSRunLoop};

struct Ivars { found: Mutex<Option<Option<(f64, f64, f64)>>> }
define_class!(
    // Keeps the newest position (or that none will come).
    #[unsafe(super(NSObject))]
    #[name = "SoraPlaceDelegate"]
    #[ivars = Ivars]
    struct Watcher;

    unsafe impl NSObjectProtocol for Watcher {}

    unsafe impl CLLocationManagerDelegate for Watcher {
        #[unsafe(method(locationManager:didUpdateLocations:))]
        fn updated(&self, _manager: &CLLocationManager, locations: &NSArray<CLLocation>) {
            if let Some(l) = locations.lastObject() {
                let c = unsafe { l.coordinate() };
                if let Ok(mut f) = self.ivars().found.lock() { *f = Some(Some((c.latitude, c.longitude, unsafe { l.horizontalAccuracy() }))); }
            }
        }
        #[unsafe(method(locationManager:didFailWithError:))]
        fn failed(&self, _manager: &CLLocationManager, _error: &NSError) {
            if let Ok(mut f) = self.ivars().found.lock() { if f.is_none() { *f = Some(None); } }
        }
    }
);

/// The device position (latitude, longitude, accuracy in metres), waiting at most 6 s as on Windows.
pub(super) fn device_position() -> Option<(f64, f64, f64)> {
    let watcher = Watcher::alloc().set_ivars(Ivars { found: Mutex::new(None) });
    let watcher: Retained<Watcher> = unsafe { msg_send![super(watcher), init] };
    let manager = unsafe { CLLocationManager::new() };
    unsafe {
        manager.setDelegate(Some(ProtocolObject::from_ref(&*watcher)));
        manager.setDesiredAccuracy(objc2_core_location::kCLLocationAccuracyHundredMeters);
        manager.requestWhenInUseAuthorization();
        manager.startUpdatingLocation();
    }
    let until = Instant::now() + Duration::from_secs(6);
    let result = loop {
        // The delegate is called from this thread's run loop.
        unsafe { NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.2)) };
        if let Some(r) = watcher.ivars().found.lock().ok().and_then(|f| *f) { break r; }
        if Instant::now() >= until { break None; }
    };
    unsafe {
        manager.stopUpdatingLocation();
        manager.setDelegate(None);
    }
    result
}
