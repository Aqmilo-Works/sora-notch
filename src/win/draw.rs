//! Drawing helpers shared by the native notch (notch_ui.rs): springs and easing, colours and transforms, SVG paths,
//! the bundled Poppins font, and decoded pictures (avatars, album art).
use std::sync::atomic::Ordering;
use windows::{core::Interface, Foundation::Numerics::Matrix3x2,
    Win32::{Foundation::E_FAIL, Graphics::{Direct2D::{Common::*, *}, DirectWrite::*, Imaging::*}, System::Com::*, UI::Shell::SHCreateMemStream}};

/// A decoded picture: BGRA rows top-down, premultiplied alpha.
pub struct Picture { pub width: u32, pub height: u32, pub bgra: Vec<u8> }

/// Runs f with Windows Imaging ready on this thread.
fn with_wic<T>(f: impl FnOnce(&IWICImagingFactory) -> windows::core::Result<T>) -> Result<T, String> {
    unsafe {
        let started = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).and_then(|wic: IWICImagingFactory| f(&wic));
        if started { CoUninitialize(); }
        result.map_err(|e| e.to_string())
    }
}
/// Resamples a picture once, with a good filter, so its longer side is exactly px device pixels and it can be drawn 1:1
/// (a linear filter shrinking 128 px into 26 makes pictures look broken).
unsafe fn fit(wic: &IWICImagingFactory, source: &IWICBitmapSource, px: u32) -> windows::core::Result<Picture> {
    let (mut w, mut h) = (0, 0);
    source.GetSize(&mut w, &mut h)?;
    let k = px.max(1) as f32 / w.max(h).max(1) as f32;
    let (tw, th) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
    let scaler = wic.CreateBitmapScaler()?;
    scaler.Initialize(source, tw, th, if k < 1.0 { WICBitmapInterpolationModeHighQualityCubic } else { WICBitmapInterpolationModeCubic })?;
    let converter = wic.CreateFormatConverter()?;
    converter.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
    let mut bgra = vec![0u8; (tw * th * 4) as usize];
    converter.CopyPixels(std::ptr::null(), tw * 4, &mut bgra)?;
    Ok(Picture { width: tw, height: th, bgra })
}
/// Decodes jpeg/png bytes (avatars, album art) to a premultiplied picture px device pixels on its longer side.
pub fn decode(bytes: &[u8], px: u32) -> Result<Picture, String> {
    with_wic(|wic| unsafe {
        let stream = SHCreateMemStream(Some(bytes)).ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let decoder = wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
        fit(wic, &decoder.GetFrame(0)?.cast()?, px)
    })
}

/// CSS cubic-bezier(x1, y1, x2, y2) at progress t.
pub(crate) fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    if t >= 1.0 { return 1.0; }
    let curve = |a: f32, b: f32, s: f32| 3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3);
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 { let mid = (lo + hi) / 2.0; if curve(x1, x2, mid) < t { lo = mid; } else { hi = mid; } }
    curve(y1, y2, (lo + hi) / 2.0)
}

// ---------------------------------------------------------------- springs
/// A spring: a new target keeps the current speed, so a stream of small changes reads as one glide.
#[derive(Clone, Copy)]
pub(crate) struct Spring { pub x: f32, pub v: f32, pub to: f32 }
impl Spring {
    pub fn new(x: f32) -> Self { Self { x, v: 0.0, to: x } }
    pub fn set(&mut self, to: f32) { self.to = to; }
    /// Moves it on by dt with its speed (omega) and damping ratio.
    pub fn step_with(&mut self, dt: f32, omega: f32, damping: f32) {
        // Small fixed sub-steps keep it stable whatever the frame time.
        let n = (dt / 0.004).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n { let a = -2.0 * damping * omega * self.v - omega * omega * (self.x - self.to); self.v += a * h; self.x += self.v * h; }
        if (self.x - self.to).abs() < 0.002 && self.v.abs() < 0.01 { self.x = self.to; self.v = 0.0; }
    }
    pub fn busy(&self) -> bool { self.x != self.to || self.v != 0.0 }
}

