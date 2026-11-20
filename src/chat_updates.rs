//! Chat helper updates without a new Sora.exe, and what the newest Sora is (update.rs). Sora's private update repository
//! publishes a signed manifest at `BASE` (update.soranotch.com, everything else there answers 404): the newest WhatsApp
//! helper (whatsmeow inside), its rollout share, an emergency pause for WhatsApp, Telegram's key pack, and the newest
//! Sora release's files. Sora asks once a day (and when Telegram
//! refuses its key), with no id of any kind: a plain GET of a static file.
//! Trust: the manifest must carry an Ed25519 signature by the main key, or by the offline backup key (which can also
//! revoke the main key); its serial only goes up (no replay of an old helper); a helper is used only if its SHA-256 and
//! size match the manifest. A downloaded helper that fails three runs in a row is dropped for the built-in one.
#![cfg_attr(not(win), allow(dead_code))]
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Mutex}, thread, time::Duration};
use tauri::AppHandle;

pub const BASE: &str = "https://update.soranotch.com/u-ab229925bebb/";
const KEY_MAIN: &str = "15bdf1226222b881246d4dcc1eded469f00528a31fa3b8b9efe831f2a89c45d9";
const KEY_BACKUP: &str = "774808a56feaa8081b2b5033d59f891e1d59bba4a57167ba2ebdade64de07c83";
/// The whatsmeow version (pseudo-version timestamp) of the helper built into this Sora.exe (build.rs, whatsapp/go.mod).
pub const WA_BUILT_IN: &str = env!("SORA_WA_VERSION");
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_HELPER: usize = 64 << 20;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Wa { pub version: String, pub file: String, pub sha256: String, pub size: u64, pub rollout: u32 }
/// One system's file of a Sora release ("windows": the portable Sora.exe; "macos": the .dmg).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct AppFile { pub file: String, pub sha256: String, pub size: u64 }
/// The newest Sora release (update.rs): its version and each system's file.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct App { pub version: String, pub files: std::collections::HashMap<String, AppFile> }
#[derive(Deserialize, Debug)]
pub struct Manifest { pub n: u64, #[serde(default)] pub wa: Option<Wa>, #[serde(default)] pub wa_pause: bool, #[serde(default)] pub tg: Option<Value>, #[serde(default)] pub revoke_main: bool, #[serde(default)] pub app: Option<App> }
#[derive(Deserialize)]
struct Envelope { body: String, key: String, sig: String }

/// What this PC remembers (updates\state.json): the last serial, a revoked main key, its rollout bucket, helpers that
/// failed here, the helper in use, and the pause.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct State { pub n: u64, pub main_revoked: bool, pub bucket: Option<u32>, pub bad: Vec<String>, pub active: Option<(String, String)>, pub paused: bool, pub checked: u64 }

static STATE: Mutex<Option<State>> = Mutex::new(None);
static FAILS: AtomicU64 = AtomicU64::new(0);

fn dir() -> Option<PathBuf> { Some(PathBuf::from(std::env::var_os("APPDATA")?).join("com.nook.desktop").join("updates")) }
fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) }
fn load() -> State {
    let mut g = match STATE.lock() { Ok(g) => g, Err(_) => return State::default() };
    if g.is_none() { *g = Some(dir().and_then(|d| std::fs::read(d.join("state.json")).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()); }
    g.clone().unwrap_or_default()
}
fn save(s: &State) {
    if let Ok(mut g) = STATE.lock() { *g = Some(s.clone()); }
    let Some(d) = dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    let tmp = d.join("state.tmp");
    if std::fs::write(&tmp, serde_json::to_vec(s).unwrap_or_default()).is_ok() { let _ = std::fs::rename(&tmp, d.join("state.json")); }
}

/// The manifest, if `text` carries a good signature by a key this PC still trusts. Returns the body and the key name.
pub fn verify(text: &str, main_revoked: bool) -> Option<(Manifest, String)> {
    use base64::Engine;
    let env: Envelope = serde_json::from_str(text).ok()?;
    let key = match env.key.as_str() { "main" if !main_revoked => KEY_MAIN, "backup" => KEY_BACKUP, _ => return None };
    let pub_key = hex(key)?;
    let sig = base64::engine::general_purpose::STANDARD.decode(env.sig.as_bytes()).ok()?;
    UnparsedPublicKey::new(&ED25519, pub_key).verify(env.body.as_bytes(), &sig).ok()?;
    Some((serde_json::from_str(&env.body).ok()?, env.key))
}
fn hex(s: &str) -> Option<Vec<u8>> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect() }
fn sha256_hex(b: &[u8]) -> String { ring::digest::digest(&ring::digest::SHA256, b).as_ref().iter().map(|x| format!("{x:02x}")).collect() }

