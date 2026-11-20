//! In-app updater (the GitHub edition and macOS; the Store edition updates through the Store). What the newest version
//! is comes from the signed manifest at update.soranotch.com (chat_updates.rs checks its Ed25519 signature): its version
//! and, per system, the file's name, SHA-256 and size. The file itself comes from this repository's GitHub release
//! `v<version>`, and is installed only if its size and SHA-256 match the manifest, whoever hosts it.
use serde::Serialize;
use std::time::Duration;
use tauri::{Emitter, Manager};
// What differs per OS lives in win/update.rs and mac/update.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/update.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/update.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

/// GitHub repository ("owner/name") that publishes releases. Empty means updates are not configured.
const REPO: &str = include_str!("update_repo.txt");
const MAX_BYTES: u64 = 300 * 1024 * 1024;

#[derive(Serialize)]
pub struct UpdateInfo { configured: bool, available: bool, current: String, latest: String, size: u64 }

#[derive(Debug)]
struct Release { tag: String, url: String, size: u64, sha256: String }

fn err(e: impl std::fmt::Display) -> String { e.to_string() }
fn repo() -> Option<&'static str> {
    let r = REPO.trim();
    let parts: Vec<&str> = r.split('/').collect();
    (parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))).then_some(r)
}
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut it = core.split('.');
    Some((it.next()?.parse().ok()?, it.next().unwrap_or("0").parse().ok()?, it.next().unwrap_or("0").parse().ok()?))
}
fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(a), Some(b)) if a > b)
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(Duration::from_secs(120)).user_agent(format!("Sora/{}", env!("CARGO_PKG_VERSION"))).build().map_err(err)
}
/// This system's file out of a verified manifest (pure; tested). The version is digits and dots only and the file name a
/// plain name, so the link can only point at this repository's own release of that version.
fn pick_release(repo: &str, m: &crate::chat_updates::Manifest) -> Result<Release, String> {
    let app = m.app.as_ref().ok_or("No release has been published yet.")?;
    let v = app.version.trim_start_matches('v');
    if v.is_empty() || !v.chars().all(|c| c.is_ascii_digit() || c == '.') || parse_version(v).is_none() { return Err("The release has no valid version.".into()); }
    let f = app.files.get(PLATFORM).ok_or("This release has nothing for this system yet.")?;
    let plain = !f.file.is_empty() && f.file.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) && !f.file.starts_with('.');
    if !plain || f.sha256.len() != 64 || !f.sha256.chars().all(|c| c.is_ascii_hexdigit()) || f.size == 0 { return Err("The release information is incomplete.".into()); }
    Ok(Release { tag: v.into(), url: format!("https://github.com/{repo}/releases/download/v{v}/{}", f.file), size: f.size, sha256: f.sha256.to_ascii_lowercase() })
}
async fn latest(repo: &str) -> Result<Release, String> {
    let text = crate::chat_updates::download_text(&format!("{}manifest.json", crate::chat_updates::BASE), 64 * 1024).await.ok_or("Could not reach the update server. Check your connection.")?;
    let (m, _) = crate::chat_updates::verify(&text, crate::chat_updates::main_revoked()).ok_or("The update information is not signed by Sora's publisher.")?;
    pick_release(repo, &m)
}

#[tauri::command]
pub fn app_version() -> String { env!("CARGO_PKG_VERSION").into() }

/// The Store edition updates through the Store; platform::SELF_UPDATES says whether this OS's build replaces itself.
fn self_updates() -> bool { !crate::STORE && crate::platform::SELF_UPDATES }

#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let Some(repo) = repo().filter(|_| self_updates()) else { return Ok(UpdateInfo { configured: false, available: false, current, latest: String::new(), size: 0 }) };
    let release = latest(repo).await?;
    Ok(UpdateInfo { configured: true, available: is_newer(&release.tag, &current), latest: release.tag, size: release.size, current })
}

/// The download is exactly the file the signed manifest names, and a program for this system.
fn check_download(release: &Release, data: &[u8]) -> Result<(), String> {
    let sha: String = ring::digest::digest(&ring::digest::SHA256, data).as_ref().iter().map(|x| format!("{x:02x}")).collect();
    if data.len() as u64 != release.size || sha != release.sha256 { return Err("The download does not match what Sora's publisher signed, so it was not installed.".into()); }
    is_program(data)
}

