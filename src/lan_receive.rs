//! Receiving with LocalSend (protocol v2): Sora shows up as a device in the LocalSend app on phones and PCs on the same
//! network, and a sender's files land in Downloads (and the Tray) only after a click on Accept in the notice under the
//! notch. Deny, or no answer within a minute, refuses them. One transfer at a time; another sender meanwhile is told
//! the PC is busy. lan_send.rs is the sending side (same identity and certificate, so Sora is one device to LocalSend).
//!
//! Endpoints (HTTPS on port 53317, Sora's own self-signed certificate; its SHA-256 is the fingerprint):
//! POST /api/localsend/v2/register, GET /api/localsend/v2/info, POST /api/localsend/v2/prepare-upload,
//! POST /api/localsend/v2/upload?sessionId&fileId&token, POST /api/localsend/v2/cancel?sessionId.
use rustls::{pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}, ServerConfig, ServerConnection, StreamOwned};
use serde_json::{json, Value};
use std::{collections::HashMap, io::{BufRead, BufReader, Read, Write}, net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf}, sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc, Mutex}, time::{Duration, Instant}};
use tauri::{AppHandle, Emitter, Manager};

const PORT: u16 = 53317;
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
/// How long the notice waits for Accept or Deny.
const ASK_FOR: Duration = Duration::from_secs(60);
const MAX_FILES: usize = 500;
const MAX_CONNECTIONS: usize = 16;

struct File { name: String, size: u64, token: String, done: bool, path: Option<PathBuf> }
struct Session { id: String, from: String, files: HashMap<String, File>, total: u64, got: u64, shown: Instant }
/// The request waiting for Accept or Deny, and the transfer under way.
static ASKING: Mutex<Option<(String, mpsc::Sender<bool>)>> = Mutex::new(None);
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// What the notice under the notch shows while files come in: its line and how far along (0..1).
static STATUS: Mutex<(String, f32)> = Mutex::new((String::new(), 0.0));

/// The receiving notice's line and progress (the Windows notch draws them).
#[cfg_attr(not(win), allow(dead_code))]
pub fn status() -> (String, f32) { STATUS.lock().map(|s| s.clone()).unwrap_or_default() }
/// A transfer is under way (its progress notice is still worth showing).
pub fn active() -> bool { SESSION.lock().is_ok_and(|s| s.is_some()) }
/// Accept (true) or Deny on the notice of request `id`.
pub fn decide(id: &str, accept: bool) {
    if let Ok(mut a) = ASKING.lock() { if a.as_ref().is_some_and(|(i, _)| i == id) { if let Some((_, tx)) = a.take() { let _ = tx.send(accept); } } }
}
#[tauri::command]
pub fn receive_decide(id: String, accept: bool) { decide(&id, accept); }

fn token() -> String {
    let mut b = [0u8; 16];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}
/// A file name a sender gave, safe to use in Downloads (pure; tested): the last part of any path, no characters
/// Windows or macOS refuse, no leading dots (hidden or relative), at most 180 characters.
fn safe_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c)).collect();
    let clean = clean.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).chars().take(180).collect::<String>();
    let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
    if clean.is_empty() || reserved { format!("file{}", if clean.is_empty() { String::new() } else { format!("-{clean}") }) } else { clean }
}
/// Where a file is written while it comes in (renamed to `p` once complete).
fn part_of(p: &Path) -> PathBuf { PathBuf::from(format!("{}.part", p.display())) }
/// `name` in `dir`, or "name (2).ext"... when that is taken.
fn unique(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') { Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")), _ => (name.to_string(), String::new()) };
    (1..10_000).map(|i| dir.join(if i == 1 { name.to_string() } else { format!("{stem} ({i}){ext}") })).find(|p| !p.exists() && !part_of(p).exists())
        .unwrap_or_else(|| dir.join(format!("{stem} {}{ext}", token())))
}
fn size_text(b: u64) -> String { if b >= 1 << 30 { format!("{:.1} GB", b as f64 / (1u64 << 30) as f64) } else if b >= 1 << 20 { format!("{:.1} MB", b as f64 / (1u64 << 20) as f64) } else { format!("{} KB", b.div_ceil(1024)) } }
fn files_text(n: usize) -> String { if n == 1 { "1 file".into() } else { format!("{n} files") } }

/// Sora as LocalSend sees it.
fn identity(app: &AppHandle) -> Value {
    let id = app.state::<crate::lan_send::Network>().id().to_string();
    let host = std::env::var("COMPUTERNAME").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| if cfg!(win) { "PC".into() } else { "Mac".into() });
    json!({"alias": format!("Sora · {host}"), "version": "2.1", "deviceModel": "Sora", "deviceType": "desktop", "fingerprint": id, "port": PORT, "protocol": "https", "download": false})
}

