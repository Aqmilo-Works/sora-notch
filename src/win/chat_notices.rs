//! Chat notifications from other apps (WhatsApp, Telegram, Discord, ...), shown in the notch instead of Windows' own popup.
//! Read with the documented UserNotificationListener (polled: its change event fails for unpackaged apps, 0x80070490).
//! For each chat app seen, Windows' banner is turned off (ShowBanner = 0) while the notification still goes to the
//! Notification Center (ShowInActionCenter = 1); the previous values are kept in a file and put back when Sora quits.
use std::{collections::{HashMap, HashSet}, fs, os::windows::process::CommandExt, path::PathBuf, process::Command, sync::Mutex, thread, time::Duration};
use tauri::{AppHandle, Manager};
use windows::UI::Notifications::{KnownNotificationBindings, Management::{UserNotificationListener, UserNotificationListenerAccessStatus}, NotificationKinds};

/// Chat and social apps, matched on the app's display name (lower case).
/// AI chat apps are here too: ChatGPT, Claude, Copilot and Perplexity tell you an answer is ready the same way.
const CHAT_APPS: [&str; 15] = ["whatsapp", "telegram", "discord", "messenger", "instagram", "signal", "line", "wechat", "slack", "teams", "zalo", "chatgpt", "claude", "copilot", "perplexity"];
/// Browsers: web chats (WhatsApp Web, Instagram, X...) notify through them, and a click can open the right tab.
const BROWSERS: [&str; 6] = ["chrome", "edge", "brave", "firefox", "opera", "vivaldi"];
const POLL: Duration = Duration::from_millis(600);
const NO_WINDOW: u32 = 0x0800_0000;
const SETTINGS: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings";

fn named(name: &str, list: &[&str]) -> bool { let n = name.to_lowercase(); list.iter().any(|a| n.split(|c: char| !c.is_alphanumeric()).any(|w| w == *a)) }
fn is_chat(name: &str) -> bool { named(name, &CHAT_APPS) }
fn is_browser(name: &str) -> bool { named(name, &BROWSERS) }

/// AUMID -> ShowBanner before Sora changed it (None: the value did not exist).
static CHANGED: Mutex<Option<(PathBuf, HashMap<String, Option<u32>>)>> = Mutex::new(None);

fn reg(args: &[&str]) -> Option<String> {
    let out = Command::new("reg.exe").args(args).creation_flags(NO_WINDOW).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
fn show_banner(aumid: &str) -> Option<u32> {
    let out = reg(&["query", &format!(r"{SETTINGS}\{aumid}"), "/v", "ShowBanner"])?;
    let hex = out.split_whitespace().last()?.trim_start_matches("0x");
    u32::from_str_radix(hex, 16).ok()
}
fn set_dword(aumid: &str, name: &str, v: u32) { let _ = reg(&["add", &format!(r"{SETTINGS}\{aumid}"), "/v", name, "/t", "REG_DWORD", "/d", &v.to_string(), "/f"]); }

/// Turns Windows' popup off for this chat app (once), remembering what it was.
fn quiet(aumid: &str) {
    let Ok(mut g) = CHANGED.lock() else { return };
    let Some((path, map)) = g.as_mut() else { return };
    if map.contains_key(aumid) { return; }
    map.insert(aumid.to_string(), show_banner(aumid));
    let _ = fs::write(&*path, serde_json::to_vec(map).unwrap_or_default());
    set_dword(aumid, "ShowBanner", 0);
    set_dword(aumid, "ShowInActionCenter", 1);
}
fn put_back(aumid: &str, before: Option<u32>) {
    match before {
        Some(v) => set_dword(aumid, "ShowBanner", v),
        None => { let _ = reg(&["delete", &format!(r"{SETTINGS}\{aumid}"), "/v", "ShowBanner", "/f"]); }
    }
}
/// Sora is quitting (or Windows is signing out): every chat app gets its own popup back, as it was.
pub fn restore_banners() {
    let Ok(mut g) = CHANGED.lock() else { return };
    let Some((path, map)) = g.as_mut() else { return };
    for (aumid, before) in map.drain() { put_back(&aumid, before); }
    let _ = fs::remove_file(&*path);
}
/// Sora could not read this app's notification: its Windows popup comes back, so nothing is missed.
fn unquiet(aumid: &str) {
    let Ok(mut g) = CHANGED.lock() else { return };
    let Some((path, map)) = g.as_mut() else { return };
    if let Some(before) = map.remove(aumid) { put_back(aumid, before); let _ = fs::write(&*path, serde_json::to_vec(map).unwrap_or_default()); }
}

/// One toast's app name and AUMID, and its lines of text (None: there was no readable text).
fn read(n: &windows::UI::Notifications::UserNotification) -> Option<(String, String, Option<Vec<String>>)> {
    let info = n.AppInfo().ok()?;
    let name = info.DisplayInfo().ok()?.DisplayName().ok()?.to_string();
    let aumid = info.AppUserModelId().ok()?.to_string();
    let lines = (|| {
        let binding = n.Notification().ok()?.Visual().ok()?.GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?).ok()?;
        let lines: Vec<String> = binding.GetTextElements().ok()?.into_iter().filter_map(|t| t.Text().ok().map(|s| s.to_string())).filter(|s| !s.trim().is_empty()).collect();
        (!lines.is_empty()).then_some(lines)
    })();
    Some((name, aumid, lines))
}