#[tauri::command]
pub async fn apply_update(app: tauri::AppHandle) -> Result<(), String> {
    let repo = repo().filter(|_| self_updates()).ok_or("Updates are not configured for this build.")?;
    let release = latest(repo).await?;
    if !is_newer(&release.tag, env!("CARGO_PKG_VERSION")) { return Err("Sora is already up to date.".into()); }
    if release.size > MAX_BYTES { return Err("The update is unexpectedly large, so it was not downloaded.".into()); }
    let mut response = client()?.get(&release.url).send().await.and_then(|r| r.error_for_status()).map_err(|_| "Could not download the update.".to_string())?;
    let total = release.size.max(1);
    let mut data = Vec::with_capacity(total.min(MAX_BYTES) as usize);
    let mut shown = 101;
    while let Some(chunk) = response.chunk().await.map_err(|_| "The download was interrupted.".to_string())? {
        data.extend_from_slice(&chunk);
        if data.len() as u64 > release.size { return Err("The update is larger than announced, so it was stopped.".into()); }
        let percent = (data.len() as u64 * 100 / total).min(100);
        if percent != shown { shown = percent; let _ = app.emit("update-progress", percent); }
    }
    check_download(&release, &data)?;
    let _ = app.emit("update-progress", 100u64);
    swap_in(&app, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v1.0.1", "1.0.0") && is_newer("1.1", "1.0.9") && is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.0", "1.0.0") && !is_newer("v0.9.9", "1.0.0") && !is_newer("garbage", "1.0.0"));
        assert_eq!(parse_version("v1.2.3-beta.1+build"), Some((1, 2, 3)));
    }
    fn manifest(app: serde_json::Value) -> crate::chat_updates::Manifest { serde_json::from_value(serde_json::json!({"n": 1, "app": app})).unwrap() }
    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    #[test]
    fn links_only_to_this_repositorys_release() {
        let m = manifest(serde_json::json!({"version": "2.1.0", "files": {PLATFORM: {"file": "Sora-2.1.0.bin", "sha256": SHA, "size": 9}}}));
        let r = pick_release("me/sora", &m).unwrap();
        assert_eq!((r.tag.as_str(), r.url.as_str(), r.size), ("2.1.0", "https://github.com/me/sora/releases/download/v2.1.0/Sora-2.1.0.bin", 9));
        for bad in [serde_json::json!({"version": "2.1.0/../x", "files": {PLATFORM: {"file": "a", "sha256": SHA, "size": 9}}}),
                    serde_json::json!({"version": "2.1.0", "files": {PLATFORM: {"file": "../a", "sha256": SHA, "size": 9}}}),
                    serde_json::json!({"version": "2.1.0", "files": {PLATFORM: {"file": "a", "sha256": "12", "size": 9}}}),
                    serde_json::json!({"version": "2.1.0", "files": {"other": {"file": "a", "sha256": SHA, "size": 9}}})] {
            assert!(pick_release("me/sora", &manifest(bad)).is_err());
        }
        assert!(pick_release("me/sora", &serde_json::from_value(serde_json::json!({"n": 1})).unwrap()).is_err(), "no app yet");
    }
    /// What a release holds here: Sora.exe on Windows, a disk image on macOS.
    fn sample_program() -> Vec<u8> {
        if cfg!(win) { return b"MZ pretend this is Sora.exe".to_vec(); }
        let mut d = vec![0u8; 1024]; d[512..516].copy_from_slice(b"koly"); d
    }
    #[test]
    fn installs_only_the_signed_file() {
        let sha = |b: &[u8]| -> String { ring::digest::digest(&ring::digest::SHA256, b).as_ref().iter().map(|x| format!("{x:02x}")).collect() };
        let program = sample_program();
        let r = Release { tag: "9.0.0".into(), url: String::new(), size: program.len() as u64, sha256: sha(&program) };
        assert!(check_download(&r, &program).is_ok());
        let mut tampered = program.clone(); tampered[5] ^= 1;
        assert!(check_download(&r, &tampered).is_err());
        let not_program = b"hello".to_vec();
        assert!(check_download(&Release { size: 5, sha256: sha(&not_program), ..r }, &not_program).is_err());
    }
    #[test]
    fn repo_names_are_validated() { assert!(repo().map_or(true, |r| r.matches('/').count() == 1)); }
}
