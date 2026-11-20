//! The notch on macOS, drawn natively: a borderless NSPanel over the top of the main display holding one black
//! CAShapeLayer (the notch with its inverted top corners) that springs from the mini notch to the panel and back, as on
//! Windows. The mini notch wraps the camera housing on a MacBook with one (it only adds a lip below and beside it, where
//! nothing hides), and the panel's contents start below the camera: the page sits under the menu bar's height.
//! Home, Music, Tray... are the web page in panel mode (index.html?panel=1), a transparent WebView window above the
//! shape, kept loaded and shown while the panel is open. The panel ignores the mouse: a watcher reads the pointer and
//! buttons (no click is taken from other apps) and opens on a click or a file dragged onto the notch, closes on a click
//! outside. `--web-notch` (open -a Sora --args --web-notch) brings back the all-web notch.
//! On a MacBook with a camera notch the mini notch is the camera notch with a wing on each side, in its height: the
//! clock on the left and, on the right, the date, or the recording dot, the focus timer or the music bars (in that order).
//! Without a camera notch (the camera is in the frame) the mini notch is a capsule as tall as the menu bar with Sora's face
//! in its middle; the face never moves aside: during a timer or a recording the clock and the status take its place, and
//! with music it dances as the Windows mascot does (mac/rig.rs: blinks, glances, nods, sways, hums), without notes.
use std::{cell::RefCell, sync::{atomic::{AtomicBool, Ordering}, mpsc::Receiver, Mutex, OnceLock}, time::{Duration, Instant}};
use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSColor, NSPanel, NSScreen, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_core_foundation::{CFRetained, CGAffineTransform, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{ns_string, NSNumber, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{CABasicAnimation, CALayer, CAMediaTiming, CAShapeLayer, CASpringAnimation, CATextLayer, CATransaction};
use tauri::{Emitter, Manager};
#[path = "rig.rs"]
mod rig;

pub use crate::marks::{app_info, marks, MarkView};

// Sizes in the page's CSS px, as on Windows (win/notch_ui.rs), times the notch scale `k` (points per CSS px).
const PANEL: (f64, f64, f64) = (720.0, 234.0, 30.0);
const EAR: f64 = 12.0;
const TALLEST: f64 = 400.0;
/// A Home menu (Focus, Location...) or History: the panel grows by this much, as on Windows.
const TALL: f64 = 150.0;
/// A wing beside the camera housing (the clock, the date, the timer), before the 1.9 that gives them room.
const WING: f64 = 64.0;
/// The camera housing's bottom corners (points), which the island keeps so it reads as the notch itself.
const CAMERA_RADIUS: f64 = 10.0;
/// The capsule's height on a Mac without a camera notch when the menu bar hides itself (points; the menu bar is 24).
const MIN_BAND: f64 = 24.0;
/// How much the capsule swells while the pointer rests on it (a Mac without a camera notch).
const PEEK: (f64, f64) = (14.0, 6.0);

/// What the shape and the page follow. Points, from the main display's top-left corner.
#[derive(Clone, Copy)]
/// `busy`: a timer or a recording runs (in the capsule, the clock and that status take the face's place).
struct Geom { width: f64, inset: f64, camera: f64, scale: f64, k: f64, open: bool, peek: bool, tall: f64, visible: bool, busy: bool }
static GEOM: Mutex<Option<Geom>> = Mutex::new(None);
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
static PAGE_READY: AtomicBool = AtomicBool::new(false);
static PAGE_SHOWN: AtomicBool = AtomicBool::new(false);
static RECORDING: AtomicBool = AtomicBool::new(false);
static TAB: Mutex<String> = Mutex::new(String::new());
static DEADLINE: Mutex<Option<i64>> = Mutex::new(None);
static DEADLINE_FILE: OnceLock<std::path::PathBuf> = OnceLock::new();
static PLAYING: AtomicBool = AtomicBool::new(false);
/// Clicks that went to another app (an NSEvent global monitor): a click outside the open panel, however short.
static OUTSIDE_CLICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// A file dialog of Sora's is up: the notch sits at the normal level under it, and no click folds the panel away.
static DIALOG: AtomicBool = AtomicBool::new(false);
/// Panel openings so far: a fold-back still waiting cancels itself when the panel opened again meanwhile.
static OPENS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Where the pointer is from the face, -1..1 across and down, while it is near the notch: the face looks at it.
static LOOK: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// Hides in flight: a hide's order-out waits for its slide, and a show in between cancels it.
static HIDES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// What the right wing shows.
#[derive(Clone, PartialEq, Debug)]
enum Right { Nothing, Recording, Timer(String), Music }
/// `top`: the panel's height. Its layers are drawn bottom-up (AppKit's own y), never flipped: flipping a layer was
/// ignored on real Macs and drew the notch upside down at the panel's bottom.
/// `root` holds the shape and the wings (it slides up out of sight to hide them); `shown`: the panel is on screen.
struct Ui { panel: Retained<NSPanel>, root: Retained<CALayer>, shape: Retained<CAShapeLayer>, content: Content, top: f64, shown: std::cell::Cell<bool> }
/// The mini notch's layers: the face (two eyes and a mouth, in the 160 x 70 units of the Windows mascot, posed by the
/// rig), the clock (left wing), the timer's digits or the date (right wing), four music bars and the red dot.
struct Content { all: Retained<CALayer>, face: Retained<CALayer>, eyes: [Retained<CAShapeLayer>; 2], mouth: Retained<CAShapeLayer>, clock: Retained<CATextLayer>, timer: Retained<CATextLayer>, bars: Retained<CALayer>, bar: Vec<Retained<CALayer>>, dot: Retained<CALayer>, right: RefCell<Right> }
thread_local! { static UI: RefCell<Option<Ui>> = const { RefCell::new(None) }; }

/// The notch scale for the Size slider, as on Windows (0 smallest, 100 largest).
fn size_scale(size: u32) -> f64 { 0.64 + (0.85 - 0.64) * size.min(100) as f64 / 100.0 }
/// The shape now: centre x, body width and height, bottom corner radius, ear size (points).
fn shape_of(g: &Geom) -> (f64, f64, f64, f64, f64) {
    let cx = g.width / 2.0;
    let ear = EAR * g.k;
    if g.open { return (cx, PANEL.0 * g.k, g.inset + (PANEL.1 + g.tall) * g.k, PANEL.2 * g.k, ear); }
    let (band, wing) = (band(g), wing(g));
    // Beside a camera: the camera notch with a wing on each side, always; its ears as small as Windows' notch's.
    if g.camera > 0.0 { return (cx, g.camera + 2.0 * wing, band, CAMERA_RADIUS, band * EAR_SHARE); }
    // The capsule: the face alone, or the clock and the timer or recording side by side; Windows' corners, no pill ends.
    let w = if g.busy { 3.3 * wing } else { 2.1 * wing };
    let (w, h) = if g.peek { (w + PEEK.0 * g.k, band + PEEK.1 * g.k) } else { (w, band) };
    (cx, w, h, (h * CORNER_SHARE).max(6.0), h * EAR_SHARE)
}
/// Windows' mini notch: bottom corners 20 and top ears 12 on a notch 64 tall, as shares of the height.
const CORNER_SHARE: f64 = 20.0 / 64.0;
const EAR_SHARE: f64 = 12.0 / 64.0;
/// Small (0.85) or Large (1): the Size preference, read off the notch scale (0.85 is the largest).
fn size_of(g: &Geom) -> f64 { (g.k / 0.85).min(1.0) }
/// The mini notch's height: the camera housing's, or the menu bar's on a Mac without one.
fn band(g: &Geom) -> f64 { if g.camera > 0.0 { g.inset } else { g.inset.max(MIN_BAND) } }
/// A wing's width: beside a camera, room for the clock or the date clear of both edges; in the capsule, twice its height
/// (both smaller at the Small size).
fn wing(g: &Geom) -> f64 { if g.camera > 0.0 { 1.9 * WING * g.k } else { 2.0 * band(g) * size_of(g) } }
/// Where the wings' contents sit: left centre x, right centre x, the band's centre y and height (pure; tested): beside
/// the camera housing, in its height, never over it; in the capsule the face in the middle, or (while `busy`) the clock
/// left of the middle and the status right of it.
fn wings(g: &Geom) -> (f64, f64, f64, f64) {
    let (band, wing, cx) = (band(g), wing(g), g.width / 2.0);
    let (l, r) = if g.camera > 0.0 { let off = g.camera / 2.0 + wing / 2.0; (cx - off, cx + off) } else if g.busy { (cx - 0.8 * wing, cx + 0.8 * wing) } else { (cx, cx) };
    (l, r, band / 2.0, band)
}
/// Whether Sora's face shows (pure; tested): only in the capsule, and not while a timer or a recording takes its place.
fn mascot_shows(g: &Geom) -> bool { g.camera <= 0.0 && !g.busy && !g.open }
/// A time pattern without its AM/PM (pure; tested): "h:mm a" is "h:mm", the wings are narrow.
fn short_time(pattern: &str) -> String { pattern.replace('a', "").replace(['\u{202f}', '\u{a0}'], " ").trim().to_string() }
/// The time and the date as the wings show them, in the Mac's own 12/24-hour choice and language: "9:21", "Thu 19".
fn now_texts() -> (String, String) {
    use objc2_foundation::{NSDate, NSDateFormatter, NSLocale};
    let locale = NSLocale::currentLocale();
    let f = NSDateFormatter::new();
    f.setLocale(Some(&locale));
    let time = NSDateFormatter::dateFormatFromTemplate_options_locale(ns_string!("jmm"), 0, Some(&locale)).map(|p| short_time(&p.to_string())).unwrap_or_else(|| "H:mm".into());
    f.setDateFormat(Some(&NSString::from_str(&time)));
    let now = NSDate::now();
    let t = f.stringFromDate(&now).to_string();
    f.setLocalizedDateFormatFromTemplate(ns_string!("EEEd"));
    (t, f.stringFromDate(&now).to_string())
}
/// The focus timer as the notch shows it: "m:ss", or "h:mm:ss" from an hour (pure; tested).
fn clock(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}
/// What the right wing shows now: the recording dot first, then a running timer, then music (pure; tested).
fn right_for(recording: bool, left_ms: Option<i64>, playing: bool) -> Right {
    if recording { Right::Recording } else if let Some(ms) = left_ms.filter(|m| *m > 0) { Right::Timer(clock(ms)) } else if playing { Right::Music } else { Right::Nothing }
}
fn now_ms() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }

/// A path of `points`: a start and cubic segments (three points each).
fn curves(points: &[(f64, f64)]) -> CFRetained<CGMutablePath> {
    let p = CGMutablePath::new();
    let n = std::ptr::null();
    unsafe {
        CGMutablePath::move_to_point(Some(&p), n, points[0].0, points[0].1);
        for c in points[1..].chunks(3) { CGMutablePath::add_curve_to_point(Some(&p), n, c[0].0, c[0].1, c[1].0, c[1].1, c[2].0, c[2].1); }
    }
    p
}
fn white(a: f64) -> CFRetained<CGColor> { CGColor::new_srgb(1.0, 1.0, 1.0, a) }
fn stroke(path: &CGMutablePath, width: f64) -> Retained<CAShapeLayer> {
    let l = CAShapeLayer::new();
    l.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(160.0, 70.0)));
    l.setPath(Some(path));
    l.setFillColor(None);
    l.setStrokeColor(Some(&white(1.0)));
    l.setLineWidth(width);
    l.setLineCap(unsafe { objc2_quartz_core::kCALineCapRound });
    l.setLineJoin(unsafe { objc2_quartz_core::kCALineJoinRound });
    l
}
/// Draws the face in a pose of the rig. Main thread only.
fn draw_face(c: &Content, p: &rig::Pose) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    for (i, e) in c.eyes.iter().enumerate() { e.setPath(Some(&curves(&p.eyes[i]))); e.setLineWidth(p.eye_width[i]); }
    c.mouth.setPath(Some(&curves(&p.mouth)));
    c.mouth.setLineWidth(p.mouth_width);
    CATransaction::commit();
}
/// A line of white digits (the clock, the timer, the date) in the Mac's system font with even-width digits.
fn digits(scale: f64) -> Retained<CATextLayer> {
    let t = CATextLayer::new();
    t.setContentsScale(scale);
    t.setForegroundColor(Some(&white(0.92)));
    t.setAlignmentMode(unsafe { objc2_quartz_core::kCAAlignmentCenter });
    let font = objc2_app_kit::NSFont::monospacedDigitSystemFontOfSize_weight(13.0, unsafe { objc2_app_kit::NSFontWeightSemibold });
    unsafe { t.setFont(Some(&*(Retained::as_ptr(&font) as *const objc2_core_foundation::CFType))) };
    t.setHidden(true);
    t
}
/// A repeating animation of one value (scale or offset), back and forth.
fn swing(key: &NSString, from: f64, to: f64, seconds: f64, forever: bool) -> Retained<CABasicAnimation> {
    let a = CABasicAnimation::animationWithKeyPath(Some(key));
    unsafe {
        a.setFromValue(Some(&NSNumber::new_f64(from)));
        a.setToValue(Some(&NSNumber::new_f64(to)));
    }
    a.setDuration(seconds);
    a.setAutoreverses(true);
    a.setRepeatCount(if forever { f32::INFINITY } else { 1.0 });
    a
}
fn content(scale: f64) -> Content {
    let all = CALayer::new();
    let face = CALayer::new();
    face.setBounds(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(160.0, 70.0)));
    face.setAnchorPoint(CGPoint::new(0.5, 31.0 / 70.0));
    let rest = rig::pose(&rig::rest(), 1.0);
    let eyes = [stroke(&curves(&rest.eyes[0]), rest.eye_width[0]), stroke(&curves(&rest.eyes[1]), rest.eye_width[1])];
    for e in &eyes { face.addSublayer(e); }
    let mouth = stroke(&curves(&rest.mouth), rest.mouth_width);
    face.addSublayer(&mouth);
    all.addSublayer(&face);
    let (clock, timer) = (digits(scale), digits(scale));
    all.addSublayer(&clock);
    all.addSublayer(&timer);
    let bars = CALayer::new();
    let bar: Vec<Retained<CALayer>> = (0..4).map(|_| { let b = CALayer::new(); b.setBackgroundColor(Some(&white(0.9))); b.setAnchorPoint(CGPoint::new(0.5, 0.0)); bars.addSublayer(&b); b }).collect();
    bars.setHidden(true);
    all.addSublayer(&bars);
    let dot = CALayer::new();
    dot.setBackgroundColor(Some(&CGColor::new_srgb(1.0, 0.27, 0.23, 1.0)));
    dot.setHidden(true);
    all.addSublayer(&dot);
    Content { all, face, eyes, mouth, clock, timer, bars, bar, dot, right: RefCell::new(Right::Nothing) }
}
/// Puts the wings' contents where the mini notch is, and fades them while the panel is open. `top`: the panel's height
/// (layers run bottom-up).
fn lay_out(c: &Content, g: &Geom, top: f64) {
    let (lx, rx, y, band) = wings(g);
    let y = top - y;
    c.all.setOpacity(if g.open { 0.0 } else { 1.0 });
    // The face: its eyes and mouth (34 units tall) take 62% of the band, in the middle of the capsule.
    let s = band * 0.62 / 34.0;
    c.face.setPosition(CGPoint::new(g.width / 2.0, y));
    // The face is drawn top-down (the Windows rig's units): d < 0 stands it upright in the bottom-up panel.
    c.face.setAffineTransform(CGAffineTransform { a: s, b: 0.0, c: 0.0, d: -s, tx: 0.0, ty: 0.0 });
    c.face.setHidden(!mascot_shows(g));
    c.clock.setHidden(mascot_shows(g));
    let (size, wide) = ((band * 0.41).max(10.0), wing(g));
    for (t, x) in [(&c.clock, lx), (&c.timer, rx)] {
        t.setFontSize(size);
        t.setFrame(CGRect::new(CGPoint::new(x - wide / 2.0, y - size * 0.68), CGSize::new(wide, size * 1.3)));
    }
    let (bw, bh, gap) = ((band * 0.09).max(2.0), band * 0.42, (band * 0.07).max(1.5));
    c.bars.setFrame(CGRect::new(CGPoint::new(rx - (4.0 * bw + 3.0 * gap) / 2.0, y - bh / 2.0), CGSize::new(4.0 * bw + 3.0 * gap, bh)));
    for (i, b) in c.bar.iter().enumerate() {
        b.setBounds(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(bw, bh)));
        b.setPosition(CGPoint::new(i as f64 * (bw + gap) + bw / 2.0, 0.0));
        b.setCornerRadius(bw / 2.0);
    }
    let r = (band * 0.16).clamp(3.0, 5.0);
    c.dot.setFrame(CGRect::new(CGPoint::new(rx - r, y - r), CGSize::new(2.0 * r, 2.0 * r)));
    c.dot.setCornerRadius(r);
}
/// Shows what the right wing should: the dot, the timer's digits, the music bars (beside a camera: in the capsule the face
/// dances instead), or beside a camera with nothing on, the date. Main thread only.
fn set_right(c: &Content, right: Right, camera: bool, date: &str) {
    c.dot.setHidden(right != Right::Recording);
    let text = match &right { Right::Timer(t) => Some(t.clone()), Right::Nothing if camera => Some(date.to_string()), _ => None };
    if let Some(t) = &text { unsafe { c.timer.setString(Some(&NSString::from_str(t))) }; }
    c.timer.setHidden(text.is_none());
    let music = right == Right::Music && camera;
    if music != !c.bars.isHidden() {
        c.bars.setHidden(!music);
        for (i, b) in c.bar.iter().enumerate() {
            if music { b.addAnimation_forKey(&swing(ns_string!("transform.scale.y"), 1.0, 0.3, [0.42, 0.55, 0.37, 0.61][i], true), Some(ns_string!("bounce"))); }
            else { b.removeAnimationForKey(ns_string!("bounce")); }
        }
    }
    *c.right.borrow_mut() = right;
}

