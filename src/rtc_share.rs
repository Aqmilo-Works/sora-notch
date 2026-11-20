//! Send > QR Code with no certificate warning on any network. The phone opens a page on the share service (a real HTTPS
//! name on a public address, so the browser trusts it and no router filters it) and the page connects straight to this PC
//! over WebRTC on the local network: files never pass through the service.
//!
//! No signaling server. Everything the page needs is in the QR's #fragment, which browsers never send to a server: this PC's
//! address and UDP port, its ICE password and its DTLS certificate fingerprint, and the link token. The PC is ICE-lite with
//! those fixed credentials and learns each phone from its first STUN check. The phone's browser checks the PC's certificate
//! against the fingerprint, so it talks only to this PC; the PC knows a phone by the ICE password (only in the QR), then the
//! token and the 6-digit code. So the PC does not check the phone's certificate (nothing to check it against).
//!
//! The rules are web_share's, on the same `Shared` state: the code, one use per link, approval of what the phone sends, the
//! same events for the panel. Protocol on one negotiated data channel (id 0): JSON text messages, binary file chunks.
use super::*;
use std::{collections::VecDeque, net::UdpSocket};
use str0m::{channel::{ChannelConfig, ChannelId}, config::{DtlsCert, Fingerprint}, net::{Protocol, Receive}, Candidate, Event, IceConnectionState, IceCreds, Input, Output, Rtc};

/// File chunks to the phone. Measured (100 MB, Chrome, loopback): 16 KB with str0m's 128 KB send buffer is the fastest;
/// bigger chunks or a bigger buffer made downloads slower (8 MB: 7x slower).
const CHUNK: usize = 16 * 1024;
/// Stop reading the file while this much is still waiting to go out (str0m's send buffer is 128 KB).
const HIGH_WATER: usize = 128 << 10;
/// UDP socket buffers, so bursts on Wi-Fi are not dropped by Windows.
const SOCKET_BUFFER: usize = 4 << 20;
const MAX_PEERS: usize = 8;
const OFFER_WAIT: Duration = Duration::from_secs(60);

/// What goes in the QR: the page, then (in the fragment) version, IPv4, port, fingerprint, ICE ufrag and password, token.
pub(super) struct Link { base: String, head: Vec<u8> }
impl Link {
    pub(super) fn url(&self, token: &str) -> String { format!("{}{}", self.base, b64(&[&self.head[..], token.as_bytes()].concat())) }
}
pub(super) struct Setup { socket: UdpSocket, cert: DtlsCert, creds: IceCreds }

/// Letters and digits only: ICE ufrag and password allow no '-' or '_'. (The small modulo bias leaves about 140 bits.)
fn alnum(n: usize) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    random(n).iter().map(|b| A[*b as usize % A.len()] as char).collect()
}
fn rtc_for(cert: &DtlsCert, creds: &IceCreds) -> Rtc {
    Rtc::builder().set_ice_lite(true).set_fingerprint_verification(false).set_local_ice_credentials(creds.clone()).set_dtls_cert(cert.clone()).build(Instant::now())
}
/// A UDP port on the home address, a certificate and ICE credentials for this link (all made fresh each time).
pub(super) fn setup(ip: IpAddr, page: &str) -> AppResult<(Setup, Link)> {
    let IpAddr::V4(v4) = ip else { return Err("Connect to Wi-Fi or Ethernet first.".into()) };
    let provider = str0m::crypto::from_feature_flags();
    let cert = provider.dtls_provider.generate_certificate().ok_or("Could not make a certificate.")?;
    provider.install_process_default();
    let socket = UdpSocket::bind((ip, 0)).map_err(err)?;
    let raw = socket2::SockRef::from(&socket);
    let _ = raw.set_recv_buffer_size(SOCKET_BUFFER);
    let _ = raw.set_send_buffer_size(SOCKET_BUFFER);
    let port = socket.local_addr().map_err(err)?.port();
    let creds = IceCreds { ufrag: alnum(8), pass: alnum(24) };
    let fp = rtc_for(&cert, &creds).direct_api().local_dtls_fingerprint().clone();
    if fp.bytes.len() != 32 { return Err("Unexpected certificate fingerprint.".into()); }
    let mut head = vec![1u8];
    head.extend(v4.octets());
    head.extend(port.to_be_bytes());
    head.extend(&fp.bytes);
    head.extend(creds.ufrag.as_bytes());
    head.extend(creds.pass.as_bytes());
    Ok((Setup { socket, cert, creds }, Link { base: format!("{page}#"), head }))
}

