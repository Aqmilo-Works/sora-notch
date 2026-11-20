//! chat_notices on macOS (main.rs swaps this file in when `cfg(win)` is off): chat notifications from other apps
//! (WhatsApp, Telegram, Discord, ...) shown in the notch, read from Notification Center's own database. macOS has no
//! API for this; reading that database needs Full Disk Access for Sora (System Settings > Privacy & Security). macOS
//! still shows its own banner too: apps cannot turn another app's banners off there.
use std::{collections::HashSet, path::PathBuf, process::Command, thread, time::{Duration, Instant}};
use tauri::AppHandle;

/// Chat and social apps, matched on the app's name (lower case), as on Windows.
const CHAT_APPS: [&str; 15] = ["whatsapp", "telegram", "discord", "messenger", "instagram", "signal", "line", "wechat", "slack", "teams", "zalo", "chatgpt", "claude", "copilot", "perplexity"];
/// Browsers: web chats (WhatsApp Web, Instagram, X...) notify through them.
const BROWSERS: [&str; 7] = ["chrome", "edge", "brave", "firefox", "opera", "vivaldi", "safari"];
const POLL: Duration = Duration::from_millis(1500);

fn named(name: &str, list: &[&str]) -> bool { let n = name.to_lowercase(); list.iter().any(|a| n.split(|c: char| !c.is_alphanumeric()).any(|w| w == *a)) }
fn is_chat(name: &str) -> bool { named(name, &CHAT_APPS) }
fn is_browser(name: &str) -> bool { named(name, &BROWSERS) }

/// Notification Center's database: macOS 15 keeps it in a group container, older versions in the per-user temp folder.
fn database() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let new = home.join("Library/Group Containers/group.com.apple.usernoted/db2/db");
    if new.exists() { return Some(new); }
    let dir = Command::new("getconf").arg("DARWIN_USER_DIR").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())?;
    Some(PathBuf::from(dir).join("com.apple.notificationcenter/db2/db")).filter(|p| p.exists())
}
/// New records since `after`: (record id, app bundle id, the record's plist bytes), through macOS's own sqlite3 tool.
fn records(db: &PathBuf, after: i64) -> Result<Vec<(i64, String, Vec<u8>)>, String> {
    let sql = format!("select r.rec_id, a.identifier, hex(r.data) from record r join app a on a.app_id = r.app_id where r.rec_id > {after} order by r.rec_id limit 200;");
    let out = Command::new("/usr/bin/sqlite3").args(["-readonly", "-separator", "\t"]).arg(db).arg(sql).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
    Ok(String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| {
        let mut f = l.splitn(3, '\t');
        Some((f.next()?.parse().ok()?, f.next()?.to_string(), unhex(f.next()?)?))
    }).collect())
}
fn unhex(s: &str) -> Option<Vec<u8>> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect() }
/// Title, subtitle and body of one record's plist ({ req: { titl, subt, body } }) (pure; tested).
pub(crate) fn texts(data: &[u8]) -> Option<(String, String, String)> {
    let v = plist::Value::from_reader(std::io::Cursor::new(data)).ok()?;
    let req = v.as_dictionary()?.get("req")?.as_dictionary()?;
    let s = |k: &str| req.get(k).and_then(|x| x.as_string()).unwrap_or("").trim().to_string();
    let (t, sub, body) = (s("titl"), s("subt"), s("body"));
    (!(t.is_empty() && body.is_empty())).then_some((t, sub, body))
}
/// The app's name as macOS shows it ("WhatsApp"), from its bundle id.
fn app_name(bundle: &str) -> String {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;
    let url = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle));
    url.and_then(|u| u.path()).map(|p| p.to_string()).and_then(|p| std::path::Path::new(&p).file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| bundle.rsplit('.').next().unwrap_or(bundle).to_string())
}
fn duplicate(recent: &mut Vec<(String, Instant)>, title: &str, first: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let key = format!("{}\u{1}{}", norm(title), norm(first));
    recent.retain(|(_, at)| at.elapsed() < Duration::from_secs(20));
    if recent.iter().any(|(k, _)| *k == key) { return true; }
    recent.push((key, Instant::now()));
    false
}

