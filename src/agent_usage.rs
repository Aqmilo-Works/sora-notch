//! Plan usage in the Agents tab (Labs, off until switched on), nothing to install:
//! - Claude: when the tab asks (at most once a minute) Sora reads Claude Code's own login (~/.claude/.credentials.json),
//!   lets `claude auth status` renew it when it ran out, and asks Anthropic's usage endpoint, as Claude Code's /usage
//!   does. The token goes to api.anthropic.com only and is never kept or logged.
//! - Antigravity: while it runs, its own language server knows the model quotas. Sora finds that process, its local
//!   port and the CSRF token on its command line, and asks GetUserStatus on 127.0.0.1 (nothing leaves the PC).
//! - Codex: nothing is installed. When the tab asks (at most once a minute), Sora starts `codex app-server`, asks it
//!   `account/rateLimits/read` as Codex's /status does, and stops it (15 s at most).
//!
//! The Codex part is ported from Coucou (github.com/louis-cfm/coucou: src-tauri/src/codex_plan.rs; MIT,
//! Copyright (c) 2026 Louis Raillé; see THIRD-PARTY-NOTICES.md).
#![cfg_attr(not(win), allow(dead_code))]
use std::{io::{BufRead, BufReader, Write}, path::PathBuf, process::{Command, Stdio}, sync::{mpsc, Mutex}, time::{Duration, Instant}};
use serde_json::{json, Value};
// What differs per OS lives in win/agent_usage.rs and mac/agent_usage.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/agent_usage.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/agent_usage.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;


// ---------------------------------------------------------------- what the tab shows
static CODEX: Mutex<Option<(Option<Value>, Instant)>> = Mutex::new(None);
fn now_ms() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64) }
/// One window of a plan: percent used and when it resets (ms since epoch), from either agent's names (pure; tested).
pub fn window(v: &Value) -> Option<Value> {
    let used = ["used_percentage", "usedPercent", "utilization"].iter().find_map(|k| v.get(*k).and_then(Value::as_f64))?;
    let reset = ["resets_at", "resetsAt"].iter().find_map(|k| v.get(*k)).and_then(|r| r.as_i64().or_else(|| r.as_f64().map(|f| f as i64)).or_else(|| r.as_str().and_then(iso_ms)));
    // Seconds since epoch become ms.
    let reset = reset.map(|r| if r < 100_000_000_000 { r * 1000 } else { r });
    let mins = v.get("windowDurationMins").and_then(Value::as_i64);
    Some(json!({ "used": used.clamp(0.0, 100.0), "resets": reset, "mins": mins }))
}

/// "2026-11-15T13:00:00.123+07:00" (or Z) as ms since epoch (pure; tested).
pub fn iso_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b' ') { return None; }
    let n = |a: usize, z: usize| s.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?, n(17, 19)?);
    let mut i = 19;
    let mut ms = 0;
    if b.get(i) == Some(&b'.') {
        let st = i + 1; i = st;
        while b.get(i).is_some_and(u8::is_ascii_digit) { i += 1; }
        ms = format!("{:0<3}", &s[st..i])[..3].parse().ok()?;
    }
    let off = match b.get(i) {
        None | Some(b'Z') | Some(b'z') => 0,
        Some(&c @ (b'+' | b'-')) => { let o = n(i + 1, i + 3)? * 60 + n(i + 4, i + 6)?; if c == b'+' { o } else { -o } }
        _ => return None,
    };
    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let (yy, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + d - 1;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + h * 3600 + mi * 60 + se - off * 60) * 1000 + ms)
}