/// Every half second: the clock and the date, the timer's digits (and its end: a chime and the reminder), what the right
/// wing shows; the music player is asked every three seconds.
fn tick() {
    let mut n = 0u64;
    let mut shown: Option<(Right, String)> = None;
    let mut time = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(500));
        if n % 6 == 0 { PLAYING.store(crate::media_read(false).ok().flatten().is_some_and(|m| m.playing), Ordering::SeqCst); }
        n += 1;
        let end = DEADLINE.lock().ok().and_then(|d| *d);
        if end.is_some_and(|e| now_ms() >= e) {
            set_deadline(None);
            let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Glass.aiff").spawn();
            if let Some(app) = APP.get() { crate::notices::timer_finished(app); }
        }
        let (now, date) = now_texts();
        if now != time {
            time = now.clone();
            on_main(move || UI.with(|ui| if let Some(ui) = ui.borrow().as_ref() { unsafe { ui.content.clock.setString(Some(&NSString::from_str(&now))) }; }));
        }
        let right = right_for(RECORDING.load(Ordering::SeqCst), end.map(|e| e - now_ms()), PLAYING.load(Ordering::SeqCst));
        if shown.as_ref() != Some(&(right.clone(), date.clone())) {
            shown = Some((right.clone(), date.clone()));
            let busy = matches!(right, Right::Recording | Right::Timer(_));
            let camera = geom().is_some_and(|g| g.camera > 0.0);
            on_main(move || UI.with(|ui| if let Some(ui) = ui.borrow().as_ref() { set_right(&ui.content, right, camera, &date); }));
            // In the capsule a timer or a recording puts the clock and the status where the face was.
            if geom().is_some_and(|g| g.busy != busy) { update(|g| g.busy = busy); redraw(false); }
        }
    }
}
/// The face's rig, while the face is on screen: 60 steps a second with music (the dance), 30 at rest (the glance and the
/// blinks), none while it is hidden.
fn animate() {
    let mut rig = rig::Rig::new();
    let (mut last, mut seed) = (Instant::now(), now_ms() as u64 | 1);
    loop {
        let on = geom().is_some_and(|g| g.visible && mascot_shows(&g));
        let (music, looking) = (PLAYING.load(Ordering::SeqCst), LOOK.lock().is_ok_and(|l| l.is_some()));
        std::thread::sleep(Duration::from_millis(if !on { 200 } else if music || looking { 16 } else { 33 }));
        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        if !on { continue; }
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let look = LOOK.lock().ok().and_then(|l| *l);
        let lid = rig.step(dt, music, (seed >> 11) as f64 / (1u64 << 53) as f64, look);
        let pose = rig::pose(&rig.face, lid);
        on_main(move || UI.with(|ui| if let Some(ui) = ui.borrow().as_ref() { draw_face(&ui.content, &pose); }));
    }
}
fn set_deadline(end: Option<i64>) {
    if let Ok(mut d) = DEADLINE.lock() { *d = end; }
    if let Some(f) = DEADLINE_FILE.get() { let _ = std::fs::write(f, serde_json::to_vec(&end).unwrap_or_default()); }
}

