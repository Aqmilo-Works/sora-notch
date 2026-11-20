//! Health: a water reminder and screen time per app, for the Home widgets. The water reminder is on from the start and runs
//! without the widget on Home (its bell turns it off); screen time is off until switched on. Both stay on this PC (health.json in Sora's data folder). Nothing here runs a fast loop:
//! the reminder wakes every 30 s, screen time every 5 s and only while it is on.
#![cfg_attr(not(win), allow(dead_code))]
use std::{collections::BTreeMap, fs, path::PathBuf, sync::{Mutex, OnceLock}, thread, time::{Duration, Instant}};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
// What differs per OS lives in win/health.rs and mac/health.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/health.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/health.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

const KEEP_DAYS: usize = 30;
/// No keyboard or mouse for this long: the person is away, so neither the reminder nor screen time counts.
const AWAY_SECS: u32 = 120;
const REMINDER_AWAY_SECS: u32 = 600;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Prefs { pub water: bool, pub water_minutes: u32, pub screen_time: bool }
impl Default for Prefs { fn default() -> Self { Self { water: true, water_minutes: 60, screen_time: false } } }
/// Only turning the reminder on or off restarts its countdown. A new interval leaves the running countdown alone and is
/// used from the next reminder on.
fn restarts(old: &Prefs, new: &Prefs) -> bool { old.water != new.water }
fn valid(p: &Prefs) -> Result<(), String> { if (15..=240).contains(&p.water_minutes) { Ok(()) } else { Err("Choose a reminder between 15 minutes and 4 hours.".into()) } }

/// date ("2026-11-04") -> app -> seconds in front
type Days = BTreeMap<String, BTreeMap<String, u64>>;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store { prefs: Prefs, days: Days, glasses: BTreeMap<String, u32> }
static STORE: Mutex<Option<Store>> = Mutex::new(None);
static FILE: OnceLock<PathBuf> = OnceLock::new();
/// When the next reminder is due (None while the reminder is off); shared so "I drank" and turning it on restart it.
static NEXT: Mutex<Option<Instant>> = Mutex::new(None);
fn set_next(minutes: Option<u32>) { if let Ok(mut n) = NEXT.lock() { *n = minutes.map(|m| Instant::now() + Duration::from_secs(m as u64 * 60)); } }

fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> { STORE.lock().ok().and_then(|mut s| s.as_mut().map(f)) }
fn save() {
    let Some(path) = FILE.get() else { return };
    if let Some(bytes) = with_store(|s| serde_json::to_vec(s).ok()).flatten() {
        let temp = path.with_extension("tmp");
        if fs::write(&temp, bytes).is_ok() { let _ = fs::rename(temp, path); }
    }
}

// ---------------------------------------------------------------- pure helpers (tested)
/// Days since 1970-01-01 of a "YYYY-MM-DD" date (civil calendar), for comparing and counting days.
fn ordinal(date: &str) -> Option<i64> {
    let mut p = date.split('-').map(|n| n.parse::<i64>().ok());
    let (y, m, d) = (p.next()??, p.next()??, p.next()??);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    Some(era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468)
}
/// "C:\Program Files\Google\Chrome\Application\chrome.exe" -> "Chrome"
fn pretty(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let mut c = stem.chars();
    c.next().map_or_else(|| "Unknown".into(), |f| f.to_uppercase().chain(c).collect())
}
fn add(days: &mut Days, date: &str, app: &str, secs: u64) {
    *days.entry(date.to_string()).or_default().entry(app.to_string()).or_default() += secs;
    while days.len() > KEEP_DAYS { if let Some(old) = days.keys().next().cloned() { days.remove(&old); } }
}
#[derive(Serialize, Debug, PartialEq)]
pub struct App { name: String, secs: u64 }
#[derive(Serialize)]
pub struct Snapshot { prefs: Prefs, today: u64, week: u64, apps: Vec<App>, glasses: u32, next_secs: Option<u64> }
fn summarize(days: &Days, today: &str, prefs: &Prefs, glasses: u32, next_secs: Option<u64>) -> Snapshot {
    let now = ordinal(today).unwrap_or(0);
    let mut apps: Vec<App> = days.get(today).map(|d| d.iter().map(|(n, s)| App { name: n.clone(), secs: *s }).collect()).unwrap_or_default();
    apps.sort_by(|a, b| b.secs.cmp(&a.secs).then(a.name.cmp(&b.name)));
    let week = days.iter().filter(|(d, _)| ordinal(d).is_some_and(|o| (now - 6..=now).contains(&o))).flat_map(|(_, a)| a.values()).sum();
    let today_total = apps.iter().map(|a| a.secs).sum();
    apps.truncate(8);
    Snapshot { prefs: prefs.clone(), today: today_total, week, apps, glasses, next_secs }
}

