//! Home's Steam widget: the games installed on this PC, newest played first, the way launchers (Flow Launcher's Steam
//! plugin) find them. Steam keeps one `appmanifest_<id>.acf` per installed game in each library folder; the library
//! folders are listed in `steamapps/libraryfolders.vdf`; the artwork Steam already downloaded sits in
//! `appcache/librarycache`. Everything is read from disk, nothing is sent anywhere. A game starts with `steam://rungameid/<id>`.
use serde::Serialize;
use std::path::{Path, PathBuf};
// What differs per OS lives in win/steam.rs and mac/steam.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/steam.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/steam.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

type AppResult<T> = Result<T, String>;

#[derive(Serialize, Debug, PartialEq, Clone)]
/// minutes: time played (Steam keeps it per account in userdata/<id>/config/localconfig.vdf); size: bytes on disk.
pub struct Game { id: u32, name: String, played: u64, minutes: u32, size: u64 }

/// The value of `"key"   "value"` on a Valve text-config line (backslash escapes undone), when the line has that key.
fn kv<'a>(line: &'a str, key: &str) -> Option<String> {
    let mut parts = line.trim().split('"').skip(1).step_by(2);
    (parts.next()? == key).then(|| parts.next().map(|v| v.replace("\\\\", "\\"))).flatten()
}
fn field(text: &str, key: &str) -> Option<String> { text.lines().find_map(|l| kv(l, key)) }

/// One game from the text of its appmanifest: only fully installed games (StateFlags bit 4), never Steam's own tools.
fn parse_manifest(text: &str) -> Option<Game> {
    let id: u32 = field(text, "appid")?.parse().ok()?;
    let name = field(text, "name")?;
    let flags: u32 = field(text, "StateFlags")?.parse().ok()?;
    let tool = id == 228980 || ["Proton", "Steam Linux Runtime", "Steamworks Common", "Steamworks Shared"].iter().any(|t| name.starts_with(t));
    (flags & 4 != 0 && !tool).then(|| Game { id, name, played: field(text, "LastPlayed").and_then(|v| v.parse().ok()).unwrap_or(0), minutes: 0, size: field(text, "SizeOnDisk").and_then(|v| v.parse().ok()).unwrap_or(0) })
}
/// Every `"path"` in libraryfolders.vdf.
fn library_paths(text: &str) -> Vec<PathBuf> { text.lines().filter_map(|l| kv(l, "path")).map(PathBuf::from).collect() }


/// Minutes played per game from one localconfig.vdf: each app is a block named by its id, with a "Playtime" line inside.
fn playtimes(text: &str) -> std::collections::HashMap<u32, u32> {
    let mut out = std::collections::HashMap::new();
    let mut app = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(id) = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).and_then(|x| x.parse::<u32>().ok()) { app = Some(id); }
        else if let (Some(id), Some(m)) = (app, kv(t, "Playtime").and_then(|v| v.parse::<u32>().ok())) { out.insert(id, m); }
    }
    out
}

/// Installed games, last played first (never played ones by name).
fn installed() -> Vec<Game> {
    let Some(root) = steam_dir() else { return vec![] };
    let mut libs = vec![root.clone()];
    if let Ok(t) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) { libs.extend(library_paths(&t)); }
    libs.dedup();
    let mut games: Vec<Game> = vec![];
    for lib in libs {
        let Ok(dir) = std::fs::read_dir(lib.join("steamapps")) else { continue };
        for f in dir.flatten() {
            let n = f.file_name().to_string_lossy().into_owned();
            if !(n.starts_with("appmanifest_") && n.ends_with(".acf")) { continue; }
            if let Some(g) = std::fs::read_to_string(f.path()).ok().and_then(|t| parse_manifest(&t)) { if !games.iter().any(|x| x.id == g.id) { games.push(g); } }
        }
    }
    // The most minutes any signed-in account has on a game.
    if let Ok(users) = std::fs::read_dir(root.join("userdata")) {
        for u in users.flatten() {
            let Ok(t) = std::fs::read_to_string(u.path().join("config").join("localconfig.vdf")) else { continue };
            for (id, m) in playtimes(&t) { if let Some(g) = games.iter_mut().find(|g| g.id == id) { g.minutes = g.minutes.max(m); } }
        }
    }
    games.sort_by(|a, b| b.played.cmp(&a.played).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    games
}