/// The notch outline: straight along the panel's top edge, ears curving in, round bottom corners. Worked out top-down
/// (y from the screen's top), drawn bottom-up: `top` is the panel's height. The top edge runs 2 points above the screen
/// (off it): exactly on the edge, its row of pixels was only half covered and showed as a thin line over the notch.
fn outline(cx: f64, w: f64, h: f64, r: f64, ear: f64, top: f64) -> CFRetained<CGMutablePath> {
    let p = CGMutablePath::new();
    let (l, rt) = (cx - w / 2.0, cx + w / 2.0);
    let n = std::ptr::null();
    let y = |v: f64| top - v;
    unsafe {
        CGMutablePath::move_to_point(Some(&p), n, l - ear, y(-2.0));
        CGMutablePath::add_line_to_point(Some(&p), n, rt + ear, y(-2.0));
        CGMutablePath::add_line_to_point(Some(&p), n, rt + ear, y(0.0));
        CGMutablePath::add_arc_to_point(Some(&p), n, rt, y(0.0), rt, y(ear), ear);
        CGMutablePath::add_line_to_point(Some(&p), n, rt, y(h - r));
        CGMutablePath::add_arc_to_point(Some(&p), n, rt, y(h), rt - r, y(h), r);
        CGMutablePath::add_line_to_point(Some(&p), n, l + r, y(h));
        CGMutablePath::add_arc_to_point(Some(&p), n, l, y(h), l, y(h - r), r);
        CGMutablePath::add_line_to_point(Some(&p), n, l, y(ear));
        CGMutablePath::add_arc_to_point(Some(&p), n, l, y(0.0), l - ear, y(0.0), ear);
        CGMutablePath::add_line_to_point(Some(&p), n, l - ear, y(-2.0));
    }
    CGMutablePath::close_subpath(Some(&p));
    p
}
/// Whether a point (points, display top-left) is on the shape's box, ears included (pure; tested).
fn on_shape(s: (f64, f64, f64, f64, f64), x: f64, y: f64) -> bool { (x - s.0).abs() <= s.1 / 2.0 + s.4 && y >= 0.0 && y <= s.2 }

