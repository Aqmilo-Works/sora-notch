use super::{add_paths, err, AppResult, Shelf};
use image::ImageReader;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use serde::Serialize;
use std::{fs, io::{BufReader, ErrorKind, Read, Seek}, path::{Path, PathBuf}, sync::Mutex};
use tauri::Manager;
// What differs per OS lives in win/convert.rs and mac/convert.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/convert.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/convert.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

#[path = "convert_documents.rs"]
mod documents;
#[path = "converter_engines.rs"]
pub(crate) mod engines;

#[derive(Serialize)]
pub struct Group { label: &'static str, formats: Vec<&'static str> }

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn groups(path: &Path) -> Vec<Group> {
    let mut list = all_groups(path);
    // A group none of whose formats this system can make is left out (macOS has no WebP or MP3 encoder built in).
    list.retain(|g| !g.formats.is_empty());
    list
}
fn all_groups(path: &Path) -> Vec<Group> {
    let ext = extension(path);
    let ext = match ext.as_str() { "jpg" | "jfif" => "jpeg", "tif" => "tiff", ext => ext };
    let group = |label, choices: &[&'static str]| Group {
        label, formats: choices.iter().copied().filter(|&format| format != ext && engines::can_make(ext, format)).collect(),
    };
    let audio = ["mp3", "m4a", "wav", "flac", "ogg", "opus"];
    match ext {
        "jpeg" | "png" | "gif" | "bmp" | "webp" | "tiff" | "ico" | "heic" | "heif" | "avif" | "jxl" => {
            let label = match ext { "gif" | "webp" | "tiff" | "avif" | "jxl" | "heic" | "heif" => "Still image · first frame", _ => "Save image as" };
            vec![group(label, &["jpeg", "png", "webp", "bmp", "tiff", "avif"]), group("Single-page document", &["pdf"])]
        }
        "mp4" | "m4v" | "mov" | "wmv" | "avi" | "mkv" | "webm" => {
            vec![group("Convert video", &["mp4", "mov", "mkv", "webm", "wmv"]), group("Extract audio", &audio)]
        }
        "mp3" | "m4a" | "aac" | "wav" | "wma" | "flac" | "ogg" | "oga" | "opus" => vec![group("Convert audio", &audio)],
        "pdf" => vec![group("All pages as PNG images", &["zip"])],
        "docx" => vec![group("Extract document text", &["txt"])],
        _ => vec![],
    }
}

#[tauri::command]
pub fn tray_conversion_options(state: tauri::State<Shelf>, path: String) -> AppResult<Vec<Group>> {
    let items = state.0.lock().map_err(err)?;
    let item = items.iter().find(|f| f.path == path).ok_or("File is no longer in the tray")?;
    Ok(if item.directory || !item.available { vec![] } else { groups(Path::new(&path)) })
}

fn image_pdf(width: u32, height: u32, jpeg: &[u8]) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let (root, pages, page, image, stream) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4), Ref::new(5));
    let name = Name(b"Photo");
    let scale = 0.75_f32.min(14400.0 / width.max(height) as f32);
    let (w, h) = (width as f32 * scale, height as f32 * scale);
    pdf.catalog(root).pages(pages);
    pdf.pages(pages).kids([page]).count(1);
    let mut sheet = pdf.page(page);
    sheet.parent(pages).media_box(Rect::new(0.0, 0.0, w, h)).contents(stream);
    sheet.resources().x_objects().pair(name, image);
    sheet.finish();
    let mut photo = pdf.image_xobject(image, jpeg);
    photo.filter(Filter::DctDecode);
    photo.width(width as i32).height(height as i32).bits_per_component(8);
    photo.color_space().device_rgb();
    photo.finish();
    let mut draw = Content::new();
    draw.transform([w, 0.0, 0.0, h, 0.0, 0.0]).x_object(name);
    pdf.stream(stream, &draw.finish());
    pdf.finish()
}

type Progress<'a> = &'a dyn Fn(u8, &str);
fn write_image_pdf(cache: &Path, source: &Path, output: &Path, progress:Progress<'_>) -> AppResult<()> {
    let mut jpeg = tempfile::tempfile_in(output.parent().ok_or("No output folder")?).map_err(err)?;
    engines::image(cache, source, jpeg.try_clone().map_err(err)?, "jpeg",progress)?;
    jpeg.rewind().map_err(err)?;
    let size = ImageReader::with_format(BufReader::new(&jpeg), image::ImageFormat::Jpeg).into_dimensions().map_err(err)?;
    jpeg.rewind().map_err(err)?;
    let mut data = Vec::new();
    jpeg.read_to_end(&mut data).map_err(err)?;
    fs::write(output, image_pdf(size.0, size.1, &data)).map_err(err)
}