pub fn start(app: &AppHandle) {
    let net = app.state::<crate::lan_send::Network>();
    let (cert, key) = (net.cert().to_vec(), net.key().to_vec());
    let Ok(config) = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions()
        .and_then(|b| b.with_no_client_auth().with_single_cert(vec![CertificateDer::from(cert)], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))) else { return };
    // The LocalSend app itself (or another copy of Sora) may already have the port: then this one does not receive.
    let Ok(listener) = TcpListener::bind((Ipv4Addr::UNSPECIFIED, PORT)) else { eprintln!("Sora receive: port {PORT} is in use"); return };
    let (config, a) = (Arc::new(config), app.clone());
    std::thread::spawn(move || serve(listener, config, a));
    let a = app.clone();
    std::thread::spawn(move || announce_and_answer(&a));
}

// ---------------------------------------------------------------- discovery
fn lan_v4() -> Vec<Ipv4Addr> {
    if_addrs::get_if_addrs().unwrap_or_default().into_iter().filter_map(|i| match i.addr { if_addrs::IfAddr::V4(v) if !v.ip.is_loopback() && (v.ip.is_private() || v.ip.is_link_local()) => Some(v.ip), _ => None }).collect()
}
fn multicast(packet: &[u8]) {
    for ip in lan_v4() {
        if let Ok(s) = UdpSocket::bind((ip, 0)) { let s = socket2::Socket::from(s); let _ = s.set_multicast_if_v4(&ip); let _ = s.send_to(packet, &SocketAddr::from((GROUP, PORT)).into()); }
    }
}
/// Says hello once (open LocalSend apps list Sora), then answers every LocalSend app that looks for devices.
fn announce_and_answer(app: &AppHandle) {
    let me = identity(app);
    let mut hello = me.clone(); hello["announce"] = true.into(); hello["announcement"] = true.into();
    multicast(&serde_json::to_vec(&hello).unwrap_or_default());
    let Ok(sock) = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None) else { return };
    let _ = sock.set_reuse_address(true);
    if sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into()).is_err() { return; }
    let udp = UdpSocket::from(sock);
    let mut joined: Vec<Ipv4Addr> = Vec::new();
    let _ = udp.set_read_timeout(Some(Duration::from_secs(30)));
    let mut buf = [0u8; 4096];
    loop {
        // Wi-Fi changes: join the group on every network the PC is on now.
        for ip in lan_v4() { if !joined.contains(&ip) && udp.join_multicast_v4(&GROUP, &ip).is_ok() { joined.push(ip); } }
        let Ok((n, from)) = udp.recv_from(&mut buf) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else { continue };
        let asks = v["announce"] == true || v["announcement"] == true;
        if !asks || v["fingerprint"] == me["fingerprint"] { continue; }
        let mut reply = me.clone(); reply["announce"] = false.into(); reply["announcement"] = false.into();
        // Its register endpoint first (what LocalSend prefers), a multicast answer as well.
        let (ip, port, scheme) = (from.ip(), v["port"].as_u64().unwrap_or(PORT as u64), if v["protocol"] == "http" { "http" } else { "https" });
        let body = me.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(c) = reqwest::Client::builder().danger_accept_invalid_certs(true).timeout(Duration::from_secs(3)).build() {
                let _ = c.post(format!("{scheme}://{ip}:{port}/api/localsend/v2/register")).json(&body).send().await;
            }
        });
        multicast(&serde_json::to_vec(&reply).unwrap_or_default());
    }
}