fn geom() -> Option<Geom> { GEOM.lock().ok().and_then(|g| *g) }
fn update(f: impl FnOnce(&mut Geom)) -> Option<Geom> { let mut g = GEOM.lock().ok()?; let v = g.as_mut()?; f(v); Some(*v) }

/// The main display: width, menu bar height (the camera housing's on a MacBook with one), the housing's width, scale.
fn display(mtm: MainThreadMarker) -> Option<(f64, f64, f64, f64)> {
    let screen = NSScreen::screens(mtm).firstObject()?;
    let f = screen.frame();
    let v = screen.visibleFrame();
    let bar = (f.origin.y + f.size.height) - (v.origin.y + v.size.height);
    let safe = screen.safeAreaInsets().top;
    let camera = if safe > 0.0 { f.size.width - screen.auxiliaryTopLeftArea().size.width - screen.auxiliaryTopRightArea().size.width } else { 0.0 };
    Some((f.size.width, bar.max(safe), camera.max(0.0), screen.backingScaleFactor()))
}

/// Draws the shape for `g` (animated with a spring unless `now`), and moves the page to it. Main thread only.
fn apply(g: Geom, now: bool) {
    UI.with(|ui| {
        let ui = ui.borrow();
        let Some(ui) = ui.as_ref() else { return };
        let s = shape_of(&g);
        let path = outline(s.0, s.1, s.2, s.3, s.4, ui.top);
        let from = unsafe { ui.shape.presentationLayer() }.and_then(|p| p.downcast::<CAShapeLayer>().ok()).and_then(|p| p.path()).or_else(|| ui.shape.path());
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        ui.shape.setPath(Some(&path));
        CATransaction::commit();
        if let (false, Some(from)) = (now, from) {
            let anim = CASpringAnimation::animationWithKeyPath(Some(ns_string!("path")));
            anim.setMass(1.0);
            anim.setStiffness(600.0);
            anim.setDamping(44.0);
            anim.setDuration(anim.settlingDuration());
            unsafe {
                anim.setFromValue(Some(&*(Retained::as_ptr(&from) as *const AnyObject)));
                anim.setToValue(Some(&*(CFRetained::as_ptr(&path).as_ptr() as *const AnyObject)));
            }
            ui.shape.addAnimation_forKey(&anim, Some(ns_string!("path")));
        }
        lay_out(&ui.content, &g, ui.top);
        // Hidden and shown again (the shortcut, full screen): the notch slides up out of the screen's edge and back down.
        let away = s.2 + s.4 + 8.0;
        match (ui.shown.replace(g.visible), g.visible) {
            (false, true) => { HIDES.fetch_add(1, Ordering::SeqCst); ui.panel.orderFrontRegardless(); slide(&ui.root, away, 0.0, false); }
            (true, false) => {
                slide(&ui.root, 0.0, away, true);
                let hide = HIDES.fetch_add(1, Ordering::SeqCst) + 1;
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(260));
                    on_main(move || UI.with(|ui| if let Some(ui) = ui.borrow().as_ref() { if !ui.shown.get() && HIDES.load(Ordering::SeqCst) == hide { ui.panel.orderOut(None); } }));
                });
            }
            (_, true) => ui.panel.orderFrontRegardless(),
            _ => {}
        }
    });
    place_page(&g);
}
/// Moves `root` from `from` to `to` points up (bottom-up y), easing in when it leaves and out when it arrives.
fn slide(root: &CALayer, from: f64, to: f64, leaving: bool) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    root.setAffineTransform(CGAffineTransform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: to });
    CATransaction::commit();
    let a = CABasicAnimation::animationWithKeyPath(Some(ns_string!("transform.translation.y")));
    unsafe {
        a.setFromValue(Some(&NSNumber::new_f64(from)));
        a.setToValue(Some(&NSNumber::new_f64(to)));
        let curve = if leaving { objc2_quartz_core::kCAMediaTimingFunctionEaseIn } else { objc2_quartz_core::kCAMediaTimingFunctionEaseOut };
        a.setTimingFunction(Some(&objc2_quartz_core::CAMediaTimingFunction::functionWithName(curve)));
    }
    a.setDuration(if leaving { 0.22 } else { 0.32 });
    root.addAnimation_forKey(&a, Some(ns_string!("slide")));
}
/// The page sits under the menu bar (below the camera), an ear wider on each side, as tall as the panel is.
fn place_page(g: &Geom) {
    let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) else { return };
    let x = g.width / 2.0 - (PANEL.0 / 2.0 + EAR) * g.k;
    let _ = w.set_position(tauri::LogicalPosition::new(x, g.inset));
    let _ = w.set_size(tauri::LogicalSize::new((PANEL.0 + 2.0 * EAR) * g.k, (PANEL.1 + g.tall.max(0.0)) * g.k + 2.0));
}
fn on_main(f: impl FnOnce() + Send + 'static) { if let Some(a) = APP.get() { let _ = a.run_on_main_thread(f); } }
fn redraw(now: bool) { if let Some(g) = geom() { on_main(move || apply(g, now)); } }

/// Shows the page as the panel opens. A hidden WKWebView runs no animation frames, so the page only says it is ready
/// (panel_ready) once shown: the window shows at once (transparent until then) and its contents fade in when ready.
fn show_page() {
    if !geom().is_some_and(|g| g.open) { return; }
    let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) else { return };
    // Shown again if it is not on screen, whatever the flag says (a fold-back that raced an opening once left it hidden).
    if !PAGE_SHOWN.swap(true, Ordering::SeqCst) || !w.is_visible().unwrap_or(true) {
        note("page shown");
        let _ = w.show();
        let _ = w.set_focus();
        // With another app active, show() leaves the page behind it (macOS won't hand Sora the focus on its own).
        crate::platform::show_on_top(&w);
        // If the page never says it is ready (no frame drawn, the call lost), show its contents anyway.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(900));
            if PAGE_SHOWN.load(Ordering::SeqCst) && !PAGE_READY.load(Ordering::SeqCst) {
                note("page not ready after 900 ms: showing it anyway");
                if let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) { let _ = w.emit("panel-show", ()); }
            }
        });
    }
    if PAGE_READY.load(Ordering::SeqCst) { let _ = w.emit("panel-show", ()); }
}
/// A line in ~/Library/Logs/Sora/notch.log, for what only a real Mac shows (where the notch is drawn, the page's load).
fn note(line: &str) {
    use std::io::Write;
    let Some(home) = std::env::var_os("HOME") else { return };
    let dir = std::path::Path::new(&home).join("Library/Logs/Sora");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("notch.log")) { let _ = writeln!(f, "{} {line}", now_ms()); }
}
fn expand(tab: Option<&str>) {
    let Some(app) = APP.get() else { return };
    if let Some(t) = tab { crate::desktop::want_tab(app, t); }
    let was = geom().is_some_and(|g| g.open);
    if update(|g| { g.open = true; g.peek = false; g.visible = true; }).is_none() { return; }
    OPENS.fetch_add(1, Ordering::SeqCst);
    redraw(false);
    if was { if let Some(t) = tab { let _ = app.emit_to("panel", "panel-tab", t); } return; }
    // The page fades in once the shape is about nine tenths open (its spring), so nothing shows outside the black.
    std::thread::spawn(|| { std::thread::sleep(Duration::from_millis(160)); show_page(); });
}
fn collapse() {
    if !geom().is_some_and(|g| g.open) { return; }
    if let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) { let _ = w.emit("panel-hide", ()); }
    // The page's contents shrink and fade first; then it goes, and the shape folds back, unless it was opened again
    // in the meantime (that used to hide the page of the panel just opened: a black, empty panel).
    let opens = OPENS.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(70));
        if OPENS.load(Ordering::SeqCst) != opens { return; }
        PAGE_SHOWN.store(false, Ordering::SeqCst);
        if let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) { let _ = w.hide(); }
        update(|g| { g.open = false; g.tall = 0.0; });
        redraw(false);
        if let Some(app) = APP.get() { crate::desktop::collapsed(app); }
    });
}