// ---- opening what a notification is about, the way a click on its Windows popup would

/// The toast's XML as Windows stored it (wpndatabase.db, read from a copy with Windows' own SQLite, winsqlite3.dll):
/// the row with this notification's id, else that app's newest toast.
/// shortcut: wpndatabase.db is undocumented and may change with Windows updates; then the click only opens the app.
fn toast_xml(id: u32, aumid: &str) -> Option<String> {
    use std::ffi::c_void;
    use windows::{core::{w, PCSTR}, Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW}};
    let src = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(r"Microsoft\Windows\Notifications\wpndatabase.db");
    let dir = std::env::temp_dir().join("sora-wpn");
    fs::create_dir_all(&dir).ok()?;
    let db = dir.join("wpn.db");
    let side = |p: &PathBuf, e: &str| PathBuf::from(format!("{}{e}", p.display()));
    let _ = (fs::remove_file(side(&db, "-wal")), fs::remove_file(side(&db, "-shm")));
    // The copy holds every notification Windows kept: it is deleted as soon as this function ends.
    struct Wipe(Vec<PathBuf>);
    impl Drop for Wipe { fn drop(&mut self) { for p in &self.0 { let _ = fs::remove_file(p); } } }
    let _wipe = Wipe(vec![db.clone(), side(&db, "-wal"), side(&db, "-shm")]);
    fs::copy(&src, &db).ok()?;
    let _ = fs::copy(side(&src, "-wal"), side(&db, "-wal"));
    let path = format!("{}\0", db.display());
    let sql = "SELECT n.Payload FROM Notification n JOIN NotificationHandler h ON n.HandlerId = h.RecordId WHERE n.Type = 'toast' AND (n.Id = ?1 OR h.PrimaryId = ?2) ORDER BY n.Id = ?1 DESC, n.ArrivalTime DESC LIMIT 1\0";
    unsafe {
        let lib = LoadLibraryW(w!("winsqlite3.dll")).ok()?;
        macro_rules! api { ($name:literal: $t:ty) => { std::mem::transmute::<_, $t>(GetProcAddress(lib, PCSTR(concat!($name, "\0").as_ptr()))?) } }
        let open = api!("sqlite3_open_v2": unsafe extern "system" fn(*const u8, *mut *mut c_void, i32, *const u8) -> i32);
        let prepare = api!("sqlite3_prepare_v2": unsafe extern "system" fn(*mut c_void, *const u8, i32, *mut *mut c_void, *mut *const u8) -> i32);
        let bind_int = api!("sqlite3_bind_int64": unsafe extern "system" fn(*mut c_void, i32, i64) -> i32);
        let bind_text = api!("sqlite3_bind_text": unsafe extern "system" fn(*mut c_void, i32, *const u8, i32, isize) -> i32);
        let step = api!("sqlite3_step": unsafe extern "system" fn(*mut c_void) -> i32);
        let blob = api!("sqlite3_column_blob": unsafe extern "system" fn(*mut c_void, i32) -> *const u8);
        let bytes = api!("sqlite3_column_bytes": unsafe extern "system" fn(*mut c_void, i32) -> i32);
        let finalize = api!("sqlite3_finalize": unsafe extern "system" fn(*mut c_void) -> i32);
        let close = api!("sqlite3_close": unsafe extern "system" fn(*mut c_void) -> i32);
        let (mut h, mut st) = (std::ptr::null_mut(), std::ptr::null_mut());
        // Read-write on the copy, so SQLite can fold the copied -wal in.
        if open(path.as_ptr(), &mut h, 2, std::ptr::null()) != 0 { close(h); return None; }
        let mut xml = None;
        if prepare(h, sql.as_ptr(), -1, &mut st, std::ptr::null_mut()) == 0 {
            bind_int(st, 1, id as i64);
            bind_text(st, 2, aumid.as_ptr(), aumid.len() as i32, -1);
            if step(st) == 100 {
                let (p, n) = (blob(st, 0), bytes(st, 0));
                if !p.is_null() && n > 0 { xml = Some(String::from_utf8_lossy(std::slice::from_raw_parts(p, n as usize)).into_owned()); }
            }
            finalize(st);
        }
        close(h);
        xml
    }
}
/// From a toast's XML: its activationType and launch string (XML-unescaped), as Windows would use them on a click.
fn click_target(xml: &str) -> Option<(String, String)> {
    let tag = &xml[xml.find("<toast")?..];
    let tag = &tag[..tag.find('>')?];
    let attr = |name: &str| {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[at..].split('"').next()?.to_string())
    };
    let unescape = |s: String| s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    Some((attr("activationType").unwrap_or_default(), unescape(attr("launch").unwrap_or_default())))
}
/// The sender's or group's picture from a toast's XML (`<image placement="appLogoOverride" src=...>`, else the first
/// image): a local file, or the app's own data folder for a packaged app (ms-appdata). Web pictures are not fetched.
fn avatar(xml: &str, aumid: &str) -> Option<crate::draw::Picture> {
    let images: Vec<&str> = xml.split("<image").skip(1).map(|t| &t[..t.find('>').unwrap_or(t.len())]).collect();
    let tag = images.iter().find(|t| t.contains("appLogoOverride")).or(images.first())?;
    let at = tag.find(" src=\"")? + 6;
    let src = tag[at..].split('"').next()?.replace("&amp;", "&");
    let family = aumid.split('!').next().unwrap_or(aumid);
    let local = |sub: &str, rest: &str| std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Packages").join(family).join(sub).join(rest.replace('/', "\\")));
    let path = if let Some(rest) = src.strip_prefix("ms-appdata:///local/") { local("LocalState", rest)? }
        else if let Some(rest) = src.strip_prefix("ms-appdata:///temp/") { local("TempState", rest)? }
        else if let Some(rest) = src.strip_prefix("file:///") { PathBuf::from(rest.replace("%20", " ")) }
        else if src.len() > 2 && src.as_bytes()[1] == b':' { PathBuf::from(&src) }
        else { return None };
    let bytes = fs::read(path).ok().filter(|b| b.len() < 4 << 20)?;
    crate::draw::decode(&bytes, 96).ok()
}
/// A browser notification's last line is often just the site (web.whatsapp.com): that names it better than the browser.
fn site_line(line: &str) -> bool { !line.contains(' ') && line.contains('.') && line.len() < 60 }
/// The same message shown twice (WhatsApp Web and WhatsApp Desktop on one account, any app open in two places):
/// the same sender and the same text within a short time. Different accounts get different messages, so both show.
fn duplicate(recent: &mut Vec<(String, std::time::Instant)>, title: &str, first: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let key = format!("{}\u{1}{}", norm(title), norm(first));
    recent.retain(|(_, at)| at.elapsed() < Duration::from_secs(20));
    if recent.iter().any(|(k, _)| *k == key) { return true; }
    recent.push((key, std::time::Instant::now()));
    false
}
/// A link Windows may open for a protocol toast: a real scheme, not a file.
fn is_link(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else { return false };
    scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) && !scheme.eq_ignore_ascii_case("file")
}
/// The app's toast activator (COM), as Win32 apps register it: Windows App SDK style, else on the app's Start entry.
unsafe fn activator(aumid: &str) -> Option<windows::core::GUID> {
    use windows::{core::{GUID, HSTRING}, Win32::UI::Shell::{IShellItem2, SHCreateItemFromParsingName, PropertiesSystem::PROPERTYKEY}};
    if let Some(out) = reg(&["query", &format!(r"HKCU\Software\Classes\AppUserModelId\{aumid}"), "/v", "CustomActivator"]) {
        if let Some(g) = out.split_whitespace().last().and_then(|s| GUID::try_from(s.trim_matches(|c| c == '{' || c == '}')).ok()) { return Some(g); }
    }
    // A packaged app (WhatsApp...): its manifest's toast activator, from the installed package's folder.
    if let Some((family, _)) = aumid.split_once('!') {
        let (name, publisher) = family.rsplit_once('_')?;
        let repo = r"HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
        let keys = reg(&["query", repo, "/k", "/f", &format!("{name}_")]).unwrap_or_default();
        for key in keys.lines().rev().map(str::trim).filter(|k| k.starts_with("HKEY") && k.ends_with(&format!("__{publisher}"))) {
            let Some(out) = reg(&["query", key, "/v", "PackageRootFolder"]) else { continue };
            let Some(root) = out.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string())) else { continue };
            let Ok(manifest) = fs::read_to_string(PathBuf::from(root).join("AppxManifest.xml")) else { continue };
            let Some(at) = manifest.find("ToastActivatorCLSID=\"") else { continue };
            let clsid = manifest[at + 21..].split('"').next().unwrap_or("");
            return GUID::try_from(clsid.trim_matches(|c| c == '{' || c == '}')).ok();
        }
        return None;
    }
    // On the app's Start menu shortcut: System.AppUserModel.ID (pid 5) and .ToastActivatorCLSID (pid 26).
    let key = |pid| PROPERTYKEY { fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3), pid };
    let mut dirs: Vec<PathBuf> = ["APPDATA", "ProgramData"].iter().filter_map(|v| std::env::var_os(v)).map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs")).collect();
    while let Some(dir) = dirs.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() { dirs.push(p); continue; }
            if !p.extension().map_or(false, |x| x.eq_ignore_ascii_case("lnk")) { continue; }
            let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem2>(&HSTRING::from(p.as_os_str()), None) else { continue };
            let Ok(id) = item.GetString(&key(5)) else { continue };
            let same = id.to_string().map_or(false, |s| s.eq_ignore_ascii_case(aumid));
            windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as _));
            if same { return item.GetCLSID(&key(26)).ok().filter(|g| *g != GUID::zeroed()); }
        }
    }
    None
}
/// A click on a chat notice: hand it to the app the way Windows would (its link, its toast activator, or the packaged
/// app with the toast's arguments), so the app opens what the notification is about; else just the app.
/// `target`: "<AUMID>\n<notification id>".
pub fn open(target: String) {

    thread::spawn(move || unsafe {
        use windows::{core::{w, HSTRING}, Win32::{System::Com::*, UI::{Notifications::INotificationActivationCallback, Shell::*, WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY, SW_SHOWNORMAL}}}};
        let Some((aumid, id)) = target.split_once('\n') else { return };
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        // The click was ours: let the app take the foreground.
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let (kind, launch) = toast_xml(id.parse().unwrap_or(0), aumid).and_then(|x| click_target(&x)).unwrap_or_default();
        let done = if kind == "protocol" {
            is_link(&launch) && ShellExecuteW(None, w!("open"), &HSTRING::from(launch.as_str()), None, None, SW_SHOWNORMAL).0 as isize > 32
        } else if let (false, Some(clsid)) = (launch.is_empty(), activator(aumid)) {
            CoCreateInstance::<_, INotificationActivationCallback>(&clsid, None, CLSCTX_LOCAL_SERVER)
                .and_then(|cb| cb.Activate(&HSTRING::from(aumid), &HSTRING::from(launch.as_str()), &[])).is_ok()
        } else if aumid.contains('!') {
            CoCreateInstance::<_, IApplicationActivationManager>(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                .and_then(|m| m.ActivateApplication(&HSTRING::from(aumid), &HSTRING::from(launch.as_str()), AO_NONE)).is_ok()
        } else { false };
        if !done { let _ = Command::new("explorer.exe").arg(format!("shell:AppsFolder\\{aumid}")).spawn(); }
        CoUninitialize();
    });
}