/// The phone's ufrag from the USERNAME ("<ours>:<theirs>") of a STUN binding request meant for us.
fn stun_remote_ufrag(b: &[u8], ours: &str) -> Option<String> {
    if b.len() < 20 || b[0..2] != [0, 1] || b[4..8] != [0x21, 0x12, 0xA4, 0x42] { return None; }
    let mut i = 20;
    while i + 4 <= b.len() {
        let (t, l) = (u16::from_be_bytes([b[i], b[i + 1]]), u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize);
        let v = b.get(i + 4..i + 4 + l)?;
        if t == 6 {
            let (a, r) = std::str::from_utf8(v).ok()?.split_once(':')?;
            return (a == ours && !r.is_empty() && r.len() <= 256).then(|| r.to_string());
        }
        i += 4 + l.div_ceil(4) * 4;
    }
    None
}

struct Down { idx: usize, file: File, left: u64 }
struct Up { slot: usize, file: File, path: PathBuf, name: String, left: u64, len: u64, before: u64, total: u64, last: Instant }
/// One phone (or computer) that opened the page.
struct Peer { rtc: Rtc, cid: Option<ChannelId>, open: bool, hello: bool, name: String, sid: String, outbox: VecDeque<(bool, Vec<u8>)>, down: Option<Down>, up: Option<Up>, offered: Option<String>, told: bool }
impl Peer {
    fn new(cert: &DtlsCert, creds: &IceCreds, local: std::net::SocketAddr, remote_ufrag: String) -> Option<Self> {
        let mut rtc = rtc_for(cert, creds);
        rtc.add_local_candidate(Candidate::host(local, "udp").ok()?);
        let cid = {
            let mut d = rtc.direct_api();
            d.set_ice_controlling(false);
            // ICE-lite never sends checks, so the phone's password is never used; its ufrag tells this phone's checks apart.
            d.set_remote_ice_credentials(IceCreds { ufrag: remote_ufrag, pass: alnum(24) });
            // Not checked (see the top): DTLS only needs one to be set.
            d.set_remote_fingerprint(Fingerprint { hash_func: "sha-256".into(), bytes: vec![0; 32] });
            d.start_dtls(false).ok()?;
            d.start_sctp(false);
            d.create_data_channel(ChannelConfig { label: "sora".into(), negotiated: Some(0), ..Default::default() })
        };
        Some(Self { rtc, cid: Some(cid), open: false, hello: false, name: String::new(), sid: String::new(), outbox: VecDeque::new(), down: None, up: None, offered: None, told: false })
    }
    fn say(&mut self, v: serde_json::Value) { self.outbox.push_back((false, v.to_string().into_bytes())); }
    fn authed(&self, sh: &Shared) -> bool { !self.sid.is_empty() && authed(sh, &format!("sid={}", self.sid)) }
    /// An unfinished upload is removed and its place given back.
    fn drop_upload(&mut self, sh: &Shared) {
        if let Some(up) = self.up.take() {
            drop(up.file);
            let _ = std::fs::remove_file(&up.path);
            if let Ok(mut s) = sh.state.lock() { if let Some(o) = s.offer.as_mut() { if let Some(c) = o.claimed.get_mut(up.slot) { *c = false; } } }
        }
    }
}