pub fn start(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("health.json");
    let store: Store = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(|s: &Store| valid(&s.prefs).is_ok()).unwrap_or_default();
    let _ = FILE.set(path);
    *STORE.lock().map_err(|e| e.to_string())? = Some(store);
    let reminder = app.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            let Some(p) = with_store(|s| s.prefs.clone()) else { continue };
            if !p.water { set_next(None); continue; }
            let due = {
                let Ok(mut next) = NEXT.lock() else { continue };
                if next.is_none() { *next = Some(Instant::now() + Duration::from_secs(p.water_minutes as u64 * 60)); }
                next.is_some_and(|t| Instant::now() >= t)
            };
            if due {
                // Not over a full-screen app or a game, and not for someone who has walked away: try again soon.
                let quiet = busy() || idle_secs() > REMINDER_AWAY_SECS;
                if quiet { if let Ok(mut n) = NEXT.lock() { *n = Some(Instant::now() + Duration::from_secs(300)); } }
                else { crate::notices::water(&reminder); set_next(Some(p.water_minutes)); }
            }
        }
    });
    thread::spawn(move || {
        let (mut last, mut unsaved) = (Instant::now(), 0u32);
        loop {
            let on = with_store(|s| s.prefs.screen_time).unwrap_or(false);
            thread::sleep(Duration::from_secs(if on { 5 } else { 20 }));
            let elapsed = last.elapsed().as_secs().min(10);
            last = Instant::now();
            if !on { continue; }
            if idle_secs() < AWAY_SECS {
                if let Some(name) = foreground_app() { with_store(|s| add(&mut s.days, &today(), &name, elapsed)); unsaved += 1; }
            }
            if unsaved >= 12 { unsaved = 0; save(); }
        }
    });
    Ok(())
}