#[tauri::command]
pub async fn steam_games() -> Vec<Game> {
    tauri::async_runtime::spawn_blocking(|| { let mut g = installed(); g.truncate(12); g }).await.unwrap_or_default()
}


/// The artwork file for a game inside its cache folder: tall cover first (or the wide header first when `wide`). Newer Steam puts each file
/// in a folder named by a hash, older Steam beside the others.
fn cover_file(dir: &Path, wide: bool) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = vec![];
    let mut visit = |d: &Path| if let Ok(r) = std::fs::read_dir(d) { for f in r.flatten() { if f.path().is_file() { found.push(f.path()); } } };
    visit(dir);
    if let Ok(r) = std::fs::read_dir(dir) { for f in r.flatten() { if f.path().is_dir() { visit(&f.path()); } } }
    let order = if wide { ["library_header.jpg", "header.jpg", "library_600x900.jpg", "library_capsule.jpg"] } else { ["library_600x900.jpg", "library_capsule.jpg", "library_header.jpg", "header.jpg"] };
    order.iter()
        .find_map(|want| found.iter().find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(want))).cloned())
}

/// The cover of an installed game as a data URL (the page may only show `data:` images).
#[tauri::command]
pub async fn steam_cover(id: u32, wide: Option<bool>) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !installed().iter().any(|g| g.id == id) { return Err("That game is not installed.".to_string()); }
        let dir = steam_dir().ok_or("Steam was not found.")?.join("appcache").join("librarycache").join(id.to_string());
        let file = cover_file(&dir, wide.unwrap_or(false)).ok_or("No cover yet.")?;
        let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
        if bytes.len() > 600_000 { return Err("Cover too large.".into()); }
        use base64::Engine;
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    }).await.map_err(|e| e.to_string())?
}

/// Starts a game through Steam. Only an installed game's id is accepted.
#[tauri::command]
pub async fn steam_launch(id: u32) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        if !installed().iter().any(|g| g.id == id) { return Err("That game is not installed.".to_string()); }
        crate::platform::open_path(&format!("steam://rungameid/{id}")).map_err(|_| "Steam did not answer.".to_string())?;
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    const ACF: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"LastPlayed\"\t\t\"1700000000\"\n}\n";
    #[test] fn reads_an_installed_game() { assert_eq!(parse_manifest(ACF), Some(Game { id: 730, name: "Counter-Strike 2".into(), played: 1700000000, minutes: 0, size: 0 })); }
    #[test] fn skips_half_installed_and_tools() {
        assert_eq!(parse_manifest(&ACF.replace("\"4\"", "\"1026\"")), None);
        assert_eq!(parse_manifest(&ACF.replace("Counter-Strike 2", "Proton 9.0")), None);
        assert_eq!(parse_manifest(&ACF.replace("\"730\"", "\"228980\"")), None);
    }
    #[test] fn never_played_is_zero() { assert_eq!(parse_manifest(&ACF.replace("\"LastPlayed\"\t\t\"1700000000\"", "")).map(|g| g.played), Some(0)); }
    #[test] fn finds_library_folders() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"E:\\\\Gaming Station\\\\Steam\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("E:\\Gaming Station\\Steam")]);
    }
    #[test] fn reads_play_time() {
        let vdf = "\"apps\"\n{\n\t\"730\"\n\t{\n\t\t\"LastPlayed\"\t\"1\"\n\t\t\"Playtime\"\t\t\"4063\"\n\t}\n\t\"228980\"\n\t{\n\t\t\"Playtime2wks\"\t\"7\"\n\t}\n}\n";
        let m = playtimes(vdf); assert_eq!(m.get(&730), Some(&4063)); assert_eq!(m.get(&228980), None);
    }
    #[test] #[ignore = "needs Steam on this PC"] fn reads_this_pc() { let g = installed(); println!("{g:?}"); assert!(!g.is_empty()); }
}