fn on_message(p: &mut Peer, sh: &Shared, v: serde_json::Value) {
    let kind = v["t"].as_str().unwrap_or("");
    if kind == "hello" {
        let given = v["token"].as_str().unwrap_or("");
        let (now, prev) = sh.state.lock().map(|s| (s.token.clone(), s.prev.clone())).unwrap_or_default();
        if !sh.alive() { return p.say(serde_json::json!({ "t": "stopped" })); }
        if same(given, &now) {
            p.hello = true;
            p.name = v["name"].as_str().and_then(clean_device_name).unwrap_or_else(|| "Phone".into());
            return p.say(serde_json::json!({ "t": "code", "host": sh.host }));
        }
        return p.say(serde_json::json!({ "t": if !prev.is_empty() && same(given, &prev) { "used" } else { "gone" } }));
    }
    if kind == "code" {
        if !p.hello { return; }
        let given = v["code"].as_str().unwrap_or("");
        let Ok(mut st) = sh.state.lock() else { return };
        if same(given, &st.code) {
            let sid = b64(&random(24));
            let device = unique_name(p.name.clone(), &st.sessions);
            st.sessions.push((sid.clone(), device.clone())); if st.sessions.len() > MAX_SESSIONS { st.sessions.remove(0); }
            st.started = Instant::now();
            let upload = st.upload;
            drop(st);
            p.sid = sid;
            sh.say("connected", serde_json::json!({ "devices": sh.devices() }));
            sh.rotate_code();
            let files: Vec<serde_json::Value> = sh.files.iter().map(|f| serde_json::json!({ "name": f.name, "size": f.size })).collect();
            return p.say(serde_json::json!({ "t": "in", "host": sh.host, "name": device, "files": files, "upload": upload }));
        }
        st.wrong += 1;
        let locked = st.wrong >= MAX_WRONG;
        if locked { st.dead = true; }
        drop(st);
        if locked { sh.say("locked", serde_json::json!({})); return p.say(serde_json::json!({ "t": "locked" })); }
        return p.say(serde_json::json!({ "t": "wrong" }));
    }
    if !p.authed(sh) { return p.say(serde_json::json!({ "t": "out" })); }
    match kind {
        "name" => {
            let Some(name) = v["name"].as_str().and_then(clean_device_name) else { return };
            if let Ok(mut st) = sh.state.lock() {
                if let Some(i) = st.sessions.iter().position(|(k, _)| same(k, &p.sid)) {
                    let others: Vec<(String, String)> = st.sessions.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| d.clone()).collect();
                    st.sessions[i].1 = unique_name(name, &others);
                }
            }
            sh.say("connected", serde_json::json!({ "devices": sh.devices() }));
        }
        "get" => {
            if p.down.is_some() { return; }
            let Some((idx, entry)) = v["i"].as_u64().and_then(|i| sh.files.get(i as usize).map(|e| (i as usize, e))) else { return };
            let Some((file, len)) = File::open(&entry.path).ok().and_then(|f| { let m = f.metadata().ok()?; m.is_file().then(|| (f, m.len())) }) else { return p.say(serde_json::json!({ "t": "missing", "i": idx })) };
            p.say(serde_json::json!({ "t": "file", "i": idx, "name": entry.name, "size": len }));
            p.down = Some(Down { idx, file, left: len });
        }
        "offer" => {
            let open = sh.state.lock().map_or(false, |s| s.upload && s.offer.as_ref().map_or(true, |o| o.verdict != Verdict::Pending));
            if !open { return p.say(serde_json::json!({ "t": "verdict", "ok": false })); }
            let list: Option<Vec<(String, u64)>> = v["files"].as_array().map(|a| a.iter().filter_map(|f| Some((safe_name(f.get("name")?.as_str()?), f.get("size")?.as_u64()?))).collect());
            let Some(files) = list.filter(|l| (1..=MAX_UPLOADS as usize).contains(&l.len()) && l.iter().all(|f| (1..=MAX_UPLOAD).contains(&f.1))) else { return p.say(serde_json::json!({ "t": "verdict", "ok": false })) };
            let id = b64(&random(9));
            let device = device_of(sh, &format!("sid={}", p.sid));
            let view = OfferView { count: files.len(), total: files.iter().map(|f| f.1).sum(), names: files.iter().take(3).map(|f| f.0.clone()).collect(), device: device.clone() };
            if let Ok(mut st) = sh.state.lock() { st.offer = Some(Offer { id: id.clone(), claimed: vec![false; files.len()], files, done: 0, bytes_done: 0, device, verdict: Verdict::Pending, at: Instant::now() }); }
            p.offered = Some(id);
            p.told = false;
            sh.say("offer", serde_json::json!({ "offer": view }));
        }
        "put" => {
            if p.up.is_some() { return; }
            let (name, len) = (safe_name(v["name"].as_str().unwrap_or("")), v["size"].as_u64().unwrap_or(0));
            let offered = p.offered.clone().unwrap_or_default();
            // Only a file the PC accepted, once, with the size that was announced.
            let slot = sh.state.lock().ok().and_then(|mut s| {
                if !s.upload { return None; }
                let o = s.offer.as_mut().filter(|o| o.verdict == Verdict::Accepted && same(&o.id, &offered))?;
                let i = (0..o.files.len()).find(|&i| !o.claimed[i] && o.files[i].0 == name && o.files[i].1 == len)?;
                o.claimed[i] = true; Some(i)
            });
            let Some(slot) = slot else { return p.say(serde_json::json!({ "t": "refused" })) };
            let (before, total) = sh.state.lock().ok().and_then(|s| s.offer.as_ref().map(|o| (o.bytes_done, o.files.iter().map(|f| f.1).sum::<u64>()))).unwrap_or((0, len));
            match create_unique(&sh.inbox, &name) {
                Ok((file, path)) => { p.up = Some(Up { slot, file, path, name, left: len, len, before, total, last: Instant::now() }); p.say(serde_json::json!({ "t": "go" })); }
                Err(_) => { if let Ok(mut s) = sh.state.lock() { if let Some(o) = s.offer.as_mut() { o.claimed[slot] = false; } } p.say(serde_json::json!({ "t": "refused" })); }
            }
        }
        _ => {}
    }
}
/// A chunk of the file the phone is sending.
fn on_bytes(p: &mut Peer, sh: &Shared, data: &[u8]) {
    let Some(up) = p.up.as_mut() else { return };
    if data.len() as u64 > up.left || up.file.write_all(data).is_err() { p.drop_upload(sh); return p.say(serde_json::json!({ "t": "refused" })); }
    up.left -= data.len() as u64;
    let done = up.before + (up.len - up.left);
    if up.left > 0 {
        if up.last.elapsed() >= Duration::from_millis(150) { up.last = Instant::now(); sh.say("progress", serde_json::json!({ "done": done, "total": up.total, "name": up.name })); }
        return;
    }
    let Some(up) = p.up.take() else { return };
    drop(up.file);
    if !sh.alive() { let _ = std::fs::remove_file(&up.path); return; }
    let finished = sh.state.lock().map_or(false, |mut s| s.offer.as_mut().map_or(false, |o| { o.done += 1; o.bytes_done += up.len; o.done == o.files.len() }));
    sh.say("progress", serde_json::json!({ "done": done, "total": up.total, "name": up.name }));
    sh.say("uploaded", serde_json::json!({ "name": up.path.file_name().map_or(up.name.clone(), |n| n.to_string_lossy().into_owned()) }));
    p.say(serde_json::json!({ "t": "saved" }));
    if finished { sh.rotate(); }
}

