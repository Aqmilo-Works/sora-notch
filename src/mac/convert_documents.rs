//! The macOS side of convert_documents (convert_documents.rs), its `os` module. Everything else there is shared.
//! PDF pages are drawn with CoreGraphics' own PDF renderer (what Preview uses), on white, then written as PNG.
#![allow(unused_imports)]
use super::*;
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint { x: f64, y: f64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize { width: f64, height: f64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect { origin: CGPoint, size: CGSize }
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(alloc: *const c_void, buffer: *const u8, len: isize, is_dir: bool) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPDFDocumentCreateWithURL(url: *const c_void) -> *mut c_void;
    fn CGPDFDocumentRelease(doc: *mut c_void);
    fn CGPDFDocumentIsEncrypted(doc: *mut c_void) -> bool;
    fn CGPDFDocumentIsUnlocked(doc: *mut c_void) -> bool;
    fn CGPDFDocumentGetNumberOfPages(doc: *mut c_void) -> usize;
    fn CGPDFDocumentGetPage(doc: *mut c_void, number: usize) -> *mut c_void;
    fn CGPDFPageGetBoxRect(page: *mut c_void, kind: i32) -> CGRect;
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(data: *mut c_void, width: usize, height: usize, bits: usize, row: usize, space: *mut c_void, info: u32) -> *mut c_void;
    fn CGContextRelease(ctx: *mut c_void);
    fn CGContextSetRGBFillColor(ctx: *mut c_void, r: f64, g: f64, b: f64, a: f64);
    fn CGContextFillRect(ctx: *mut c_void, rect: CGRect);
    fn CGContextScaleCTM(ctx: *mut c_void, sx: f64, sy: f64);
    fn CGContextTranslateCTM(ctx: *mut c_void, tx: f64, ty: f64);
    fn CGContextDrawPDFPage(ctx: *mut c_void, page: *mut c_void);
}
/// kCGPDFCropBox: the page as a viewer shows it. kCGImageAlphaPremultipliedLast: RGBA bytes.
const CROP_BOX: i32 = 1;
const RGBA: u32 = 1;

/// One page as PNG, the long side at most 2400 px and at most twice its size (as on Windows).
fn render_png(page: *mut c_void) -> Option<Vec<u8>> {
    unsafe {
        let b = CGPDFPageGetBoxRect(page, CROP_BOX);
        let scale = (2400.0 / b.size.width.max(b.size.height).max(1.0)).min(2.0);
        let (w, h) = ((b.size.width * scale).round().max(1.0) as usize, (b.size.height * scale).round().max(1.0) as usize);
        let mut px = vec![0u8; w * h * 4];
        let space = CGColorSpaceCreateDeviceRGB();
        let ctx = CGBitmapContextCreate(px.as_mut_ptr().cast(), w, h, 8, w * 4, space, RGBA);
        CGColorSpaceRelease(space);
        if ctx.is_null() { return None; }
        CGContextSetRGBFillColor(ctx, 1.0, 1.0, 1.0, 1.0);
        CGContextFillRect(ctx, CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: w as f64, height: h as f64 } });
        CGContextScaleCTM(ctx, scale, scale);
        CGContextTranslateCTM(ctx, -b.origin.x, -b.origin.y);
        CGContextDrawPDFPage(ctx, page);
        CGContextRelease(ctx);
        let img = image::RgbaImage::from_raw(w as u32, h as u32, px)?;
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).ok()?;
        Some(png.into_inner())
    }
}

pub(in crate::convert) fn pdf_pages(source: &Path, dest: &Path, progress: crate::convert::Progress<'_>) -> AppResult<()> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = source.as_os_str().as_bytes();
    unsafe {
        let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bytes.as_ptr(), bytes.len() as isize, false);
        if url.is_null() { return Err("macOS could not open this PDF.".into()); }
        let doc = CGPDFDocumentCreateWithURL(url);
        CFRelease(url);
        if doc.is_null() { return Err("macOS could not open this PDF.".into()); }
        struct Doc(*mut c_void);
        impl Drop for Doc { fn drop(&mut self) { unsafe { CGPDFDocumentRelease(self.0) } } }
        let doc = Doc(doc);
        if CGPDFDocumentIsEncrypted(doc.0) && !CGPDFDocumentIsUnlocked(doc.0) { return Err("This PDF needs a password.".into()); }
        let pages = CGPDFDocumentGetNumberOfPages(doc.0);
        if pages == 0 { return Err("This PDF has no pages.".into()); }
        let mut zip = ZipWriter::new(BufWriter::new(File::create(dest).map_err(err)?));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for i in 1..=pages {
            let page = CGPDFDocumentGetPage(doc.0, i);
            let png = if page.is_null() { None } else { render_png(page) }.ok_or("A page could not be drawn.")?;
            zip.start_file(format!("page-{i:04}.png"), options).map_err(err)?;
            zip.write_all(&png).map_err(err)?;
            progress((5 + 90 * i / pages) as u8, "Rendering pages");
        }
        zip.finish().map_err(err)?.flush().map_err(err)
    }
}