/// What a verified manifest changes on this PC (pure, so it is tested): the new state, and the helper to fetch, if any.
pub fn apply(mut s: State, m: &Manifest, key: &str) -> Option<(State, Option<Wa>)> {
    if m.n < s.n { return None; } // an older manifest: replay, refused
    s.n = m.n;
    if m.revoke_main && key == "backup" { s.main_revoked = true; }
    s.paused = m.wa_pause;
    let bucket = *s.bucket.get_or_insert_with(|| { let mut b = [0u8; 4]; let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b); u32::from_le_bytes(b) % 100 });
    let current = s.active.as_ref().map_or(WA_BUILT_IN.to_string(), |a| a.0.clone());
    let want = m.wa.clone().filter(|w| {
        w.version.len() == WA_BUILT_IN.len() && w.version.as_str() > current.as_str() && !s.bad.contains(&w.version) && bucket < w.rollout.min(100)
            && !w.file.contains(['/', '\\', ':']) && w.file.ends_with(".exe") && w.sha256.len() == 64 && w.size as usize <= MAX_HELPER
    });
    Some((s, want))
}

async fn download(url: &str, limit: usize) -> Option<Vec<u8>> {
    let r = reqwest::Client::builder().timeout(Duration::from_secs(120)).user_agent("Sora").build().ok()?.get(url).send().await.ok()?.error_for_status().ok()?;
    if r.content_length().is_some_and(|n| n as usize > limit) { return None; }
    let b = r.bytes().await.ok()?;
    (b.len() <= limit).then(|| b.to_vec())
}
pub async fn download_text(url: &str, limit: usize) -> Option<String> { String::from_utf8(download(url, limit).await?).ok() }

/// Fetches and applies the manifest (off the UI thread). At most once per 10 minutes.
pub fn check(app: &AppHandle) {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let t = now();
    let last = LAST.load(Ordering::SeqCst);
    if t.saturating_sub(last) < 600 || LAST.compare_exchange(last, t, Ordering::SeqCst, Ordering::SeqCst).is_err() { return; }
    let app = app.clone();
    thread::spawn(move || run_check(&app, &format!("{BASE}manifest.json")));
}
fn run_check(app: &AppHandle, url: &str) {
    let mut s = load();
    s.checked = now();
    save(&s);
    let Some(text) = tauri::async_runtime::block_on(download_text(url, MAX_MANIFEST)) else { return };
    let Some((m, key)) = verify(&text, s.main_revoked) else { return };
    let Some((mut s, want)) = apply(s, &m, &key) else { return };
    let was_paused = load().paused;
    save(&s);
    if let Some(pack) = &m.tg { crate::telegram::offer_pack(app, &pack.to_string()); }
    if let Some(w) = want {
        let url = format!("{}{}", url.rsplit_once('/').map_or("", |(base, _)| base), format!("/{}", w.file));
        if let Some(bytes) = tauri::async_runtime::block_on(download(&url, MAX_HELPER)) {
            if bytes.len() as u64 == w.size && sha256_hex(&bytes) == w.sha256 {
                if let Some(d) = dir() {
                    let file = d.join(&w.file);
                    let tmp = file.with_extension("part");
                    if std::fs::write(&tmp, &bytes).is_ok() && std::fs::rename(&tmp, &file).is_ok() {
                        s.active = Some((w.version.clone(), w.file.clone()));
                        save(&s);
                        FAILS.store(0, Ordering::SeqCst);
                        crate::whatsapp::restart(app);
                        return;
                    }
                }
            }
        }
    }
    if was_paused != s.paused { crate::whatsapp::restart(app); }
}

/// The downloaded helper to run instead of the built-in one, if there is a good one.
pub fn helper() -> Option<PathBuf> {
    let s = load();
    let (version, file) = s.active?;
    if s.bad.contains(&version) { return None; }
    Some(dir()?.join(file)).filter(|p| p.is_file())
}
/// WhatsApp is paused by the manifest's emergency switch.
pub fn wa_paused() -> bool { load().paused }
/// The main key was revoked on this PC (update.rs checks the manifest the same way).
pub fn main_revoked() -> bool { load().main_revoked }
/// The WhatsApp helper ran fine (it reached linked, unlinked or a QR code).
pub fn helper_ok() { FAILS.store(0, Ordering::SeqCst); }
/// The WhatsApp helper ended on its own without running fine: three times in a row with a downloaded helper, that helper
/// is marked bad and the built-in one comes back.
pub fn helper_failed() {
    if FAILS.fetch_add(1, Ordering::SeqCst) + 1 < 3 { return; }
    FAILS.store(0, Ordering::SeqCst);
    let mut s = load();
    if let Some((v, _)) = s.active.take() { if !s.bad.contains(&v) { s.bad.push(v); } save(&s); }
}