/// Settings > Chats > Other apps: Windows' notification access page (a fixed address, nothing from the page).
#[tauri::command]
pub fn notification_access_settings(_pane: Option<String>) -> Result<(), String> {
    Command::new("explorer.exe").arg("ms-settings:privacy-notifications").creation_flags(NO_WINDOW).spawn().map(|_| ()).map_err(|_| "Could not open Windows Settings.".into())
}

pub fn start(app: &AppHandle) {
    let file = app.path().app_data_dir().ok().map(|d| d.join("chat-banners.json"));
    // Left from a crash: those apps are still quiet, keep restoring them later.
    let before: HashMap<String, Option<u32>> = file.as_ref().and_then(|f| fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let (Some(f), Ok(mut g)) = (file, CHANGED.lock()) { *g = Some((f, before)); }
    let app = app.clone();
    thread::spawn(move || {
        let Ok(listener) = UserNotificationListener::Current() else { return };
        if listener.GetAccessStatus().ok() != Some(UserNotificationListenerAccessStatus::Allowed) { eprintln!("Sora chat: notification access not allowed"); return; }
        let ids = |l: &UserNotificationListener| l.GetNotificationsAsync(NotificationKinds::Toast).ok().and_then(|op| op.get().ok());
        // What is already there when Sora starts is old news.
        let mut seen: HashSet<u32> = ids(&listener).map(|v| v.into_iter().filter_map(|n| n.Id().ok()).collect()).unwrap_or_default();
        let mut recent = Vec::new();
        loop {
            thread::sleep(POLL);
            let Some(list) = ids(&listener) else { continue };
            let mut now = HashSet::new();
            for n in list {
                let Ok(id) = n.Id() else { continue };
                now.insert(id);
                if seen.contains(&id) { continue; }
                let Some((name, aumid, lines)) = read(&n) else { continue };
                if !is_chat(&name) && !is_browser(&name) { continue; }
                let Some(lines) = lines else { unquiet(&aumid); continue };
                // WhatsApp's download toast has a file name and no message line: leave it to Windows. (Stickers and photos carry a second line.)
                if name.to_lowercase().contains("whatsapp") && lines.len() < 2 { unquiet(&aumid); continue; }
                quiet(&aumid);
                let mut lines = lines;
                // A browser's toast names the site in its last line: that becomes the label, not part of the message.
                let site = (is_browser(&name) && lines.len() > 1 && lines.last().map_or(false, |l| site_line(l))).then(|| lines.pop()).flatten();
                // WhatsApp linked in Sora (whatsapp.rs) and Telegram signed in (telegram.rs) bring the same messages
                // themselves, ready to answer: not twice.
                let from = |app: &str| name.to_lowercase().contains(app) || site.as_deref().is_some_and(|s| s.contains(app));
                if (crate::whatsapp::linked() && from("whatsapp")) || (crate::telegram::ready() && from("telegram")) { continue; }

                let mut it = lines.into_iter();
                let title = it.next().unwrap_or_else(|| name.clone());
                let rest: Vec<String> = it.collect();
                if duplicate(&mut recent, &title, rest.first().map_or("", |s| s.as_str())) { continue; }
                let detail = rest.join(" · ");
                // The Claude app's toast for a finished task says only a few words: its card shows the start of the answer itself.
                let claude = name.to_lowercase().contains("claude");
                let finished = if claude { crate::agents::final_for(&title) } else { None };
                // The agent asking for permission or an answer: Sora's own Allow/Deny or question notice shows it, not this toast too.
                if claude && finished.is_none() && crate::desktop::preferences(&app).feature_agents && (detail.starts_with("Allow Claude") || !crate::agents::agent_pending().is_empty()) { continue; }
                let detail = finished.unwrap_or(detail);
                crate::history::add(site.as_deref().unwrap_or(&name), &title, &detail, if is_browser(&name) { "" } else { &aumid });
                let image = toast_xml(id, &aumid).and_then(|x| avatar(&x, &aumid));
                crate::notices::chat(&app, site.as_deref().unwrap_or(&name), &title, "", &detail, "", &format!("{aumid}\n{id}"), image);
            }
            seen = now;
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn chat_apps_by_name() {
        assert!(super::is_chat("WhatsApp") && super::is_chat("Telegram Desktop") && super::is_chat("Discord"));
        assert!(!super::is_chat("Microsoft Edge") && !super::is_chat("Outlook") && !super::is_chat("Online Store"), "whole words only");
        assert!(super::is_browser("Google Chrome") && super::is_browser("Microsoft Edge") && !super::is_browser("Edgeware"));
    }
    /// On a real PC: the newest WhatsApp Desktop toast is found and has a click target (prints no text).
    #[test]
    #[ignore]
    fn reads_wpndatabase() {
        let xml = super::toast_xml(0, "5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App").expect("row");
        let (kind, launch) = super::click_target(&xml).expect("toast tag");
        println!("kind={kind:?} launch_len={}", launch.len());
        assert!(!launch.is_empty());
    }
    /// On a real PC: which apps have a toast activator (prints only found / not found).
    #[test]
    #[ignore]
    fn finds_activators() {
        unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED); }
        for a in ["5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App", "Chrome", "MSEdge"] { println!("{a}: {}", unsafe { super::activator(a) }.is_some()); }
    }
    #[test]
    fn one_message_in_two_places_shows_once() {
        let mut recent = Vec::new();
        assert!(!super::duplicate(&mut recent, "Budi", "See you at 7"), "WhatsApp Desktop");
        assert!(super::duplicate(&mut recent, "Budi ", "see you  at 7"), "the same message from WhatsApp Web");
        assert!(!super::duplicate(&mut recent, "Budi", "On my way"), "a new message");
        assert!(!super::duplicate(&mut recent, "Sari", "See you at 7"), "another sender (another account's chat)");
        assert!(super::site_line("web.whatsapp.com") && !super::site_line("See you at 7."));
    }
    /// On a real PC: hands the newest WhatsApp toast's click to WhatsApp's toast activator (opens WhatsApp).
    #[test]
    #[ignore]
    fn activates_whatsapp() {
        use windows::{core::HSTRING, Win32::{System::Com::*, UI::Notifications::INotificationActivationCallback}};
        let aumid = "5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App";
        let (_, launch) = super::click_target(&super::toast_xml(0, aumid).unwrap()).unwrap();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let clsid = super::activator(aumid).unwrap();
            let r = CoCreateInstance::<_, INotificationActivationCallback>(&clsid, None, CLSCTX_LOCAL_SERVER).and_then(|cb| cb.Activate(&HSTRING::from(aumid), &HSTRING::from(launch.as_str()), &[]));
            println!("activate: {:?}", r.map_err(|e| e.code()));
        }
    }
    #[test]
    fn click_target_from_toast_xml() {
        let wa = r#"<toast launch="action=Click&amp;key=msg%3Afalse&amp;tag=x"><visual><binding template="ToastGeneric"><text>Hi</text></binding></visual></toast>"#;
        assert_eq!(super::click_target(wa), Some((String::new(), "action=Click&key=msg%3Afalse&tag=x".into())));
        let link = r#"<toast activationType="protocol" launch="ms-phone:phonenotifications?deviceid=1&amp;n=2">"#;
        assert_eq!(super::click_target(link), Some(("protocol".into(), "ms-phone:phonenotifications?deviceid=1&n=2".into())));
        assert!(super::is_link("ms-phone:x") && super::is_link("https://web.whatsapp.com/") && !super::is_link("file:///C:/x") && !super::is_link("C:\\x.exe"));
    }
}
