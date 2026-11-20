//! The macOS side of converter_engines (converter_engines.rs), its `os` module. Everything else there is shared.
//! Conversions use the tools every Mac has: `sips` for pictures, `afconvert` for audio and `avconvert` (AVFoundation) for
//! video and for audio out of a video. What they cannot make (WebP, AVIF, MP3, Ogg, Opus, MKV, WebM, WMV) is not offered.
#![allow(unused_imports)]
use super::*;

/// shortcut: macOS has no share modes, the file is only opened; upgrade with flock if a swapped engine matters.
pub(super) fn open_locked(path: &Path) -> io::Result<File> { OpenOptions::new().read(true).open(path) }
/// The bundled engines are Windows programs. On macOS FFmpeg is Homebrew's (brew install ffmpeg), which meeting
/// compression uses; without it recordings stay WAV. Whisper on the graphics card is Metal in Homebrew's whisper-cpp.
impl Engine {
    pub fn prepare(&self, _cache: &Path) -> AppResult<(PathBuf, Vec<File>)> {
        match self {
            Engine::Ffmpeg => crate::platform::find_cli("ffmpeg").map(|p| (p, Vec::new())).ok_or_else(|| "FFmpeg is not installed (brew install ffmpeg).".into()),
            _ => Err("This engine is not available on macOS.".into()),
        }
    }
}
/// shortcut: on macOS a child outlives Sora and its dropped Job; upgrade with a process group killed on drop.
pub struct Job;
pub fn spawn_jobbed(cmd: &mut Command) -> AppResult<(std::process::Child, Job)> { Ok((cmd.spawn().map_err(err)?, Job)) }

const PICTURES_IN: [&str; 12] = ["jpeg", "jpg", "jfif", "png", "gif", "bmp", "tiff", "tif", "heic", "heif", "webp", "avif"];
const VIDEO_IN: [&str; 3] = ["mp4", "m4v", "mov"];
const AUDIO_IN: [&str; 6] = ["mp3", "m4a", "aac", "wav", "flac", "aiff"];
/// Whether this Mac can turn a `.from` file into `to` with its own tools (pure; tested).
pub(crate) fn can_make(from: &str, to: &str) -> bool {
    let from = from.to_ascii_lowercase();
    match to {
        "jpeg" | "png" | "bmp" | "tiff" | "pdf" => PICTURES_IN.contains(&from.as_str()),
        "mp4" | "mov" => VIDEO_IN.contains(&from.as_str()),
        "m4a" | "wav" | "flac" => VIDEO_IN.contains(&from.as_str()) || AUDIO_IN.contains(&from.as_str()),
        // PDF pages through CoreGraphics (convert_documents), DOCX text in plain Rust: both everywhere.
        "zip" => from == "pdf",
        "txt" => from == "docx",
        _ => false,
    }
}
fn tool(mut cmd: Command, what: &str, progress: crate::convert::Progress<'_>) -> AppResult<()> {
    progress(30, what);
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("macOS could not start the converter ({e})."))?;
    if out.status.success() { progress(94, what); return Ok(()); }
    let text = String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("").chars().take(240).collect::<String>();
    Err(format!("Conversion failed. {}", if text.is_empty() { "Invalid or unsupported input." } else { &text }))
}
/// A picture in another format, first frame only, through `sips` (written to `output`).
pub fn image(_cache: &Path, source: &Path, mut output: File, format: &str, progress: crate::convert::Progress<'_>) -> AppResult<()> {
    let kind = match format { "jpeg" => "jpeg", "png" => "png", "bmp" => "bmp", "tiff" => "tiff", _ => return Err("This picture format is not available on macOS.".into()) };
    let dir = tempfile::tempdir().map_err(err)?;
    let out = dir.path().join(format!("out.{kind}"));
    let mut cmd = Command::new("sips");
    cmd.args(["-s", "format", kind]);
    if kind == "jpeg" { cmd.args(["-s", "formatOptions", "92"]); }
    cmd.arg(source).arg("--out").arg(&out);
    tool(cmd, "Processing image", progress)?;
    io::copy(&mut File::open(&out).map_err(err)?, &mut output).map_err(err)?;
    Ok(())
}
/// Audio or video in another format: `afconvert` for audio files, `avconvert` for video and audio out of a video.
pub fn media(_cache: &Path, source: &Path, output: &Path, format: &str, progress: crate::convert::Progress<'_>) -> AppResult<()> {
    let from = crate::convert::extension(source);
    let video = VIDEO_IN.contains(&from.as_str());
    let cmd = match format {
        "mp4" | "mov" => { let mut c = Command::new("avconvert"); c.args(["--preset", "PresetHighestQuality", "--replace", "--source"]).arg(source).arg("--output").arg(output); c }
        "m4a" if video => { let mut c = Command::new("avconvert"); c.args(["--preset", "PresetAppleM4A", "--replace", "--source"]).arg(source).arg("--output").arg(output); c }
        "m4a" | "wav" | "flac" => {
            let (file, data): (&str, &str) = match format { "m4a" => ("m4af", "aac"), "wav" => ("WAVE", "LEI16"), _ => ("flac", "flac") };
            let mut c = Command::new("afconvert");
            c.args(["-f", file, "-d", data]);
            if format == "m4a" { c.args(["-b", "192000"]); }
            c.arg(source).arg(output);
            c
        }
        _ => return Err("This format is not available on macOS.".into()),
    };
    tool(cmd, "Encoding", progress)
}

#[cfg(test)]
mod tests {
    #[test]
    fn offers_only_what_the_mac_can_make() {
        assert!(super::can_make("heic", "jpeg") && super::can_make("mov", "mp4") && super::can_make("mp3", "m4a") && super::can_make("mp4", "wav") && super::can_make("pdf", "zip"));
        assert!(!super::can_make("png", "webp") && !super::can_make("wav", "mp3") && !super::can_make("mkv", "mp4") && !super::can_make("ogg", "wav"));
    }
}