// ---------------------------------------------------------------- Claude, plug and play
fn claude_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()).map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude")))
}
/// Claude Code's login token while it is good for another minute (read each time, never kept).
fn claude_token() -> Option<String> {
    let v: Value = serde_json::from_slice(&claude_credentials()?).ok()?;
    let o = v.get("claudeAiOauth")?;
    let fresh = o.get("expiresAt").and_then(Value::as_i64).is_none_or(|e| e > now_ms() + 60_000);
    o.get("accessToken").and_then(Value::as_str).filter(|t| fresh && !t.is_empty()).map(str::to_string)
}
/// An expired login: Claude Code renews its own token on `claude auth status` (Sora never uses the refresh token).
fn claude_renew() {
    let Some(exe) = claude_exe() else { return };
    let mut cmd = if exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) { let mut c = Command::new("cmd"); c.arg("/C").arg(&exe); c } else { Command::new(&exe) };
    let Ok(mut child) = crate::platform::no_window(cmd.args(["auth", "status"])).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline { if !matches!(child.try_wait(), Ok(None)) { return; } std::thread::sleep(Duration::from_millis(100)); }
    let _ = child.kill();
    let _ = child.wait();
}
/// The usage answer as the tab shows it: only the 5-hour and weekly windows (pure; tested).
pub fn claude_windows(v: &Value) -> Option<Value> {
    let (five, week) = (v.get("five_hour").and_then(window), v.get("seven_day").and_then(window));
    (five.is_some() || week.is_some()).then(|| json!({ "five_hour": five, "seven_day": week, "at": now_ms() }))
}
async fn claude_read() -> Option<Value> {
    let token = tauri::async_runtime::spawn_blocking(|| claude_token().or_else(|| { claude_renew(); claude_token() })).await.ok().flatten()?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("Sora").build().ok()?;
    let res = client.get("https://api.anthropic.com/api/oauth/usage").bearer_auth(token).header("anthropic-beta", "oauth-2025-04-20").send().await.ok()?;
    if !res.status().is_success() { return None; }
    claude_windows(&res.json::<Value>().await.ok()?)
}
static CLAUDE_API: Mutex<Option<(Option<Value>, Instant)>> = Mutex::new(None);
/// The last Claude numbers that came (kept 30 minutes): Some stores them, None reads them back.
fn claude_good(store: Option<Value>) -> Option<Value> {
    static GOOD: Mutex<Option<(Value, Instant)>> = Mutex::new(None);
    let mut g = GOOD.lock().ok()?;
    if let Some(v) = store { *g = Some((v, Instant::now())); }
    g.as_ref().filter(|(_, t)| t.elapsed() < Duration::from_secs(1800)).map(|(v, _)| v.clone())
}

// ---------------------------------------------------------------- Antigravity
/// MIB_TCPTABLE_OWNER_PID as u32s: the count, then rows of state, local address, local port (network order), remote
/// address, remote port, pid. Keeps the ports of `pid` on 127.0.0.1 or 0.0.0.0 (pure; tested).
pub fn listener_rows(table: &[u32], pid: u32) -> Vec<u16> {
    let n = table.first().copied().unwrap_or(0) as usize;
    let mut ports: Vec<u16> = table.get(1..).unwrap_or(&[]).chunks_exact(6).take(n)
        .filter(|r| r[5] == pid && (r[1] == u32::from_ne_bytes([127, 0, 0, 1]) || r[1] == 0))
        .map(|r| u16::from_be(r[2] as u16)).collect();
    ports.dedup();
    ports
}
/// The value after --csrf_token (with a space or =) on a command line (pure; tested).
pub fn agy_csrf(cmd: &str) -> Option<String> {
    let at = cmd.find("--csrf_token")? + "--csrf_token".len();
    let v = cmd[at..].trim_start_matches(['=', ' ']).split_whitespace().next()?.trim_matches('"');
    (!v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then(|| v.to_string())
}
/// GetUserStatus as quota rows: one per model (its High/Medium/Low variants share a pool, the busiest one counts),
/// the share used and when it resets; models with no quota at all are left out (pure; tested).
pub fn agy_models(v: &Value) -> Option<Value> {
    let list = v.pointer("/userStatus/cascadeModelConfigData/clientModelConfigs")?.as_array()?;
    let mut models: Vec<(String, f64, Option<i64>)> = vec![];
    for m in list {
        let Some(q) = m.get("quotaInfo") else { continue };
        let resets = q.get("resetTime").and_then(Value::as_str).and_then(iso_ms);
        let left = q.get("remainingFraction").and_then(Value::as_f64);
        // A fraction of 0 is left out of the answer, so a reset time alone means used up; neither means no quota.
        let Some(left) = left.or(resets.map(|_| 0.0)) else { continue };
        let label = m.get("label").and_then(Value::as_str).unwrap_or("Model");
        let name = label.split(" (").next().unwrap_or(label).trim().to_string();
        let used = ((1.0 - left.clamp(0.0, 1.0)) * 1000.0).round() / 10.0;
        match models.iter_mut().find(|x| x.0 == name) {
            Some(x) => { if used > x.1 { x.1 = used; } x.2 = match (x.2, resets) { (Some(a), Some(b)) => Some(a.min(b)), (a, b) => a.or(b) }; }
            None => models.push((name, used, resets)),
        }
    }
    let models: Vec<Value> = models.into_iter().map(|(label, used, resets)| json!({ "label": label, "used": used, "resets": resets })).collect();
    (!models.is_empty()).then(|| json!({ "models": models, "plan": v.pointer("/userStatus/planStatus/planInfo/planName") }))
}
async fn agy_read() -> Option<Value> {
    let (token, ports) = tauri::async_runtime::spawn_blocking(agy_server).await.ok().flatten()?;
    // Its own self-signed certificate on 127.0.0.1 only.
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).danger_accept_invalid_certs(true).build().ok()?;
    let body = json!({ "metadata": { "ideName": "antigravity", "extensionName": "antigravity", "locale": "en" } });
    for port in ports { for scheme in ["https", "http"] {
        let url = format!("{scheme}://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus");
        let Ok(res) = client.post(url).header("Connect-Protocol-Version", "1").header("X-Codeium-Csrf-Token", &token).json(&body).send().await else { continue };
        if let Some(v) = res.json::<Value>().await.ok().as_ref().and_then(agy_models) { return Some(v); }
    } }
    None
}
static AGY: Mutex<Option<(Option<Value>, Instant)>> = Mutex::new(None);