/// The pointer and the buttons, every 30 ms: hover swells the mini notch, a click or a file dragged onto it opens the
/// panel, a click outside the open panel closes it.
fn watch() {
    let (mut hover_since, mut drag_since, mut pressed_outside) = (None::<Instant>, None::<Instant>, false);
    let mut clicks = crate::platform::left_clicks();
    let mut outside = OUTSIDE_CLICKS.load(Ordering::SeqCst);
    loop {
        std::thread::sleep(Duration::from_millis(16));
        let Some(g) = geom() else { continue };
        if !g.visible { continue; }
        let Some((x, y)) = crate::platform::cursor_points() else { continue };
        let down = crate::platform::mouse_down()[0];
        let s = shape_of(&g);
        let inside = on_shape(s, x, y);
        // The face follows the pointer only while it is on the small notch itself (not across the open panel's area).
        let near = !g.open && inside;
        if let Ok(mut l) = LOOK.lock() { *l = near.then(|| (((x - s.0) / (s.1 / 2.0).max(1.0)).clamp(-1.0, 1.0), ((y - s.2 / 2.0) / (s.2 / 2.0).max(1.0)).clamp(-1.0, 1.0))); }
        // The system's click count, not the button now: a quick tap between two looks still counts.
        // Clicks in another app also come from the global monitor (the count alone missed some).
        let (now, away) = (crate::platform::left_clicks(), OUTSIDE_CLICKS.load(Ordering::SeqCst));
        let pressed = now != clicks || away != outside;
        (clicks, outside) = (now, away);
        if pressed { pressed_outside = !inside; }
        if g.open {
            // A click anywhere else (another app, the desktop) folds it back; the pointer only passing by does not. Not
            // while a file is dragged or Sora's own file dialog is up (clicks in it are Sora's, outside the panel).
            if pressed && !inside && !crate::desktop::DRAG_ACTIVE.load(Ordering::SeqCst) && !DIALOG.load(Ordering::SeqCst) { collapse(); }
            hover_since = None;
            drag_since = None;
            continue;
        }
        if inside {
            let since = *hover_since.get_or_insert_with(Instant::now);
            if !g.peek && since.elapsed() > Duration::from_millis(50) { update(|g| g.peek = true); redraw(false); }
            if pressed { expand(None); continue; }
            // A file dragged from elsewhere and held over the notch: open the Tray to drop it in.
            if down && pressed_outside {
                if drag_since.get_or_insert_with(Instant::now).elapsed() > Duration::from_millis(280) { drag_since = None; expand(Some("shelf")); }
            } else { drag_since = None; }
        } else {
            hover_since = None;
            drag_since = None;
            if g.peek { update(|g| g.peek = false); redraw(false); }
        }
    }
}