/// Nothing to put back: Sora never changes another app's banners on macOS.
pub fn restore_banners() {}
/// Opens the chat app a card came from (the target is its bundle id); for a native notch, which macOS does not have yet.
#[allow(dead_code)]
pub fn open(target: String) {
    let bundle = target.lines().next().unwrap_or("").to_string();
    if !bundle.is_empty() && bundle.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)) { let _ = Command::new("open").args(["-b", &bundle]).spawn(); }
}
/// System Settings > Privacy & Security at the page Sora needs: Full Disk Access for chat notifications (the default),
/// "screen" Screen & System Audio Recording and "microphone" Microphone for Meeting. Fixed addresses only.
#[tauri::command]
pub fn notification_access_settings(pane: Option<String>) -> Result<(), String> {
    let page = match pane.as_deref() { Some("screen") => "Privacy_ScreenCapture", Some("microphone") => "Privacy_Microphone", _ => "Privacy_AllFiles" };
    Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{page}")).spawn().map(|_| ()).map_err(|e| e.to_string())
}
pub fn start(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let Some(db) = database() else { eprintln!("Sora chat: no Notification Center database"); return };
        // What is already there when Sora starts is old news; no access (no Full Disk Access yet) is retried quietly.
        let mut last = loop {
            match Command::new("/usr/bin/sqlite3").arg("-readonly").arg(&db).arg("select coalesce(max(rec_id), 0) from record;").output() {
                Ok(o) if o.status.success() => break String::from_utf8_lossy(&o.stdout).trim().parse::<i64>().unwrap_or(0),
                _ => thread::sleep(Duration::from_secs(30)),
            }
        };
        let mut recent = Vec::new();
        let mut done: HashSet<i64> = HashSet::new();
        loop {
            thread::sleep(POLL);
            let Ok(list) = records(&db, last) else { continue };
            for (id, bundle, data) in list {
                last = last.max(id);
                if !done.insert(id) { continue; }
                let name = app_name(&bundle);
                if !is_chat(&name) && !is_browser(&name) { continue; }
                let Some((title, sub, body)) = texts(&data) else { continue };
                // A browser's notification names the site in its subtitle: that becomes the label.
                let site = (is_browser(&name) && !sub.is_empty()).then(|| sub.clone());
                // WhatsApp linked in Sora and Telegram signed in bring the same messages themselves, ready to answer.
                let from = |a: &str| name.to_lowercase().contains(a) || site.as_deref().is_some_and(|s| s.to_lowercase().contains(a));
                if (crate::whatsapp::linked() && from("whatsapp")) || (crate::telegram::ready() && from("telegram")) { continue; }
                let detail = if site.is_some() || sub.is_empty() { body.clone() } else { format!("{sub} · {body}") };
                if duplicate(&mut recent, &title, &detail) { continue; }
                let claude = name.to_lowercase().contains("claude");
                let finished = if claude { crate::agents::final_for(&title) } else { None };
                if claude && finished.is_none() && crate::desktop::preferences(&app).feature_agents && (detail.starts_with("Allow Claude") || !crate::agents::agent_pending().is_empty()) { continue; }
                let detail = finished.unwrap_or(detail);
                let label = site.as_deref().unwrap_or(&name);
                crate::history::add(label, &title, &detail, if is_browser(&name) { "" } else { &bundle });
                crate::notices::chat(&app, label, &title, "", &detail, "", &bundle, None);
            }
            if done.len() > 4096 { done.clear(); }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_a_notification_record() {
        let mut req = plist::Dictionary::new();
        req.insert("titl".into(), "Ana".into());
        req.insert("body".into(), "See you at 5".into());
        let mut root = plist::Dictionary::new();
        root.insert("app".into(), "net.whatsapp.WhatsApp".into());
        root.insert("req".into(), plist::Value::Dictionary(req));
        let mut bytes = Vec::new();
        plist::Value::Dictionary(root).to_writer_binary(&mut bytes).unwrap();
        assert_eq!(super::texts(&bytes), Some(("Ana".into(), String::new(), "See you at 5".into())));
        assert!(super::is_chat("WhatsApp") && super::is_browser("Google Chrome") && !super::is_chat("Finder"));
    }
}