/// The id-2 answer in a stream of JSON-RPC lines, with limits on size (pure; tested).
pub fn first_answer(reader: impl BufRead) -> Option<Value> {
    let mut total = 0usize;
    for line in reader.split(b'\n') {
        let line = line.ok()?;
        total += line.len();
        if total > 2 << 20 || line.len() > 256 << 10 { return None; }
        let Ok(m) = serde_json::from_slice::<Value>(&line) else { continue };
        if m.get("id").and_then(Value::as_i64) == Some(2) { return m.get("result").filter(|r| r.is_object()).cloned(); }
    }
    None
}
fn codex_read() -> Option<Value> {
    let exe = codex_exe()?;
    let mut cmd = if exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) { let mut c = Command::new("cmd"); c.arg("/C").arg(&exe); c } else { Command::new(&exe) };
    let mut child = crate::platform::no_window(cmd.arg("app-server")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(first_answer(BufReader::new(stdout))); });
    let req = concat!(r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"sora","version":"1"}}}"#, "\n", r#"{"method":"initialized"}"#, "\n", r#"{"id":2,"method":"account/rateLimits/read"}"#, "\n");
    let _ = stdin.write_all(req.as_bytes()).and_then(|_| stdin.flush());
    let answer = rx.recv_timeout(Duration::from_secs(15)).ok().flatten();
    drop(stdin);
    kill_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
    // Only the limits: never the account or anything else in the answer.
    let l = answer?.get("rateLimits").cloned()?;
    Some(json!({ "primary": l.get("primary").and_then(window), "secondary": l.get("secondary").and_then(window), "plan": l.get("planType"), "reached": l.get("rateLimitReachedType") }))
}