// ---------------------------------------------------------------- HTTP
struct Request { method: String, path: String, query: HashMap<String, String>, length: Option<u64>, chunked: bool, close: bool }
fn read_request<R: BufRead>(r: &mut R) -> Option<Request> {
    let mut line = String::new();
    if r.read_line(&mut line).ok()? == 0 { return None; }
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let (path, q) = target.split_once('?').unwrap_or((&target, ""));
    let query = q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), percent(v))).collect();
    let (mut length, mut chunked, mut close) = (None, false, false);
    for _ in 0..100 {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 { return None; }
        let h = h.trim_end();
        if h.is_empty() { break; }
        let Some((k, v)) = h.split_once(':') else { continue };
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = v.trim().parse().ok(),
            "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
            "connection" => close = v.trim().eq_ignore_ascii_case("close"),
            _ => {}
        }
    }
    Some(Request { method, path: path.to_string(), query, length, chunked, close })
}
fn percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() { if let Some(x) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(x); i += 3; continue; } }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
/// Reads a request's body in pieces (`Content-Length` or chunked), handing each to `each`; at most `limit` bytes.
fn read_body<R: BufRead>(r: &mut R, req: &Request, limit: u64, mut each: impl FnMut(&[u8]) -> bool) -> bool {
    let mut buf = vec![0u8; 64 * 1024];
    let mut copy = |r: &mut R, mut left: u64, total: &mut u64| -> bool {
        while left > 0 {
            let room = left.min(buf.len() as u64) as usize;
            let n = match r.read(&mut buf[..room]) { Ok(0) | Err(_) => return false, Ok(n) => n };
            *total += n as u64;
            if *total > limit || !each(&buf[..n]) { return false; }
            left -= n as u64;
        }
        true
    };
    let mut total = 0;
    if req.chunked {
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 { return false; }
            let Ok(n) = u64::from_str_radix(line.trim().split(';').next().unwrap_or(""), 16) else { return false };
            if n == 0 { let mut end = String::new(); let _ = r.read_line(&mut end); return true; }
            if !copy(r, n, &mut total) { return false; }
            let mut crlf = String::new(); let _ = r.read_line(&mut crlf);
        }
    }
    copy(r, req.length.unwrap_or(0), &mut total)
}
fn json_body<R: BufRead>(r: &mut R, req: &Request) -> Option<Value> {
    let mut b = Vec::new();
    read_body(r, req, 2 << 20, |c| { b.extend_from_slice(c); true }).then(|| serde_json::from_slice(&b).ok()).flatten()
}
fn respond<W: Write>(w: &mut W, status: &str, body: Option<&Value>) {
    let text = body.map(|v| v.to_string()).unwrap_or_default();
    let _ = write!(w, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{text}", text.len());
    let _ = w.flush();
}

fn serve(listener: TcpListener, config: Arc<ServerConfig>, app: AppHandle) {
    let active = Arc::new(AtomicUsize::new(0));
    for tcp in listener.incoming().flatten() {
        // Only the local network: never a request from the internet that got through a forwarded port.
        if !tcp.peer_addr().is_ok_and(|a| match a.ip() { std::net::IpAddr::V4(v) => v.is_private() || v.is_link_local() || v.is_loopback(), std::net::IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xffc0) == 0xfe80 }) { continue; }
        if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS { continue; }
        active.fetch_add(1, Ordering::SeqCst);
        let (config, app, active) = (config.clone(), app.clone(), active.clone());
        std::thread::spawn(move || {
            connection(tcp, config, &app);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}
fn connection(tcp: TcpStream, config: Arc<ServerConfig>, app: &AppHandle) {
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(90)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(conn) = ServerConnection::new(config) else { return };
    let mut r = BufReader::new(StreamOwned::new(conn, tcp));
    // Keep-alive: one connection may carry several requests.
    while let Some(req) = read_request(&mut r) {
        let keep = handle(app, &mut r, &req) && !req.close;
        if !keep { break; }
    }
    let s = r.get_mut(); s.conn.send_close_notify(); let _ = s.flush();
}
/// Answers one request; false when the connection should close (a body left unread).
fn handle<S: Read + Write>(app: &AppHandle, r: &mut BufReader<S>, req: &Request) -> bool {
    let p = req.path.trim_end_matches('/');
    match (req.method.as_str(), p) {
        ("POST", "/api/localsend/v2/register") | ("POST", "/api/localsend/v1/register") => { let _ = json_body(r, req); respond(r.get_mut(), "200 OK", Some(&identity(app))); true }
        ("GET", "/api/localsend/v2/info") | ("GET", "/api/localsend/v1/info") => { respond(r.get_mut(), "200 OK", Some(&identity(app))); true }
        ("POST", "/api/localsend/v2/prepare-upload") => {
            let Some(body) = json_body(r, req) else { respond(r.get_mut(), "400 Bad Request", None); return false };
            let (status, answer) = prepare(app, &body);
            respond(r.get_mut(), status, answer.as_ref());
            true
        }
        ("POST", "/api/localsend/v2/upload") => upload(app, r, req),
        ("POST", "/api/localsend/v2/cancel") => {
            let _ = read_body(r, req, 1 << 20, |_| true);
            if let Some(id) = req.query.get("sessionId") { finish(app, id, true); }
            respond(r.get_mut(), "200 OK", None);
            true
        }
        _ => { respond(r.get_mut(), "404 Not Found", None); req.length.unwrap_or(0) == 0 && !req.chunked }
    }
}

/// A sender asks to send files: the notice asks, and only Accept opens a session.
fn prepare(app: &AppHandle, body: &Value) -> (&'static str, Option<Value>) {
    if SESSION.lock().map_or(true, |s| s.is_some()) || ASKING.lock().map_or(true, |a| a.is_some()) { return ("409 Conflict", None); }
    let from: String = body["info"]["alias"].as_str().unwrap_or("A device").chars().filter(|c| !c.is_control()).take(48).collect();
    let Some(list) = body["files"].as_object() else { return ("400 Bad Request", None) };
    if list.is_empty() { return ("204 No Content", None); }
    if list.len() > MAX_FILES { return ("413 Payload Too Large", None); }
    let mut files = HashMap::new();
    for (id, f) in list {
        if id.len() > 128 { return ("400 Bad Request", None); }
        let name = safe_name(f["fileName"].as_str().unwrap_or("file"));
        files.insert(id.clone(), File { name, size: f["size"].as_u64().unwrap_or(0), token: token(), done: false, path: None });
    }
    let total: u64 = files.values().map(|f| f.size).sum();
    let ask = token();
    let (tx, rx) = mpsc::channel();
    if let Ok(mut a) = ASKING.lock() { *a = Some((ask.clone(), tx)); }
    crate::notices::receive(app, "ask", &ask, &format!("{from} wants to send"), &format!("{} · {}", files_text(files.len()), size_text(total)));
    let accepted = rx.recv_timeout(ASK_FOR).unwrap_or(false);
    if let Ok(mut a) = ASKING.lock() { if a.as_ref().is_some_and(|(i, _)| *i == ask) { *a = None; } }
    if !accepted { return ("403 Forbidden", None); }
    let id = token();
    let tokens: serde_json::Map<String, Value> = files.iter().map(|(k, f)| (k.clone(), Value::String(f.token.clone()))).collect();
    let count = files.len();
    if let Ok(mut s) = SESSION.lock() { *s = Some(Session { id: id.clone(), from: from.clone(), files, total, got: 0, shown: Instant::now() }); }
    if let Ok(mut st) = STATUS.lock() { *st = (format!("0 of {} · 0 of {}", count, size_text(total)), 0.0); }
    crate::notices::receive(app, "busy", "", &format!("Receiving from {from}"), &format!("0 of {}", files_text(count)));
    // A sender that never sends (closed the app) does not keep the PC busy: the session ends after a quiet minute.
    let (a, sid) = (app.clone(), id.clone());
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        let idle = SESSION.lock().ok().and_then(|s| s.as_ref().filter(|s| s.id == sid).map(|s| s.shown.elapsed()));
        match idle { None => break, Some(t) if t > Duration::from_secs(90) => { finish(&a, &sid, true); break; } _ => {} }
    });
    ("200 OK", Some(json!({ "sessionId": id, "files": tokens })))
}

fn upload<S: Read + Write>(app: &AppHandle, r: &mut BufReader<S>, req: &Request) -> bool {
    let q = |k: &str| req.query.get(k).cloned().unwrap_or_default();
    let (sid, fid, tok) = (q("sessionId"), q("fileId"), q("token"));
    let dir = app.path().download_dir().ok();
    let target = SESSION.lock().ok().and_then(|mut s| {
        let s = s.as_mut().filter(|s| s.id == sid)?;
        let f = s.files.get_mut(&fid).filter(|f| f.token == tok && !f.done && f.path.is_none())?;
        let path = unique(dir.as_ref()?, &f.name);
        // Made while the session is locked, so two files of the same name sent at once never get the same place.
        let out = std::fs::File::create(part_of(&path)).ok()?;
        f.path = Some(path.clone());
        Some((path, f.size, out))
    });
    let Some((path, size, mut out)) = target else { respond(r.get_mut(), "403 Forbidden", None); return false };
    let part = part_of(&path);
    let mut written = 0u64;
    let mut last = Instant::now();
    let ok = read_body(r, req, size.max(1), |c| {
        if out.write_all(c).is_err() { return false; }
        written += c.len() as u64;
        if last.elapsed() >= Duration::from_millis(150) { last = Instant::now(); progress(app, &sid); }
        add_got(&sid, c.len() as u64);
        true
    });
    drop(out);
    if !ok || written != size || std::fs::rename(&part, &path).is_err() {
        let _ = std::fs::remove_file(&part);
        respond(r.get_mut(), "400 Bad Request", None);
        finish(app, &sid, true);
        return false;
    }
    let all = SESSION.lock().ok().and_then(|mut s| { let s = s.as_mut().filter(|s| s.id == sid)?; if let Some(f) = s.files.get_mut(&fid) { f.done = true; } Some(s.files.values().all(|f| f.done)) }).unwrap_or(false);
    respond(r.get_mut(), "200 OK", None);
    progress(app, &sid);
    if all { finish(app, &sid, false); }
    true
}
fn add_got(sid: &str, n: u64) {
    if let Ok(mut s) = SESSION.lock() { if let Some(s) = s.as_mut().filter(|s| s.id == sid) { s.got = s.got.saturating_add(n); s.shown = Instant::now(); } }
}
/// The notice's line and bar follow the bytes.
fn progress(app: &AppHandle, sid: &str) {
    let Some((line, frac)) = SESSION.lock().ok().and_then(|s| s.as_ref().filter(|s| s.id == sid).map(|s| {
        let done = s.files.values().filter(|f| f.done).count();
        (format!("{} of {} · {} of {}", done, s.files.len(), size_text(s.got), size_text(s.total)), if s.total == 0 { 1.0 } else { (s.got as f64 / s.total as f64) as f32 })
    })) else { return };
    if let Ok(mut st) = STATUS.lock() { *st = (line.clone(), frac.clamp(0.0, 1.0)); }
    crate::notch_ui::receive_progress();
    let _ = app.emit("receive-progress", json!({ "line": line, "frac": frac, "done": false }));
}
/// The transfer ended (all files in, the sender cancelled, or a file failed): received files go to the Tray, partial
/// ones are deleted, and a last notice says what came in.
fn finish(app: &AppHandle, sid: &str, stopped: bool) {
    let Some(s) = SESSION.lock().ok().and_then(|mut s| if s.as_ref().is_some_and(|s| s.id == sid) { s.take() } else { None }) else { return };
    let got: Vec<PathBuf> = s.files.values().filter(|f| f.done).filter_map(|f| f.path.clone()).collect();
    if let Ok(mut st) = STATUS.lock() { *st = (String::new(), 0.0); }
    let _ = app.emit("receive-progress", json!({ "line": "", "frac": 1.0, "done": true }));
    crate::notch_ui::receive_progress();
    if !got.is_empty() { crate::add_received(app, got.clone()); }
    let (title, detail) = match (got.len(), stopped) {
        (0, _) => ("Nothing received".to_string(), format!("{} stopped sending", s.from)),
        (n, true) => (format!("{} received", files_text(n)), format!("{} stopped · in Downloads and Tray", s.from)),
        (n, false) => (format!("{} received", files_text(n)), "In Downloads and Tray".to_string()),
    };
    crate::notices::receive(app, "done", "", &title, &detail);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_from_a_sender_stay_in_downloads() {
        assert_eq!(safe_name("IMG_2041.HEIC"), "IMG_2041.HEIC");
        assert_eq!(safe_name("../../Windows/evil.exe"), "evil.exe");
        assert_eq!(safe_name("folder\\sub\\a:b?.txt"), "ab.txt");
        assert_eq!(safe_name(".bashrc"), "bashrc");
        assert_eq!(safe_name("..."), "file");
        assert_eq!(safe_name("CON.txt"), "file-CON.txt");
        assert_eq!(safe_name("name. "), "name");
        assert!(safe_name(&"x".repeat(400)).len() <= 180);
    }
    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = std::env::temp_dir().join(format!("sora-recv-{}", token()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(unique(&dir, "a.txt"), dir.join("a (2).txt"));
        assert_eq!(unique(&dir, "b"), dir.join("b"));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn reads_requests_and_both_body_kinds() {
        let raw = b"POST /api/localsend/v2/upload?sessionId=s1&fileId=f%201&token=t HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello";
        let mut r = BufReader::new(&raw[..]);
        let req = read_request(&mut r).unwrap();
        assert_eq!((req.path.as_str(), req.query["fileId"].as_str(), req.length), ("/api/localsend/v2/upload", "f 1", Some(5)));
        let mut got = Vec::new();
        assert!(read_body(&mut r, &req, 100, |c| { got.extend_from_slice(c); true }));
        assert_eq!(got, b"hello");
        let raw = b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let req = read_request(&mut r).unwrap();
        let mut got = Vec::new();
        assert!(read_body(&mut r, &req, 100, |c| { got.extend_from_slice(c); true }));
        assert_eq!(got, b"abcde");
        let raw = b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\n0123456789";
        let mut r = BufReader::new(&raw[..]);
        let req = read_request(&mut r).unwrap();
        assert!(!read_body(&mut r, &req, 4, |_| true), "more than announced is refused");
    }
}