/// Sends what waits (messages first), then more of the file being downloaded while the channel has room.
fn pump(p: &mut Peer, sh: &Shared) {
    let (Some(cid), true) = (p.cid, p.open) else { return };
    let mut finished = None;
    {
        let Some(mut ch) = p.rtc.channel(cid) else { return };
        while let Some((bin, data)) = p.outbox.front() {
            match ch.write(*bin, data) { Ok(true) => { p.outbox.pop_front(); } _ => return }
        }
        let mut buf = vec![0u8; CHUNK];
        while let Some(d) = p.down.as_mut() {
            if d.left == 0 { finished = p.down.take().map(|d| d.idx); break; }
            if ch.buffered_amount() >= HIGH_WATER { break; }
            let n = match d.file.read(&mut buf[..CHUNK.min(d.left as usize)]) { Ok(n) if n > 0 => n, _ => { p.down = None; p.outbox.push_back((false, serde_json::json!({ "t": "failed" }).to_string().into_bytes())); break; } };
            d.left -= n as u64;
            // Not taken now (the buffer is full): it goes out first next time.
            if !matches!(ch.write(true, &buf[..n]), Ok(true)) { p.outbox.push_back((true, buf[..n].to_vec())); break; }
        }
    }
    if let Some(idx) = finished {
        p.say(serde_json::json!({ "t": "end", "i": idx }));
        if let Some(e) = sh.files.get(idx) { sh.say("downloaded", serde_json::json!({ "name": e.name })); }
        let all = sh.state.lock().map_or(false, |mut s| { if let Some(g) = s.got.get_mut(idx) { *g = true; } !s.got.is_empty() && s.got.iter().all(|g| *g) });
        if all { sh.rotate(); }
    }
}
/// Runs the phone's connection until it has nothing to do before the returned time.
fn drive(p: &mut Peer, sh: &Shared, socket: &UdpSocket) -> Instant {
    loop {
        if !p.rtc.is_alive() { return Instant::now(); }
        match p.rtc.poll_output() {
            Ok(Output::Timeout(t)) => return t,
            Ok(Output::Transmit(t)) => { let _ = socket.send_to(&t.contents, t.destination); }
            Ok(Output::Event(e)) => match e {
                Event::IceConnectionStateChange(IceConnectionState::Disconnected) | Event::ChannelClose(_) => p.rtc.disconnect(),
                Event::ChannelOpen(id, _) => { p.cid = Some(id); p.open = true; }
                Event::ChannelData(d) => {
                    if d.binary { on_bytes(p, sh, &d.data); }
                    else if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&d.data) { on_message(p, sh, v); }
                }
                _ => {}
            },
            Err(_) => { p.rtc.disconnect(); return Instant::now(); }
        }
    }
}