fn snapshot() -> Snapshot {
    let left = NEXT.lock().ok().and_then(|n| n.map(|t| t.saturating_duration_since(Instant::now()).as_secs()));
    with_store(|s| summarize(&s.days, &today(), &s.prefs, s.glasses.get(&today()).copied().unwrap_or(0), if s.prefs.water { left } else { None }))
        .unwrap_or_else(|| summarize(&Days::new(), &today(), &Prefs::default(), 0, None))
}
#[tauri::command]
pub fn health_get() -> Snapshot { snapshot() }
#[tauri::command]
pub fn health_set(prefs: Prefs) -> Result<Snapshot, String> {
    valid(&prefs)?;
    let (water, minutes) = (prefs.water, prefs.water_minutes);
    let restart = with_store(|s| { let changed = restarts(&s.prefs, &prefs); s.prefs = prefs; changed }).unwrap_or(false);
    if restart { set_next(water.then_some(minutes)); }
    save();
    Ok(snapshot())
}
/// "I drank": counts a glass for today and starts the countdown again.
#[tauri::command]
pub fn health_drank() -> Snapshot {
    let minutes = with_store(|s| {
        let day = today();
        *s.glasses.entry(day).or_default() += 1;
        while s.glasses.len() > KEEP_DAYS { if let Some(old) = s.glasses.keys().next().cloned() { s.glasses.remove(&old); } }
        s.prefs.water.then_some(s.prefs.water_minutes)
    }).flatten();
    if minutes.is_some() { set_next(minutes); }
    save();
    snapshot()
}
/// Takes back today's last glass (a tap too many); the reminder's countdown is left as it is.
#[tauri::command]
pub fn health_undrank() -> Snapshot {
    with_store(|s| { if let Some(n) = s.glasses.get_mut(&today()) { *n = n.saturating_sub(1); } });
    save();
    snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_count_days_across_months_and_years() {
        assert_eq!(ordinal("1970-01-01"), Some(0));
        assert_eq!(ordinal("2026-11-04").unwrap() - ordinal("2026-10-29").unwrap(), 6);
        assert_eq!(ordinal("2027-01-02").unwrap() - ordinal("2026-12-31").unwrap(), 2);
        assert_eq!(ordinal("2024-03-01").unwrap() - ordinal("2024-02-28").unwrap(), 2, "leap year");
        assert_eq!(ordinal("nope"), None);
    }
    #[test]
    fn app_names_come_from_the_file_name() {
        assert_eq!(pretty(r"C:\Program Files\Google\Chrome\Application\chrome.exe"), "Chrome");
        assert_eq!(pretty("C:/x/Code.exe"), "Code");
        assert_eq!(pretty("explorer"), "Explorer");
        assert_eq!(pretty(""), "Unknown");
    }
    #[test]
    fn screen_time_adds_up_ranks_and_keeps_thirty_days() {
        let mut days = Days::new();
        add(&mut days, "2026-11-04", "Chrome", 600);
        add(&mut days, "2026-11-04", "Code", 1200);
        add(&mut days, "2026-11-04", "Chrome", 300);
        add(&mut days, "2026-11-01", "Chrome", 100);
        add(&mut days, "2026-10-20", "Chrome", 5000);
        let s = summarize(&days, "2026-11-04", &Prefs::default(), 0, None);
        assert_eq!(s.apps, vec![App { name: "Code".into(), secs: 1200 }, App { name: "Chrome".into(), secs: 900 }]);
        assert_eq!((s.today, s.week), (2100, 2200), "the week is the last 7 days, today included");
        for i in 0..40 { add(&mut days, &format!("2026-09-{:02}", i % 28 + 1), "X", 1); add(&mut days, &format!("2025-{:02}-01", i % 12 + 1), "X", 1); }
        assert!(days.len() <= KEEP_DAYS, "old days are dropped: {}", days.len());
        assert!(days.contains_key("2026-11-04"), "the newest days stay");
    }
    #[test]
    fn at_most_eight_apps_and_empty_days_are_zero() {
        let mut days = Days::new();
        for i in 0..12 { add(&mut days, "2026-11-04", &format!("App{i}"), 10 + i); }
        let s = summarize(&days, "2026-11-04", &Prefs::default(), 0, None);
        assert_eq!(s.apps.len(), 8);
        assert!(s.apps[0].secs >= s.apps[7].secs);
        let empty = summarize(&Days::new(), "2026-11-04", &Prefs::default(), 0, None);
        assert_eq!((empty.today, empty.week, empty.apps.len()), (0, 0, 0));
    }
    #[test]
    fn prefs_default_off_and_validate() {
        let p: Prefs = serde_json::from_str("{}").unwrap();
        assert_eq!(p, Prefs { water: true, water_minutes: 60, screen_time: false });
        assert!(valid(&p).is_ok() && valid(&Prefs { water_minutes: 14, ..p.clone() }).is_err() && valid(&Prefs { water_minutes: 241, ..p.clone() }).is_err());
        assert!(!restarts(&p, &Prefs { water_minutes: 30, ..p.clone() }), "a new interval keeps the running countdown");
        assert!(restarts(&p, &Prefs { water: false, ..p.clone() }) && !restarts(&p, &Prefs { screen_time: true, ..p.clone() }));
    }
}