/// A file dialog of Sora's opens (true) or closes (false). The panel stays open (folding it away looked like an error);
/// the notch and its page go down to the floating level while the dialog is up, so the dialog (a modal panel, level 8)
/// comes in front of them, and back over the menu bar afterwards.
pub fn dialog(open: bool) {
    DIALOG.store(open, Ordering::SeqCst);
    let (notch, page) = if open { (3isize, 4isize) } else { (25, 26) };
    on_main(move || {
        UI.with(|ui| if let Some(ui) = ui.borrow().as_ref() { ui.panel.setLevel(notch); });
        if let Some(w) = APP.get().and_then(|a| a.get_webview_window("panel")) {
            if let Ok(ns) = w.ns_window() { unsafe { let _: () = objc2::msg_send![ns as *mut AnyObject, setLevel: page]; } }
        }
    });
}
/// The panel page, loaded once and kept: index.html in panel mode, transparent, above the shape.
fn build_page(app: &tauri::AppHandle, g: Geom) {
    let url = format!("index.html?panel=1&scale={:.4}&tab=home", g.k);
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(w) = tauri::WebviewWindowBuilder::new(&app, "panel", tauri::WebviewUrl::App(url.into())).title("Sora")
            .inner_size((PANEL.0 + 2.0 * EAR) * g.k, (PANEL.1 + TALLEST) * g.k).decorations(false).transparent(true).shadow(false)
            .resizable(false).always_on_top(true).skip_taskbar(true).visible(false)
            .on_page_load(|_, p| note(&format!("page {:?} {}", p.event(), p.url())))
            .build() else { note("page window failed"); return };
        let page = w.clone();
        let _ = w.run_on_main_thread(move || {
            crate::platform::hide_frame(&page);
            // One level above the shape it sits on.
            if let Ok(ns) = page.ns_window() { unsafe { let _: () = objc2::msg_send![ns as *mut AnyObject, setLevel: 26isize]; } }
        });
        place_page(&g);
    });
}
fn build(app: &tauri::AppHandle) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some((width, inset, camera, scale)) = display(mtm) else { return };
    let (_, _, size) = crate::desktop::notch_layout(app);
    let k = size_scale(size).min((width - 24.0) / (PANEL.0 + 2.0 * EAR));
    let g = Geom { width, inset, camera, scale, k, open: false, peek: false, tall: 0.0, visible: true, busy: false };
    note(&format!("display: {width} points wide, menu bar {inset}, camera housing {camera}"));
    std::thread::spawn(|| if let Some(m) = crate::platform::machine() { note(&format!("this Mac: {m}")) });
    *GEOM.lock().unwrap_or_else(|e| e.into_inner()) = Some(g);
    let Some(screen) = NSScreen::screens(mtm).firstObject() else { return };
    let f = screen.frame();
    let height = inset + (PANEL.1 + TALLEST) * k + 40.0;
    let rect = NSRect::new(NSPoint::new(f.origin.x, f.origin.y + f.size.height - height), NSSize::new(f.size.width, height));
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(NSPanel::alloc(mtm), rect, NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel, NSBackingStoreType::Buffered, false);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setIgnoresMouseEvents(true);
    // NSStatusWindowLevel: over the menu bar and full-screen apps, like the menu bar's own items.
    panel.setLevel(25);
    panel.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::IgnoresCycle | NSWindowCollectionBehavior::FullScreenAuxiliary);
    unsafe { panel.setReleasedWhenClosed(false) };
    let Some(view) = panel.contentView() else { return };
    // A layer-hosting view: the layer is ours (set before wantsLayer), AppKit leaves its tree alone. Bottom-up, unflipped.
    let layer = CALayer::new();
    layer.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(f.size.width as CGFloat, height as CGFloat)));
    view.setLayer(Some(&layer));
    view.setWantsLayer(true);
    note(&format!("build: screen {}x{} inset {inset} camera {camera} k {k:.3} panel height {height:.1}", f.size.width, f.size.height));
    let shape = CAShapeLayer::new();
    shape.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(f.size.width as CGFloat, height as CGFloat)));
    // Pure black, as the camera housing itself: the island and the housing read as one.
    shape.setFillColor(Some(&CGColor::new_srgb(0.0, 0.0, 0.0, 1.0)));
    layer.addSublayer(&shape);
    let content = content(scale);
    layer.addSublayer(&content.all);
    panel.orderFrontRegardless();
    // Clicks that go to other apps, seen at once (global monitors see mouse clicks without any permission).
    let seen = block2::RcBlock::new(|_: std::ptr::NonNull<objc2_app_kit::NSEvent>| { OUTSIDE_CLICKS.fetch_add(1, Ordering::SeqCst); });
    let mask = objc2_app_kit::NSEventMask::LeftMouseDown | objc2_app_kit::NSEventMask::RightMouseDown | objc2_app_kit::NSEventMask::OtherMouseDown;
    // Kept for the life of Sora.
    std::mem::forget(objc2_app_kit::NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &seen));
    UI.with(|ui| *ui.borrow_mut() = Some(Ui { panel, root: layer, shape, content, top: height, shown: std::cell::Cell::new(true) }));
    apply(g, true);
    build_page(app, g);
    if let Ok(dir) = app.path().app_data_dir() {
        let file = dir.join("timer-deadline.json");
        if let Some(end) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Option<i64>>(&b).ok()).flatten() { if let Ok(mut d) = DEADLINE.lock() { *d = Some(end); } }
        let _ = DEADLINE_FILE.set(file);
    }
    std::thread::spawn(watch);
    std::thread::spawn(tick);
    std::thread::spawn(animate);
}

