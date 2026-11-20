//! The Windows side of converter_engines (converter_engines.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle, process::CommandExt};
use windows::Win32::{Foundation::{CloseHandle, HANDLE}, System::JobObjects::*};
/// Opens a file for reading that nothing may write or delete while it is open (share mode read only).
pub(super) fn open_locked(path: &Path) -> io::Result<File> { OpenOptions::new().read(true).share_mode(1).open(path) }
impl Engine {
    pub fn prepare(&self, cache: &Path) -> AppResult<(PathBuf, Vec<File>)> {
        let (stem, bytes, meta): (&str, &[u8], &str) = match self {
            Self::Ffmpeg => ("ffmpeg", include_bytes!("../../data/converter/ffmpeg.zip"), include_str!("../../data/converter/ffmpeg.json")),
            Self::Magick => ("magick", include_bytes!("../../data/converter/magick.zip"), include_str!("../../data/converter/magick.json")),
            // ggml-vulkan.dll for meeting transcripts (local_ai::gpu_dir): a library, so its folder is what counts.
            Self::WhisperGpu => ("whisper-gpu", include_bytes!("../../data/converter/whisper-gpu.zip"), include_str!("../../data/converter/whisper-gpu.json")),
        };
        let manifest: Manifest = serde_json::from_str(meta).map_err(err)?;
        let folder = cache.join(stem).join(manifest.version);
        fs::create_dir_all(&folder).map_err(err)?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(err)?;
        let mut locks = Vec::new();
        for (name, hash) in manifest.files {
            let path = folder.join(&name);
            if let Some(file) = checked_file(&path, &hash).map_err(err)? {
                locks.push(file);
                continue;
            }
            let mut member = archive.by_name(&name).map_err(err)?;
            let mut temp = tempfile::NamedTempFile::new_in(&folder).map_err(err)?;
            io::copy(&mut member, &mut temp).map_err(err)?;
            drop(temp.persist(&path).map_err(err)?);
            locks.push(checked_file(&path, &hash).map_err(err)?.ok_or("Bundled converter failed its integrity check.")?);
        }
        Ok((folder.join(format!("{stem}.exe")), locks))
    }
}
pub struct Job(HANDLE);
impl Drop for Job { fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } } }
/// Starts a child that Windows ends when Sora does (or when the returned job is dropped), with no console window.
pub fn spawn_jobbed(cmd: &mut Command) -> AppResult<(std::process::Child, Job)> {
    let job = unsafe { Job(CreateJobObjectW(None, None).map_err(err)?) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(job.0, JobObjectExtendedLimitInformation, std::ptr::from_ref(&limits).cast(),
            std::mem::size_of_val(&limits) as u32).map_err(err)?;
    }
    let mut child = cmd.creation_flags(0x08000000).spawn().map_err(err)?;
    if let Err(e) = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) } {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err(e));
    }
    Ok((child, job))
}
/// Every conversion the bundled engines (ImageMagick, FFmpeg) offer is available on Windows.
pub(crate) fn can_make(_from: &str, _to: &str) -> bool { true }
pub fn image(cache: &Path, source: &Path, output: File, format: &str, progress:crate::convert::Progress<'_>) -> AppResult<()> {
    let (exe, _locks) = Engine::Magick.prepare(cache)?;
    let dir = exe.parent().ok_or("Converter folder missing")?;
    let coder = match crate::convert::extension(source).as_str() {
        "jpg" | "jpeg" | "jfif" => "JPEG".to_owned(),
        "tif" => "TIFF".to_owned(),
        "heif" => "HEIC".to_owned(),
        ext => ext.to_ascii_uppercase(),
    };
    let mut cmd = Command::new(&exe);
    // File handles keep brackets, percent signs and other filename syntax out of ImageMagick's parser.
    cmd.current_dir(dir).env("MAGICK_CONFIGURE_PATH", dir).env_remove("MAGICK_HOME")
        .stdin(File::open(source).map_err(err)?).stdout(output)
        .arg("-monitor").arg(format!("{coder}:-[0]")).arg("-auto-orient");
    if format == "jpeg" {
        cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off", "-colorspace", "sRGB", "-type", "TrueColor", "-quality", "92"]);
    } else if format == "webp" {
        cmd.args(["-define", "webp:lossless=true"]);
    }
    cmd.arg(format!("{}:-", format.to_ascii_uppercase()));
    run(cmd,progress)
}

pub fn media(cache: &Path, source: &Path, output: &Path, format: &str, progress:crate::convert::Progress<'_>) -> AppResult<()> {
    let (exe, _locks) = Engine::Ffmpeg.prepare(cache)?;
    let mut cmd = Command::new(&exe);
    let ext = crate::convert::extension(source);
    let demuxer = match ext.as_str() {
        "mp4" | "m4v" | "mov" | "m4a" => "mov",
        "mkv" | "webm" => "matroska",
        "wmv" | "wma" => "asf",
        "opus" | "ogg" | "oga" => "ogg",
        _ => &ext,
    };
    cmd.current_dir(exe.parent().unwrap()).stdin(Stdio::null()).stdout(Stdio::null())
        .args(["-nostdin", "-hide_banner", "-loglevel", "info", "-nostats", "-progress", "pipe:2", "-xerror", "-y",
               "-protocol_whitelist", "file", "-f", demuxer, "-i"]).arg(source)
        .args(["-sn", "-dn", "-map_chapters", "-1"]);
    match format {
        "mp4" | "mov" | "mkv" | "webm" | "wmv" => {
            cmd.args(["-map", "0:V:0", "-map", "0:a:0?", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-pix_fmt", "yuv420p"]);
            match format {
                "wmv" => cmd.args(["-c:v","wmv2","-q:v","4","-c:a","wmav2","-b:a","160k","-ar","44100","-ac","2"]),
                // Apple's MOV: HEVC tagged hvc1 so QuickTime, iPhone and Mac play it.
                "mov" => cmd.args(["-c:v","libx265","-preset","veryfast","-crf","24","-tag:v","hvc1","-c:a","aac","-b:a","192k"]),
                "webm" => cmd.args(["-c:v","libvpx-vp9","-crf","32","-b:v","0","-cpu-used","4","-c:a","libopus","-b:a","128k"]),
                _ => cmd.args(["-c:v","libx264","-preset","veryfast","-crf","20","-c:a","aac","-b:a","192k"]),
            };
            if matches!(format, "mp4" | "mov") { cmd.args(["-movflags","+faststart"]); }
        }
        _ => {
            cmd.args(["-map", "0:a:0", "-vn"]);
            let preset: &[&str] = match format {
                "mp3" => &["-c:a","libmp3lame","-q:a","2"],
                "m4a" => &["-c:a","aac","-b:a","192k"],
                "wav" => &["-c:a","pcm_s16le"],
                "flac" => &["-c:a","flac"],
                "opus" => &["-c:a","libopus","-b:a","128k"],
                "ogg" => &["-c:a","libvorbis","-q:a","5"],
                _ => return Err("Unknown audio format.".into()),
            };
            cmd.args(preset);
        }
    }
    cmd.arg(output);
    run(cmd,progress)
}