/// Claude's and Codex's plan limits for the tab; Codex is asked at most once a minute.
#[tauri::command]
pub async fn agent_usage(app: tauri::AppHandle) -> Value {
    if !crate::desktop::preferences(&app).agent_usage { return Value::Null; }
    // Claude: the usage endpoint, at most once a minute.
    let cached = CLAUDE_API.lock().ok().and_then(|c| c.as_ref().filter(|(_, t)| t.elapsed() < Duration::from_secs(60)).map(|(v, _)| v.clone()));
    // A failed ask (network, a busy endpoint) keeps the last numbers that came, for 30 minutes, rather than an empty row.
    let api = match cached { Some(v) => v, None => {
        let fresh = claude_read().await;
        if fresh.is_some() { claude_good(fresh.clone()); }
        let v = fresh.or_else(|| claude_good(None));
        if let Ok(mut c) = CLAUDE_API.lock() { *c = Some((v.clone(), Instant::now())); }
        v
    } };
    let claude = api;
    let fresh = CODEX.lock().ok().and_then(|c| c.as_ref().filter(|(_, t)| t.elapsed() < Duration::from_secs(60)).map(|(v, _)| v.clone()));
    let codex = match fresh {
        Some(v) => v,
        None => { let v = tauri::async_runtime::spawn_blocking(codex_read).await.ok().flatten(); if let Ok(mut c) = CODEX.lock() { *c = Some((v.clone(), Instant::now())); } v }
    };
    let fresh = AGY.lock().ok().and_then(|c| c.as_ref().filter(|(_, t)| t.elapsed() < Duration::from_secs(60)).map(|(v, _)| v.clone()));
    let antigravity = match fresh { Some(v) => v, None => { let v = agy_read().await; if let Ok(mut c) = AGY.lock() { *c = Some((v.clone(), Instant::now())); } v } };
    json!({ "claude": claude, "codex": codex, "antigravity": antigravity })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_usage_answer_reads() {
        assert_eq!(iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_ms("2026-11-15T13:00:00.5+07:00"), Some(1_794_722_400_500));
        assert_eq!(iso_ms("2024-02-29T23:59:59.123456+00:00"), Some(1_709_251_199_123));
        assert!(iso_ms("soon").is_none());
        let a = json!({ "five_hour": { "utilization": 37.0, "resets_at": "2026-11-15T06:00:00+00:00" }, "seven_day": { "utilization": 8, "resets_at": null }, "seven_day_opus": null });
        let w = claude_windows(&a).unwrap();
        assert_eq!((w["five_hour"]["used"].as_f64(), w["five_hour"]["resets"].as_i64()), (Some(37.0), Some(1_794_722_400_000)));
        assert_eq!(w["seven_day"]["used"].as_f64(), Some(8.0));
        assert!(claude_windows(&json!({ "error": {} })).is_none());
    }
    /// Real PC only, with Antigravity open: finds its language server without PowerShell, in milliseconds.
    #[test]
    #[ignore]
    fn finds_antigravity_here() {
        let t = Instant::now();
        let (token, ports) = agy_server().expect("Antigravity is not running");
        let ms = t.elapsed().as_millis();
        eprintln!("{} port(s), token {} chars, {ms} ms", ports.len(), token.len());
        assert!(!token.is_empty() && !ports.is_empty() && ms < 300);
    }
    #[test]
    fn listening_ports_come_from_the_tcp_table() {
        let lo = u32::from_ne_bytes([127, 0, 0, 1]);
        let port = |p: u16| p.to_be() as u32;
        let table = [3, 2, lo, port(53_211), 0, 0, 42, 2, 0, port(60_000), 0, 0, 42, 2, lo, port(9_000), 0, 0, 7];
        assert_eq!(listener_rows(&table, 42), vec![53_211, 60_000]);
        assert!(listener_rows(&table, 1).is_empty());
        assert!(listener_rows(&[5, 2, lo], 42).is_empty(), "a short table is not read past its end");
    }
    #[test]
    fn antigravity_quota_reads() {
        assert_eq!(agy_csrf("x.exe --standalone --csrf_token abc-12_Z --app_data_dir antigravity").as_deref(), Some("abc-12_Z"));
        assert_eq!(agy_csrf("x.exe --csrf_token=\"q1\"").as_deref(), Some("q1"));
        assert!(agy_csrf("x.exe --csrf_token ").is_none() && agy_csrf("x.exe").is_none());
        let s = json!({ "userStatus": { "planStatus": { "planInfo": { "planName": "Pro" } }, "cascadeModelConfigData": { "clientModelConfigs": [
            { "label": "Gemini 3 Pro (High)", "quotaInfo": { "remainingFraction": 0.25, "resetTime": "2026-11-15T06:00:00Z" } },
            { "label": "Gemini 3 Pro (Low)", "quotaInfo": { "remainingFraction": 0.9, "resetTime": "2026-11-15T07:00:00Z" } },
            { "label": "Claude Sonnet", "quotaInfo": { "resetTime": "2026-11-15T06:00:00Z" } },
            { "label": "Internal", "quotaInfo": {} },
            { "label": "No quota" } ] } } });
        let m = agy_models(&s).unwrap();
        assert_eq!(m["models"].as_array().unwrap().len(), 2, "variants share one row; models without quota are left out");
        assert_eq!(m["models"][0]["label"], "Gemini 3 Pro");
        assert_eq!((m["models"][0]["used"].as_f64(), m["models"][0]["resets"].as_i64()), (Some(75.0), Some(1_794_722_400_000)));
        assert_eq!(m["models"][1]["used"].as_f64(), Some(100.0), "no remainingFraction: used up");
        assert_eq!(m["plan"], "Pro");
        assert!(agy_models(&json!({})).is_none());
    }
    #[test]
    fn windows_read_both_agents_names() {
        assert_eq!(window(&json!({ "used_percentage": 73.5, "resets_at": 1900000000 })).unwrap(), json!({ "used": 73.5, "resets": 1900000000000i64, "mins": null }));
        assert_eq!(window(&json!({ "usedPercent": 100, "windowDurationMins": 10080, "resetsAt": 1794800764 })).unwrap()["mins"], 10080);
        assert!(window(&json!({})).is_none());
        let stream = "{\"id\":1,\"result\":{}}\nnoise\n{\"id\":2,\"result\":{\"rateLimits\":{}}}\n";
        assert!(first_answer(stream.as_bytes()).unwrap().get("rateLimits").is_some());
        assert!(first_answer("{\"id\":2,\"error\":{}}\n".as_bytes()).is_none());
    }
}