/// Once a day while WhatsApp or Telegram is on (a file time, so a Sora that never runs a whole day still checks);
/// woken hourly, the first look 1 to 30 minutes after start so PCs do not all ask at once.
pub fn start(app: &AppHandle) {
    // The manifest ships Windows helpers only (sora-wa-<version>.exe).
    if !crate::platform::CHAT_HELPER_UPDATES { return; }
    let app = app.clone();
    thread::spawn(move || {
        let mut b = [0u8; 1];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b);
        thread::sleep(Duration::from_secs(60 + (b[0] as u64 % 30) * 60));
        loop {
            let p = crate::desktop::preferences(&app);
            if (p.feature_whatsapp || p.feature_telegram) && now().saturating_sub(load().checked) >= 24 * 3600 { check(&app); }
            thread::sleep(Duration::from_secs(3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn wa(version: &str, rollout: u32) -> Wa { Wa { version: version.into(), file: format!("sora-wa-{version}.exe"), sha256: "a".repeat(64), size: 100, rollout } }
    fn manifest(n: u64, w: Option<Wa>) -> Manifest { Manifest { n, wa: w, wa_pause: false, tg: None, revoke_main: false, app: None } }
    fn newer() -> String { format!("{}", WA_BUILT_IN.parse::<u64>().unwrap_or(0) + 1) }

    #[test]
    fn only_a_manifest_signed_by_a_trusted_key_is_read() {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let other = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let body = r#"{"n":5,"wa":null}"#;
        let sig = base64::engine::general_purpose::STANDARD.encode(other.sign(body.as_bytes()));
        let forged = serde_json::json!({ "body": body, "key": "main", "sig": sig }).to_string();
        assert!(verify(&forged, false).is_none(), "signed by a key Sora does not know");
        assert!(verify("not json", false).is_none());
        assert!(verify(&serde_json::json!({ "body": body, "key": "other", "sig": sig }).to_string(), false).is_none());
        assert_eq!(other.public_key().as_ref().len(), 32);
        assert_eq!(hex(KEY_MAIN).unwrap().len(), 32);
        assert_eq!(hex(KEY_BACKUP).unwrap().len(), 32);
    }

    /// A manifest made and signed by the publisher's manifest tool (Go) with the real main key verifies here.
    /// SORA_MANIFEST=<path to manifest.json> cargo test --release real_manifest -- --ignored
    #[test]
    #[ignore]
    fn a_real_manifest_from_the_go_tool_verifies() {
        let text = std::fs::read_to_string(std::env::var("SORA_MANIFEST").unwrap()).unwrap();
        let (m, key) = verify(&text, false).expect("signature");
        assert_eq!(key, "main");
        assert!(m.n >= 1 && m.wa.is_some() && m.tg.is_some());
        assert!(verify(&text, true).is_none(), "a revoked main key is not trusted");
        let bent = text.replacen("\\\"n\\\":", "\\\"n\\\": ", 1);
        assert!(verify(&bent, false).is_none(), "a changed body");
    }

    /// The live site, as Sora reads it: signed manifest, then the helper it names, matching its size and SHA-256.
    #[test]
    #[ignore]
    fn the_live_helper_downloads_and_matches_its_manifest() {
        let url = format!("{BASE}manifest.json");
        let text = tauri::async_runtime::block_on(download_text(&url, MAX_MANIFEST)).expect("manifest");
        let (m, _) = verify(&text, false).expect("signature");
        let w = m.wa.expect("helper");
        let bytes = tauri::async_runtime::block_on(download(&format!("{BASE}{}", w.file), MAX_HELPER)).expect("helper download");
        assert_eq!(bytes.len() as u64, w.size);
        assert_eq!(sha256_hex(&bytes), w.sha256);
        assert!(tauri::async_runtime::block_on(download(&format!("{BASE}{}x", w.file), MAX_HELPER)).is_none(), "a missing file is refused");
    }

    #[test]
    #[cfg_attr(not(win), ignore = "chat helper updates are Windows only (.exe helpers)")]
    fn a_newer_helper_is_fetched_only_within_its_rollout_and_never_twice_when_bad() {
        let v = newer();
        let s = State { bucket: Some(30), ..Default::default() };
        assert!(apply(s.clone(), &manifest(1, Some(wa(&v, 10))), "main").unwrap().1.is_none(), "bucket 30 is past 10 %");
        assert_eq!(apply(s.clone(), &manifest(1, Some(wa(&v, 50))), "main").unwrap().1.unwrap().version, v);
        assert!(apply(s.clone(), &manifest(1, Some(wa(WA_BUILT_IN, 100))), "main").unwrap().1.is_none(), "not newer than built in");
        let bad = State { bad: vec![v.clone()], ..s.clone() };
        assert!(apply(bad, &manifest(1, Some(wa(&v, 100))), "main").unwrap().1.is_none(), "failed here before");
        let mut sneaky = wa(&v, 100);
        sneaky.file = "..\\evil.exe".into();
        assert!(apply(s.clone(), &manifest(1, Some(sneaky)), "main").unwrap().1.is_none(), "a path in the file name");
    }

    #[test]
    fn an_older_serial_is_refused_and_only_the_backup_key_revokes_the_main_one() {
        let s = State { n: 7, bucket: Some(0), ..Default::default() };
        assert!(apply(s.clone(), &manifest(6, None), "main").is_none(), "replay");
        let mut m = manifest(8, None);
        m.revoke_main = true;
        assert!(!apply(s.clone(), &m, "main").unwrap().0.main_revoked);
        assert!(apply(s.clone(), &m, "backup").unwrap().0.main_revoked);
        m.wa_pause = true;
        assert!(apply(s, &m, "main").unwrap().0.paused);
    }
}