#[cfg(test)]
fn convert(cache:&Path,source:&Path,format:&str)->AppResult<PathBuf>{convert_with_progress(cache,source,format,&|_,_|{})}
fn convert_with_progress(cache:&Path,source:&Path,format:&str,progress:Progress<'_>)->AppResult<PathBuf>{
    progress(1,"Preparing");
    if !source.is_file() { return Err("The file was moved, deleted or cannot be read.".into()); }
    if !groups(source).iter().any(|g| g.formats.contains(&format)) { return Err("This conversion is unavailable for this file type.".into()); }
    let dir = source.parent().ok_or("Source folder unavailable")?;
    let stem = source.file_stem().ok_or("Invalid file name")?;
    let ext = if format == "jpeg" { "jpg" } else { format };
    let mut temp = tempfile::Builder::new().prefix(".sora-").suffix(&format!(".{ext}")).tempfile_in(dir)
        .map_err(|e| format!("Could not create a file in the source folder: {e}"))?.into_temp_path();
    match format {
        "mp4" | "mov" | "mkv" | "webm" | "wmv" | "mp3" | "m4a" | "wav" | "flac" | "ogg" | "opus" =>
            engines::media(cache, source, &temp, format,progress)?,
        "zip" => documents::pdf_pages(source, &temp,progress)?,
        "txt" => documents::docx_text(source, &temp,progress)?,
        "pdf" => write_image_pdf(cache, source, &temp,progress)?,
        _ => engines::image(cache, source, fs::File::create(&temp).map_err(err)?, format,progress)?,
    }
    progress(96,"Saving");
    if fs::metadata(&temp).map_err(err)?.len() == 0 { return Err("The encoder produced an empty file.".into()); }
    for n in 1.. {
        let mut name = stem.to_os_string();
        if n > 1 { name.push(format!(" ({n})")); }
        name.push(format!(".{ext}"));
        let dest = dir.join(name);
        match temp.persist_noclobber(&dest) {
            Ok(()) => return Ok(dest),
            Err(e) if e.error.kind() == ErrorKind::AlreadyExists => temp = e.path,
            Err(e) => return Err(format!("Could not save the converted file: {}", e.error)),
        }
    }
    unreachable!()
}

#[derive(Serialize)]
pub struct Converted { name: String, added: bool }

// Limit concurrent decoding so large files cannot multiply peak memory usage.
static JOB: Mutex<()> = Mutex::new(());

#[tauri::command]
pub async fn tray_convert(app: tauri::AppHandle, path: String, format: String) -> AppResult<Converted> {
    tauri::async_runtime::spawn_blocking(move || {
        let _job = JOB.try_lock().map_err(|_| "Another conversion is running.")?;
        if !app.state::<Shelf>().0.lock().map_err(err)?.iter().any(|f| f.path == path && !f.directory) {
            return Err("File is no longer in the tray.".into());
        }
        let cache = app.path().app_cache_dir().map_err(err)?.join("converter");
        let previous=std::cell::Cell::new(0);
        let progress=|percent:u8,stage:&str|{if percent>previous.get(){previous.set(percent);let _=tauri::Emitter::emit(&app,"convert-progress",serde_json::json!({"path":path,"percent":percent,"stage":stage}));}};
        let dest=convert_with_progress(&cache,Path::new(&path),&format,&progress)?;progress(100,"Saved");
        let added = add_paths(&app, vec![dest.clone()]).is_ok_and(|items| items.iter().any(|f| Path::new(&f.path) == dest));
        Ok(Converted { name: dest.file_name().unwrap_or_default().to_string_lossy().into_owned(), added })
    }).await.map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(not(win), ignore = "needs the bundled engines, which are Windows programs for now")]
    fn bundled_ffmpeg_reports_processed_media_time() {
        let dir=tempfile::tempdir().unwrap();
        let rate=44100u32;let data_len=rate*2*5;
        let mut wav=Vec::new();
        wav.extend(b"RIFF");wav.extend((36+data_len).to_le_bytes());wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());wav.extend(1u16.to_le_bytes());wav.extend(1u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());wav.extend((rate*2).to_le_bytes());wav.extend(2u16.to_le_bytes());wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");wav.extend(data_len.to_le_bytes());wav.resize(wav.len()+data_len as usize,0);
        let source=dir.path().join("tone.wav");fs::write(&source,wav).unwrap();
        let measured=std::cell::Cell::new(0u8);
        let output=convert_with_progress(&dir.path().join("cache"),&source,"mp3",&|n,stage|{if stage=="Encoding"{measured.set(n)}}).unwrap();
        assert!(measured.get()>5,"missing FFmpeg progress timestamps");assert!(fs::metadata(output).unwrap().len()>0);
    }

    #[test]
    fn available_formats_follow_the_source_type() {
        let jpg = groups(Path::new("photo.JpG"));
        assert!(!jpg[0].formats.contains(&"jpeg"));
        assert!(jpg[1].formats.contains(&"pdf"));
        assert!(groups(Path::new("a.mp4"))[0].formats.contains(&"mov") && !groups(Path::new("a.mov"))[0].formats.contains(&"mov"));
        let video = groups(Path::new("clip.MOV"));
        assert_eq!(video[1].label, "Extract audio");
        // macOS has no MP3 encoder built in: its audio comes out as M4A.
        assert!(video[1].formats.contains(if cfg!(win) { &"mp3" } else { &"m4a" }));
        assert!(groups(Path::new("notes.txt")).is_empty());
    }

    #[test]
    #[cfg_attr(not(win), ignore = "needs the bundled engines, which are Windows programs for now")]
    fn converted_copy_preserves_existing_files_and_failed_jobs_leave_no_output() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("cover.v2.png");
        let cache = tempfile::tempdir().unwrap();
        image::DynamicImage::new_rgba8(16,8).save(&source).unwrap();
        let original = fs::read(&source).unwrap();
        let occupied = source.with_extension("jpg");
        fs::write(&occupied, b"other file").unwrap();
        let output = convert(cache.path(), &source,"jpeg").unwrap();
        assert_eq!(output.file_name().unwrap(),"cover.v2 (2).jpg");
        assert_eq!(image::open(output).unwrap().to_rgb8().get_pixel(0,0).0,[255,255,255]);
        assert_eq!(fs::read(occupied).unwrap(),b"other file");
        assert_eq!(fs::read(source).unwrap(),original);
        let broken = dir.path().join("bad.png");
        fs::write(&broken,b"not an image").unwrap();
        assert!(convert(cache.path(), &broken,"jpeg").is_err());
        assert!(convert(cache.path(), &broken,"../escape").is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),4);
    }
}