/// Whether the main display has a camera notch the mini notch wraps.
pub fn camera_notch() -> bool { geom().is_some_and(|g| g.camera > 0.0) }
/// None: reminders show in their own window under the notch (notices.rs).
/// The LocalSend notice is a window here (notice.js follows the transfer by its event).
pub fn receive_progress() {}
pub fn notify(_kind: &str, _label: &str, _title: &str, _detail: &str, _target: &str, _picture: Option<crate::draw::Picture>) -> Option<Receiver<()>> { None }
pub fn capsule_rect() -> Option<(i32, i32, i32, i32)> { None }
/// The notch's centre x and bottom y on screen (physical px), for reminders that hang under it.
pub fn notch_geometry() -> Option<(i32, i32)> {
    let g = geom()?;
    let s = shape_of(&g);
    Some(((s.0 * g.scale).round() as i32, (s.2 * g.scale).round() as i32))
}
/// The displays for Settings → Notch → Display. shortcut: the notch always uses the main display on macOS.
pub fn monitors() -> Vec<(String, String, bool)> {
    let Some(mtm) = MainThreadMarker::new() else { return vec![("primary".into(), "Main display".into(), true)] };
    NSScreen::screens(mtm).iter().enumerate().map(|(i, s)| (i.to_string(), s.localizedName().to_string(), i == 0)).collect()
}
/// The native notch is the default; `--web-notch` or SORA_NATIVE_NOTCH=0 brings back the all-web one.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !std::env::args().any(|a| a == "--web-notch") && std::env::var("SORA_NATIVE_NOTCH").as_deref() != Ok("0"))
}
/// Shown again (the shortcut, a timer, Sora opened again): the mini notch swells for a moment.
pub fn peek() {
    if update(|g| { g.visible = true; g.peek = true; }).is_none() { return; }
    redraw(false);
    std::thread::spawn(|| { std::thread::sleep(Duration::from_millis(1600)); if update(|g| g.peek = false).is_some() { redraw(false); } });
}
pub fn set_visible(visible: bool) {
    if !visible { collapse(); }
    if update(|g| g.visible = visible).is_some() { redraw(true); }
}
pub fn drag_offer() {}
pub fn drag_ended() {}
pub fn show_send() { expand(Some("send")); }
/// Ctrl+Option+A: the Agents tab.
pub fn show_agents() { expand(Some("agents")); }
/// The Size changed: the page is rebuilt at the new scale.
pub fn relayout() {
    let Some(app) = APP.get().cloned() else { return };
    let (_, _, size) = crate::desktop::notch_layout(&app);
    let Some(g) = update(|g| { g.k = size_scale(size).min((g.width - 24.0) / (PANEL.0 + 2.0 * EAR)); g.open = false; }) else { return };
    PAGE_READY.store(false, Ordering::SeqCst);
    PAGE_SHOWN.store(false, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("panel") { let _ = w.destroy(); }
    redraw(false);
    build_page(&app, g);
}
pub fn preview_size(size: Option<u32>) {
    let Some(app) = APP.get() else { return };
    let size = size.unwrap_or_else(|| crate::desktop::notch_layout(app).2);
    if update(|g| g.k = size_scale(size).min((g.width - 24.0) / (PANEL.0 + 2.0 * EAR))).is_some() { redraw(false); }
}
pub fn tab_visible(name: &str) -> bool { PAGE_SHOWN.load(Ordering::SeqCst) && TAB.lock().map_or(false, |t| t.as_str() == name || t.strip_prefix(name).is_some_and(|r| r.starts_with('-'))) }
/// The meeting recorder runs: the red dot stays in the right wing until it stops (the ticker shows it).
pub fn set_recording(on: bool) { RECORDING.store(on, Ordering::SeqCst); }
pub fn start(app: &tauri::AppHandle) {
    let _ = APP.set(app.clone());
    let a = app.clone();
    let _ = app.run_on_main_thread(move || build(&a));
}
/// The page has drawn: show it if the panel is already open.
#[tauri::command]
pub fn panel_ready() { note("page ready"); PAGE_READY.store(true, Ordering::SeqCst); show_page(); }
#[tauri::command]
pub fn panel_close() { collapse(); }
/// A view that needs more room (a Home menu, an agent's question): the panel grows by `px` CSS px.
#[tauri::command]
pub fn panel_tall(px: f64) { if update(|g| g.tall = px.clamp(0.0, TALLEST)).is_some() { redraw(false); } }
/// The page switched views: the panel takes that view's height, as on Windows (TALL for a Home menu or History; a view
/// that sizes itself says so through panel_tall).
#[tauri::command]
pub fn panel_view(name: String) {
    let tall = view_tall(&name);
    if let Ok(mut t) = TAB.lock() { *t = name; }
    if let Some(px) = tall { if geom().is_some_and(|g| g.tall != px) && update(|g| g.tall = px).is_some() { redraw(false); } }
}
/// How much taller than normal a view's panel is (CSS px), or None for a view that sets it itself (pure; tested).
fn view_tall(name: &str) -> Option<f64> {
    match name {
        "history" | "meeting-detail" => Some(TALL),
        "agents-question" | "board-preview" | "send-sheet" => None,
        n if n.ends_with("-editor") => Some(TALL),
        _ => Some(0.0),
    }
}
/// The focus timer's end: the right wing counts down, and the notch chimes when it is over (page open or not).
#[tauri::command]
pub fn timer_deadline(end: Option<i64>) { set_deadline(end) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_notch_clears_the_camera() {
        // A 14" MacBook Pro: 1512 points wide, camera housing 186 x 32 points.
        let g = Geom { width: 1512.0, inset: 32.0, camera: 186.0, scale: 2.0, k: 0.85, open: false, peek: false, tall: 0.0, visible: true, busy: false };
        let (cx, w, h, r, _) = shape_of(&g);
        assert!(cx == 756.0 && w == 186.0 + 2.0 * wing(&g) && h == 32.0 && r == CAMERA_RADIUS, "the camera notch with a wing on each side, in its height");
        assert_eq!(shape_of(&Geom { busy: true, peek: true, ..g }), shape_of(&g), "always the same, whatever plays");
        assert!(!mascot_shows(&g), "no face beside a camera");
        let s = shape_of(&g);
        assert!(on_shape(s, 756.0, 10.0) && on_shape(s, 756.0 + 150.0, 10.0) && !on_shape(s, 756.0 + 210.0, 10.0) && !on_shape(s, 756.0, 33.0), "the click area is the island");
        let open = shape_of(&Geom { open: true, ..g });
        assert_eq!(open.2, 32.0 + 234.0 * 0.85, "the panel's page starts below the camera");
        assert!(on_shape(open, 756.0, 100.0) && !on_shape(open, 100.0, 100.0));
        // The face and the timer sit beside the camera housing (663..849), in its band, never over it.
        let (lx, rx, y, band) = wings(&g);
        let half = wing(&g) / 2.0;
        assert!(half > 30.0, "wings wide enough for the clock and the date");
        assert!(lx + half <= 663.0 + 1e-9 && rx - half >= 849.0 - 1e-9 && y == 16.0 && band == 32.0, "{lx} {rx}");
    }
    #[test]
    fn a_mac_without_a_notch_centres_the_face() {
        // A 14" MacBook Pro (camera notch, 32-point menu bar) and an M1 MacBook Air (no notch, 24-point menu bar).
        let pro = Geom { width: 1512.0, inset: 32.0, camera: 186.0, scale: 2.0, k: 0.85, open: false, peek: false, tall: 0.0, visible: true, busy: false };
        let air = Geom { width: 1440.0, inset: 24.0, camera: 0.0, ..pro };
        let b = shape_of(&air);
        assert_eq!((b.2, b.3, b.4), (24.0, 7.5, 4.5), "a capsule as tall as the menu bar, with Windows' corners and ears");
        let small = Geom { k: 0.85 * 0.85, ..air };
        assert!(shape_of(&small).1 < b.1 && shape_of(&small).2 == 24.0, "Small: shorter, as tall");
        assert_eq!(shape_of(&Geom { inset: 0.0, ..air }).2, MIN_BAND, "still there with the menu bar hidden");
        assert_eq!((b.0, wings(&air).0), (720.0, 720.0), "the capsule and the face in the middle");
        // A timer running: the island widens, the face steps left and the timer sits right, both inside it.
        let busy = Geom { busy: true, ..air };
        let (s, (l, r, ..)) = (shape_of(&busy), wings(&busy));
        let (half, wing) = (s.1 / 2.0, wing(&busy) / 2.0);
        assert!(s.1 > b.1 && l < 720.0 && r > 720.0 && l - wing > 720.0 - half && r + wing < 720.0 + half, "{l} {r} {half}");
        assert!(mascot_shows(&air) && !mascot_shows(&busy), "the clock and the status take the face's place, it never moves");
        assert_eq!(wings(&Geom { busy: true, ..pro }), wings(&pro), "beside a camera nothing moves either");
        assert_eq!(short_time("h:mm a"), "h:mm");
        assert_eq!(short_time("HH.mm"), "HH.mm");
    }
    #[test]
    fn each_view_gets_its_height() {
        assert_eq!(view_tall("timer-editor"), Some(TALL));
        assert_eq!(view_tall("history"), Some(TALL));
        assert_eq!(view_tall("board-preview"), None, "it says its own height");
        assert_eq!(view_tall("music"), Some(0.0), "back from a taller view");
    }
    #[test]
    fn the_right_wing() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
        assert_eq!(clock(400), "0:01");
        assert_eq!(right_for(true, Some(9_000), true), Right::Recording);
        assert_eq!(right_for(false, Some(9_000), true), Right::Timer("0:09".into()));
        assert_eq!(right_for(false, Some(-5), true), Right::Music);
        assert_eq!(right_for(false, None, false), Right::Nothing);
    }
}