/// One SVG path command list, as absolute segments.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum Seg { Move(f32, f32), Line(f32, f32), Cubic(f32, f32, f32, f32, f32, f32), Quad(f32, f32, f32, f32), Arc(f32, f32, f32, bool, bool, f32, f32), Close }
/// A small SVG path parser (M L H V C S Q A Z and their relative forms): enough for line icons.
pub(crate) fn parse_path(d: &str) -> Vec<Seg> {
    let mut toks: Vec<String> = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, toks: &mut Vec<String>| if !num.is_empty() { toks.push(std::mem::take(num)); };
    for c in d.chars() {
        if c.is_ascii_alphabetic() && c != 'e' { flush(&mut num, &mut toks); toks.push(c.to_string()); }
        else if c == '-' && !num.ends_with('e') { flush(&mut num, &mut toks); num.push(c); }
        else if c == '.' && num.contains('.') { flush(&mut num, &mut toks); num.push(c); }
        else if c == ',' || c.is_whitespace() { flush(&mut num, &mut toks); }
        else { num.push(c); }
    }
    flush(&mut num, &mut toks);
    let mut out = Vec::new();
    let (mut x, mut y, mut sx, mut sy, mut i) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0usize);
    let mut cmd = 'M';
    let mut last_ctrl: Option<(f32, f32)> = None;
    while i < toks.len() {
        if toks[i].chars().next().map_or(false, |c| c.is_ascii_alphabetic()) { cmd = toks[i].chars().next().unwrap(); i += 1; if cmd == 'z' || cmd == 'Z' { out.push(Seg::Close); x = sx; y = sy; last_ctrl = None; continue; } }
        let mut n = || -> f32 { let v = toks.get(i).and_then(|t| t.parse().ok()).unwrap_or(0.0); i += 1; v };
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => { x = ox + n(); y = oy + n(); sx = x; sy = y; out.push(Seg::Move(x, y)); cmd = if rel { 'l' } else { 'L' }; last_ctrl = None; }
            'L' => { x = ox + n(); y = oy + n(); out.push(Seg::Line(x, y)); last_ctrl = None; }
            'H' => { x = if rel { x + n() } else { n() }; out.push(Seg::Line(x, y)); last_ctrl = None; }
            'V' => { y = if rel { y + n() } else { n() }; out.push(Seg::Line(x, y)); last_ctrl = None; }
            'C' => { let (a, b, c, d, e, f) = (ox + n(), oy + n(), ox + n(), oy + n(), ox + n(), oy + n()); out.push(Seg::Cubic(a, b, c, d, e, f)); last_ctrl = Some((c, d)); x = e; y = f; }
            'S' => { let (a, b) = last_ctrl.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy)); let (c, d, e, f) = (ox + n(), oy + n(), ox + n(), oy + n()); out.push(Seg::Cubic(a, b, c, d, e, f)); last_ctrl = Some((c, d)); x = e; y = f; }
            'Q' => { let (a, b, c, d) = (ox + n(), oy + n(), ox + n(), oy + n()); out.push(Seg::Quad(a, b, c, d)); x = c; y = d; last_ctrl = None; }
            'A' => { let (rx, ry, rot, large, sweep) = (n(), n(), n(), n() != 0.0, n() != 0.0); x = ox + n(); y = oy + n(); out.push(Seg::Arc(rx, ry, rot, large, sweep, x, y)); last_ctrl = None; }
            _ => { i += 1; }
        }
    }
    out
}


pub(crate) fn color(hex: u32, alpha: f32) -> D2D1_COLOR_F { D2D1_COLOR_F { r: ((hex >> 16) & 255) as f32 / 255.0, g: ((hex >> 8) & 255) as f32 / 255.0, b: (hex & 255) as f32 / 255.0, a: alpha } }
pub(crate) fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F { D2D_RECT_F { left: l, top: t, right: r, bottom: b } }
pub(crate) fn pt(x: f32, y: f32) -> D2D_POINT_2F { D2D_POINT_2F { x, y } }
pub(crate) fn wide(s: &str) -> Vec<u16> { s.encode_utf16().collect() }
pub(crate) fn mat(m11: f32, m12: f32, m21: f32, m22: f32, dx: f32, dy: f32) -> Matrix3x2 { Matrix3x2 { M11: m11, M12: m12, M21: m21, M22: m22, M31: dx, M32: dy } }
/// a then b (row vectors, as Direct2D uses them).
pub(crate) fn mul(a: Matrix3x2, b: Matrix3x2) -> Matrix3x2 {
    mat(a.M11 * b.M11 + a.M12 * b.M21, a.M11 * b.M12 + a.M12 * b.M22, a.M21 * b.M11 + a.M22 * b.M21, a.M21 * b.M12 + a.M22 * b.M22, a.M31 * b.M11 + a.M32 * b.M21 + b.M31, a.M31 * b.M12 + a.M32 * b.M22 + b.M32)
}
pub(crate) fn translate(x: f32, y: f32) -> Matrix3x2 { mat(1.0, 0.0, 0.0, 1.0, x, y) }
pub(crate) fn rotate(deg: f32) -> Matrix3x2 { let (s, c) = deg.to_radians().sin_cos(); mat(c, s, -s, c, 0.0, 0.0) }
pub(crate) fn scale(x: f32, y: f32) -> Matrix3x2 { mat(x, 0.0, 0.0, y, 0.0, 0.0) }