pub(super) fn serve(setup: Setup, sh: Arc<Shared>) {
    let Setup { socket, cert, creds, .. } = setup;
    let Ok(local) = socket.local_addr() else { return };
    let (mut peers, mut buf, mut token) = (Vec::<Peer>::new(), vec![0u8; 2048], sh.token());
    while sh.alive() {
        if sh.state.lock().map_or(false, |s| s.code_at.elapsed() >= sh.code_every) { sh.rotate_code(); }
        // The link was replaced (used, declined, "Not you?"): everybody who was in is out.
        let now_token = sh.token();
        if now_token != token {
            token = now_token;
            for p in &mut peers { p.down = None; p.drop_upload(&sh); p.offered = None; p.say(serde_json::json!({ "t": "rotated" })); }
        }
        // The PC's answer to an offer (or no answer in time) goes back to the phone that made it.
        let pending = sh.state.lock().ok().and_then(|s| s.offer.as_ref().map(|o| (o.id.clone(), o.verdict, o.at.elapsed())));
        if matches!(pending, Some((_, Verdict::Pending, waited)) if waited > OFFER_WAIT) { sh.decide(false); }
        for p in &mut peers {
            let Some(id) = p.offered.clone() else { continue };
            if p.told { continue; }
            let verdict = sh.state.lock().ok().and_then(|s| s.offer.as_ref().filter(|o| same(&o.id, &id)).map(|o| o.verdict));
            match verdict { Some(Verdict::Pending) => {}, Some(Verdict::Accepted) => { p.told = true; p.say(serde_json::json!({ "t": "verdict", "ok": true })); }
                _ => { p.told = true; p.offered = None; p.say(serde_json::json!({ "t": "verdict", "ok": false })); } }
        }
        let mut wake = Instant::now() + Duration::from_millis(50);
        for p in &mut peers { pump(p, &sh); wake = wake.min(drive(p, &sh, &socket)); }
        peers.retain_mut(|p| { if p.rtc.is_alive() { return true; } p.drop_upload(&sh); false });
        let _ = socket.set_read_timeout(Some(wake.saturating_duration_since(Instant::now()).clamp(Duration::from_millis(1), Duration::from_millis(50))));
        // Wait for the first datagram, then take everything already queued: one per loop made uploads 3.5x slower.
        let mut got = socket.recv_from(&mut buf).ok();
        if got.is_some() { let _ = socket.set_nonblocking(true); }
        for _ in 0..512 {
            let Some((n, source)) = got else { break };
            let data = &buf[..n];
            if let Ok(contents) = data.try_into() {
                let input = Input::Receive(Instant::now(), Receive { proto: Protocol::Udp, source, destination: local, contents });
                if let Some(p) = peers.iter_mut().find(|p| p.rtc.accepts(&input)) { let _ = p.rtc.handle_input(input); }
                else if peers.len() < MAX_PEERS {
                    if let Some(mut p) = stun_remote_ufrag(data, &creds.ufrag).and_then(|u| Peer::new(&cert, &creds, local, u)) {
                        if p.rtc.accepts(&input) && p.rtc.handle_input(input).is_ok() { peers.push(p); }
                    }
                }
            }
            got = socket.recv_from(&mut buf).ok();
        }
        let _ = socket.set_nonblocking(false);
        let now = Instant::now();
        for p in &mut peers { let _ = p.rtc.handle_input(Input::Timeout(now)); }
    }
    for p in &mut peers { p.drop_upload(&sh); p.rtc.disconnect(); }
    // Ended on its own (time out, too many wrong codes): tell the panel. Stopped from outside: stay quiet.
    if !sh.stop.swap(true, Ordering::SeqCst) { (sh.notify)("ended", serde_json::json!({})); }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Run by tests/rtc-e2e.test.cjs, which opens the page in a browser: prints the link and the code (again after each
    /// change), accepts what the phone offers, and reports what arrived.
    #[test]
    #[ignore]
    fn serves_a_browser_for_the_e2e_script() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        // E2E_DOWN_MB: a big file instead, to measure downloads.
        let big = std::env::var("E2E_DOWN_MB").ok().and_then(|m| m.parse::<usize>().ok());
        std::fs::write(&file, big.map_or(b"hello from the PC".to_vec(), |m| vec![7u8; m << 20])).unwrap();
        let size = std::fs::metadata(&file).unwrap().len();
        let (setup, link) = setup("127.0.0.1".parse().unwrap(), "PAGE/").unwrap();
        let say = |kind: &str, d: serde_json::Value| { if let Some(i) = d.get("info") { println!("LINK {} {}", i["url"].as_str().unwrap(), i["code"].as_str().unwrap()); } println!("EVENT {kind} {d}"); };
        let sh = Arc::new(Shared { code_every: Duration::from_secs(3600), origin: String::new(), link: Some(link), host: "TEST-PC".into(), files: vec![Entry { path: file, name: "hello.txt".into(), size }],
            state: Mutex::new(State::new(true, 1)), stop: AtomicBool::new(false), inbox: dir.path().join("in"), notify: Box::new(say) });
        let i = sh.info().unwrap();
        println!("INBOX {}", dir.path().join("in").display());
        println!("LINK {} {}", i.url, i.code);
        let (s2, server) = (sh.clone(), std::thread::spawn(move || serve(setup, sh)));
        let until = Instant::now() + Duration::from_secs(std::env::var("E2E_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(60));
        while Instant::now() < until && !std::path::Path::new(&dir.path().join("in").join("stop.txt")).exists() {
            if s2.offer_view().is_some() { s2.decide(true); }
            std::thread::sleep(Duration::from_millis(100));
        }
        for e in std::fs::read_dir(dir.path().join("in")).into_iter().flatten().flatten() { println!("GOT {} {}", e.file_name().to_string_lossy(), e.metadata().unwrap().len()); }
        s2.stop.store(true, Ordering::SeqCst);
        let _ = server.join();
        println!("DONE");
    }
    fn stun(username: &str) -> Vec<u8> {
        let mut b = vec![0, 1, 0, 0, 0x21, 0x12, 0xA4, 0x42];
        b.extend([7u8; 12]);
        let u = username.as_bytes();
        b.extend(6u16.to_be_bytes()); b.extend((u.len() as u16).to_be_bytes()); b.extend(u);
        b.resize(b.len() + (4 - u.len() % 4) % 4, 0);
        b
    }
    #[test]
    fn learns_the_phone_from_its_first_check() {
        assert_eq!(stun_remote_ufrag(&stun("pcUfrag1:aEX7"), "pcUfrag1").as_deref(), Some("aEX7"));
        assert_eq!(stun_remote_ufrag(&stun("someone:aEX7"), "pcUfrag1"), None, "a check for another agent");
        assert_eq!(stun_remote_ufrag(&stun("pcUfrag1:"), "pcUfrag1"), None);
        assert_eq!(stun_remote_ufrag(b"\x16\xfe\xfd not stun at all, a DTLS record", "pcUfrag1"), None);
        assert_eq!(stun_remote_ufrag(&stun("pcUfrag1:x")[..22], "pcUfrag1"), None, "cut short");
    }
    #[test]
    fn the_qr_carries_everything_the_page_needs() {
        let (s, link) = setup("127.0.0.1".parse().unwrap(), "https://example.test/s/").unwrap();
        let url = link.url("TOKEN");
        let (page, frag) = url.split_once('#').unwrap();
        assert_eq!(page, "https://example.test/s/");
        let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(frag).unwrap();
        assert_eq!(b[0], 1);
        assert_eq!(&b[1..5], &[127, 0, 0, 1]);
        assert_eq!(u16::from_be_bytes([b[5], b[6]]), s.socket.local_addr().unwrap().port());
        assert_eq!(&b[39..47], s.creds.ufrag.as_bytes());
        assert_eq!(&b[47..71], s.creds.pass.as_bytes());
        assert_eq!(&b[71..], b"TOKEN");
        assert!(s.creds.ufrag.bytes().chain(s.creds.pass.bytes()).all(|c| c.is_ascii_alphanumeric()), "ICE allows no - or _");
    }
}