// ---------------------------------------------------------------- fonts
const POPPINS: [&[u8]; 3] = [include_bytes!("../../ui/fonts/Poppins-Regular.woff2"), include_bytes!("../../ui/fonts/Poppins-Medium.woff2"), include_bytes!("../../ui/fonts/Poppins-SemiBold.woff2")];
/// Poppins from the bundled WOFF2 files (Windows 10 1703+ unpacks them); None means the system font is used instead.
pub(crate) unsafe fn poppins(dw: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let f5: IDWriteFactory5 = dw.cast().ok()?;
    let loader = f5.CreateInMemoryFontFileLoader().ok()?;
    f5.RegisterFontFileLoader(&loader).ok()?;
    let builder = f5.CreateFontSetBuilder().ok()?;
    for data in POPPINS {
        let stream = f5.UnpackFontFile(DWRITE_CONTAINER_TYPE_WOFF2, data.as_ptr() as _, data.len() as u32).ok()?;
        let size = stream.GetFileSize().ok()?;
        let (mut frag, mut ctx) = (std::ptr::null_mut(), std::ptr::null_mut());
        stream.ReadFileFragment(&mut frag, 0, size, &mut ctx).ok()?;
        let bytes = std::slice::from_raw_parts(frag as *const u8, size as usize).to_vec();
        stream.ReleaseFileFragment(ctx);
        // No owner object: DirectWrite copies the data.
        let file = loader.CreateInMemoryFontFileReference(&f5, bytes.as_ptr() as _, bytes.len() as u32, None).ok()?;
        builder.AddFontFile(&file).ok()?;
    }
    let set = builder.CreateFontSet().ok()?;
    f5.CreateFontCollectionFromFontSet(&set).ok()?.cast().ok()
}


pub(crate) unsafe fn path_geometry(d2d: &ID2D1Factory, segs: &[Seg]) -> Option<ID2D1PathGeometry> { build_geometry(d2d, segs, false) }
/// The same outline as a filled shape (a logo), filled by the non-zero rule like SVG.
pub(crate) unsafe fn fill_geometry(d2d: &ID2D1Factory, segs: &[Seg]) -> Option<ID2D1PathGeometry> { build_geometry(d2d, segs, true) }
unsafe fn build_geometry(d2d: &ID2D1Factory, segs: &[Seg], filled: bool) -> Option<ID2D1PathGeometry> {
    let g = d2d.CreatePathGeometry().ok()?;
    let sink = g.Open().ok()?;
    if filled { sink.SetFillMode(windows::Win32::Graphics::Direct2D::Common::D2D1_FILL_MODE_WINDING); }
    let (begin, end) = if filled { (D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED) } else { (D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN) };
    let mut open = false;
    for s in segs {
        match *s {
            Seg::Move(x, y) => { if open { sink.EndFigure(end); } sink.BeginFigure(pt(x, y), begin); open = true; }
            Seg::Line(x, y) => sink.AddLine(pt(x, y)),
            Seg::Cubic(a, b, c, d, e, f) => sink.AddBezier(&D2D1_BEZIER_SEGMENT { point1: pt(a, b), point2: pt(c, d), point3: pt(e, f) }),
            Seg::Quad(a, b, c, d) => sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: pt(a, b), point2: pt(c, d) }),
            Seg::Arc(rx, ry, rot, large, sweep, x, y) => sink.AddArc(&D2D1_ARC_SEGMENT { point: pt(x, y), size: D2D_SIZE_F { width: rx, height: ry }, rotationAngle: rot, sweepDirection: if sweep { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE }, arcSize: if large { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL } }),
            Seg::Close => { if open { sink.EndFigure(D2D1_FIGURE_END_CLOSED); open = false; } }
        }
    }
    if open { sink.EndFigure(end); }
    sink.Close().ok()?;
    Some(g)
}
pub(crate) fn rand() -> f32 {
    use std::sync::atomic::AtomicU64;
    static SEED: AtomicU64 = AtomicU64::new(0x9e3779b97f4a7c15);
    let mut x = SEED.load(Ordering::Relaxed) ^ (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() as u64);
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    SEED.store(x, Ordering::Relaxed);
    (x >> 40) as f32 / (1u64 << 24) as f32
}
