//! Send → iPhone / Mac, no app needed: a short-lived HTTPS page on the local network. Sora shows a QR code (the address, with a
//! 192-bit random token) and a separate 6-digit code. The phone scans the QR, opens the page in Safari and must type the code
//! before it sees any file name or can download anything; five wrong codes end the session. Everything travels over TLS 1.3
//! (rustls, a certificate made for this session only), nothing is cached, and the page stops after 30 minutes of nobody using it or when Disconnect is pressed. The code changes every minute and each time somebody gets in.
//! The phone may also send files to this PC, only when it was allowed in Send.
//! The certificate: with the share service set up (share_cert.rs) the page has a trusted name and the browser shows no warning;
//! otherwise it is self-signed, so Safari shows its "not private" warning once and cannot pin the certificate; someone
//! who can intercept the Wi-Fi traffic and also learns the code could still impersonate the page.
use std::{fs::{File, OpenOptions}, io::{BufRead, BufReader, Read, Write}, net::{IpAddr, TcpListener}, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc, Mutex}, time::{Duration, Instant}};
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use rustls::{pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}, ServerConfig, ServerConnection, StreamOwned};
use serde::Serialize;
use tauri::{Emitter, Manager};
use crate::{err, AppResult, SendQueue};
#[path = "rtc_share.rs"]
mod rtc;

/// How long the link lives without anybody using it. Entering, downloading and uploading keep it going.
const LIFETIME: Duration = Duration::from_secs(30 * 60);
/// The code changes this often, whether or not anybody comes in.
const CODE_EVERY: Duration = Duration::from_secs(60);
const MAX_WRONG: u32 = 5;
const MAX_CONNECTIONS: usize = 8;
const MAX_UPLOAD: u64 = 2 << 30;
const MAX_UPLOADS: u32 = 100;
const MAX_SESSIONS: usize = 4;

struct Entry { path: PathBuf, name: String, size: u64 }
#[derive(Clone, Copy, PartialEq)]
enum Verdict { Pending, Accepted, Declined }
/// What the phone wants to send: nothing is accepted until it is approved on the PC.
struct Offer { id: String, files: Vec<(String, u64)>, claimed: Vec<bool>, done: usize, bytes_done: u64, device: String, verdict: Verdict, at: Instant }
struct State { wrong: u32, dead: bool, sessions: Vec<(String, String)>, upload: bool, offer: Option<Offer>, token: String, code: String, prev: String, started: Instant, code_at: Instant, got: Vec<bool> }
impl State {
    fn new(upload: bool, files: usize) -> Self { Self { wrong: 0, dead: false, sessions: Vec::new(), upload, offer: None, token: b64(&random(24)), code: new_code(), prev: String::new(), started: Instant::now(), code_at: Instant::now(), got: vec![false; files] } }
}
/// `link`: the QR opens the hosted page and connects over WebRTC (rtc_share.rs); None: the HTTPS page on this PC.
struct Shared { code_every: Duration, origin: String, link: Option<rtc::Link>, host: String, files: Vec<Entry>, state: Mutex<State>, stop: AtomicBool, inbox: PathBuf, notify: Box<dyn Fn(&str, serde_json::Value) + Send + Sync> }
impl Shared {
    fn alive(&self) -> bool { !self.stop.load(Ordering::SeqCst) && self.state.lock().map_or(false, |s| !s.dead && s.started.elapsed() < LIFETIME) }
    fn token(&self) -> String { self.state.lock().map(|s| s.token.clone()).unwrap_or_default() }
    /// The address (with its QR) and the code of the link that is valid now.
    fn info(&self) -> Option<ShareInfo> {
        let s = self.state.lock().ok()?;
        let url = self.link.as_ref().map_or_else(|| format!("{}/s/{}", self.origin, s.token), |l| l.url(&s.token));
        let qr = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::M).ok()?;
        let modules = qr.to_colors().iter().map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' }).collect();
        // Trusted: a real name with a real certificate (share_cert.rs); otherwise the bare address and the browser warns.
        let trusted = self.link.is_some() || self.origin.trim_start_matches("https://").rsplit_once(':').map_or(false, |(h, _)| h.parse::<IpAddr>().is_err());
        Some(ShareInfo { url, code: s.code.clone(), width: qr.width(), modules, seconds: self.code_every.saturating_sub(s.code_at.elapsed()).as_secs().max(1), trusted })
    }
    /// One use only: when every file has been downloaded the link, the code and the phone's access are burned and new ones made.
    fn rotate(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.prev = std::mem::replace(&mut s.token, b64(&random(24)));
            s.code = new_code(); s.code_at = Instant::now(); s.sessions.clear(); s.wrong = 0; s.offer = None; s.started = Instant::now(); s.got.iter_mut().for_each(|g| *g = false);
        }
        self.say("rotated", serde_json::json!({ "info": self.info() }));
    }
    /// A new code (the link and everybody who is in stay): every minute, and each time somebody gets in with the current one.
    fn rotate_code(&self) {
        if let Ok(mut s) = self.state.lock() { s.code = new_code(); s.wrong = 0; s.code_at = Instant::now(); }
        self.say("code", serde_json::json!({ "info": self.info() }));
    }
    /// A stopped (replaced or disconnected) session says nothing more: its late events would close or overwrite the new one in the panel.
    fn say(&self, kind: &str, data: serde_json::Value) { if !self.stop.load(Ordering::SeqCst) { (self.notify)(kind, data); } }
    /// The phones and computers that have opened the page with the right code.
    fn devices(&self) -> Vec<String> { self.state.lock().map(|s| s.sessions.iter().map(|d| d.1.clone()).collect()).unwrap_or_default() }
    /// The PC's answer to an offer. A refusal burns the link like any other use.
    fn decide(&self, accept: bool) {
        let ok = self.state.lock().map_or(false, |mut s| match s.offer.as_mut() { Some(o) if o.verdict == Verdict::Pending => { o.verdict = if accept { Verdict::Accepted } else { Verdict::Declined }; true } _ => false });
        if !ok { return; }
        self.say("offer-closed", serde_json::json!({ "accepted": accept }));
        if !accept { self.rotate(); }
    }
    fn offer_view(&self) -> Option<OfferView> {
        let s = self.state.lock().ok()?;
        let o = s.offer.as_ref().filter(|o| o.verdict == Verdict::Pending)?;
        Some(OfferView { count: o.files.len(), total: o.files.iter().map(|f| f.1).sum(), names: o.files.iter().take(3).map(|f| f.0.clone()).collect(), device: o.device.clone() })
    }
}
static CURRENT: Mutex<Option<Arc<Shared>>> = Mutex::new(None);

fn random(n: usize) -> Vec<u8> { let mut b = vec![0u8; n]; let _ = SystemRandom::new().fill(&mut b); b }
fn b64(b: &[u8]) -> String { base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b) }
/// Six digits, uniform (values from the biased top of the u32 range are thrown away).
fn new_code() -> String {
    loop {
        let v = u32::from_le_bytes(random(4).try_into().unwrap_or([0; 4]));
        if v < 4_294_000_000 { return format!("{:06}", v % 1_000_000); }
    }
}
/// Compares without stopping at the first difference (the length is not secret: tokens and codes have fixed sizes).
fn same(a: &str, b: &str) -> bool { a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0 }

// ---------------------------------------------------------------- names
/// What the browser says about the device. An iPhone's browser gives no model, an Android one usually does.
fn device_name(agent: &str) -> String {
    if agent.contains("iPhone") { return "iPhone".into(); }
    if agent.contains("iPad") { return "iPad".into(); }
    if let Some(i) = agent.find("Android") {
        let model = agent[i..].split(';').nth(1).map(|m| m.split(')').next().unwrap_or(m).split(" Build").next().unwrap_or(m).trim()).unwrap_or("");
        return if model.chars().count() > 1 && model != "wv" { model.chars().take(32).collect() } else { "Android phone".into() };
    }
    if agent.contains("Macintosh") { return "Mac".into(); }
    if agent.contains("Windows") { return "Windows PC".into(); }
    "Phone".into()
}
/// "iPhone", then "iPhone 2" for a second one.
fn unique_name(name: String, taken: &[(String, String)]) -> String {
    let same_kind = |n: &str| taken.iter().any(|t| t.1 == n);
    if !same_kind(&name) { return name; }
    (2..20).map(|i| format!("{name} {i}")).find(|n| !same_kind(n)).unwrap_or(name)
}
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(v); i += 3; continue; }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] }); i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
pub(crate) fn percent_encode(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || b"-._~".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}
/// A file name that is safe to create on Windows: no folders, no reserved characters or device names, bounded.
fn safe_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut n: String = last.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).collect();
    n = n.trim_matches(|c: char| c == '.' || c == ' ').chars().take(120).collect();
    let stem = n.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit());
    if n.is_empty() { "file".into() } else if reserved { format!("_{n}") } else { n }
}
/// Creates the file without replacing anything: "name.ext", then "name (2).ext", ...
fn create_unique(dir: &Path, name: &str) -> std::io::Result<(File, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let (stem, ext) = match name.rfind('.') { Some(i) if i > 0 => (&name[..i], &name[i..]), _ => (name, "") };
    for n in 1..1000 {
        let path = dir.join(if n == 1 { name.to_string() } else { format!("{stem} ({n}){ext}") });
        match OpenOptions::new().write(true).create_new(true).open(&path) { Ok(f) => return Ok((f, path)), Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue, Err(e) => return Err(e) }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "too many files with that name"))
}
fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;") }
fn size_text(n: u64) -> String {
    for (u, s) in [("GB", 1u64 << 30), ("MB", 1 << 20), ("KB", 1 << 10)] { if n >= s { return format!("{:.1} {u}", n as f64 / s as f64); } }
    format!("{n} B")
}

// ---------------------------------------------------------------- the pages
const STYLE: &str = r##"<style>
:root{color-scheme:dark}*{box-sizing:border-box;-webkit-tap-highlight-color:transparent}
html{background:#050506}
body{margin:0;min-height:100dvh;font:16px/1.45 -apple-system,BlinkMacSystemFont,"SF Pro Text",system-ui,sans-serif;color:#f4f4f5;-webkit-font-smoothing:antialiased}
main{width:min(100% - 48px,420px);min-height:100dvh;margin:0 auto;display:flex;flex-direction:column;padding:max(26px,env(safe-area-inset-top)) 0 max(22px,env(safe-area-inset-bottom))}
.brand{display:flex;align-items:center;gap:9px;font-size:15px;font-weight:600;letter-spacing:-.01em;color:#8b8b93}.brand i{width:8px;height:8px;border-radius:50%;background:#c3adff}
.stage{flex:1;display:flex;flex-direction:column;justify-content:center;padding-bottom:9vh}.stage.top{justify-content:flex-start;padding:52px 0 0}
h1{margin:0;font-size:36px;line-height:1.08;font-weight:650;letter-spacing:-.035em}
.sub{margin:10px 0 0;font-size:16px;color:#8b8b93}
form{margin:0}
#code{display:block;width:100%;margin:46px 0 0;padding:0;border:0;border-radius:0;outline:0;background:none;color:#fff;caret-color:#c3adff;font:500 48px/1 ui-rounded,-apple-system,system-ui,sans-serif;font-variant-numeric:tabular-nums;letter-spacing:.3em}
#code::placeholder{color:#ffffff26}
.track{height:2px;margin-top:16px;border-radius:1px;background:#ffffff1c;overflow:hidden}
.track b{display:block;width:0;height:100%;background:#c3adff;transition:width .22s cubic-bezier(.2,.8,.2,1)}
.err{min-height:22px;margin:14px 0 0;font-size:14px;color:#ff9b93}
.go{margin-top:8px;padding:0;border:0;background:none;color:#8b8b93;font:inherit;font-size:15px}
.rows{margin-top:30px}
.file{display:flex;align-items:center;gap:14px;margin-top:10px;padding:15px 16px 15px 20px;border-radius:22px;background:#0e0e10;color:inherit;text-decoration:none;transition:background .15s,transform .15s}
.file:active{transform:scale(.985);background:#151518}
.name{flex:1;min-width:0}.name b{display:block;overflow:hidden;font-size:16px;font-weight:550;text-overflow:ellipsis;white-space:nowrap}.name span{font-size:13px;color:#8b8b93}
.dot{flex:none;display:grid;place-items:center;width:40px;height:40px;border-radius:50%;background:#c3adff;color:#170f2a}.dot.ghost{background:#ffffff14;color:#c3adff}.dot svg{width:18px;height:18px}
.up{cursor:pointer}.up input{display:none}.up.off{opacity:.45;pointer-events:none}
.xfer{margin-top:12px;padding:18px 20px 16px;border-radius:22px;background:#0e0e10}
.xtop{display:flex;align-items:baseline;justify-content:space-between;gap:16px}
.xtop b{min-width:0;overflow:hidden;font-size:15px;font-weight:550;text-overflow:ellipsis;white-space:nowrap}
.xtop span{flex:none;font-size:30px;font-weight:600;letter-spacing:-.03em;font-variant-numeric:tabular-nums;color:#c3adff}
.xtrack{position:relative;height:4px;margin-top:14px;border-radius:2px;background:#ffffff14;overflow:hidden}
.xtrack i{position:absolute;top:0;bottom:0;left:0;width:100%;border-radius:2px;background:#c3adff;transform-origin:left;transform:scaleX(0);transition:transform .25s linear}
.xfer[data-state=wait] .xtrack i{width:38%;transform:none;transition:none;animation:slide 1.3s cubic-bezier(.4,0,.2,1) infinite}
@keyframes slide{0%{transform:translateX(-100%)}100%{transform:translateX(265%)}}
#xinfo{margin:12px 0 0;font-size:13px;color:#8b8b93}
.xfer[data-state=done] #xinfo{color:#c3adff}
.xfer[data-state=fail] .xtop span,.xfer[data-state=fail] #xinfo{color:#ff9b93}.xfer[data-state=fail] .xtrack i{background:#ff9b93;transform:scaleX(1)}
@media (prefers-reduced-motion:reduce){.xfer[data-state=wait] .xtrack i{animation:none;width:100%;opacity:.5}}
.who{margin:14px 0 0;font-size:13px;color:#8b8b93}.who b{color:#f4f4f5;font-weight:550}.who button{margin-left:10px;padding:0;border:0;background:none;color:#c3adff;font:inherit;font-size:13px}
.who[hidden],.who-form[hidden]{display:none}.who-form{display:flex;gap:8px;margin-top:12px}.who-form input{flex:1;min-width:0;height:40px;padding:0 16px;border:0;border-radius:20px;background:#ffffff12;color:#fff;font:inherit;font-size:15px;outline:0;caret-color:#c3adff}.who-form button{height:40px;padding:0 18px;border:0;border-radius:20px;background:#c3adff;color:#170f2a;font:inherit;font-size:14px;font-weight:600}
.foot{padding-top:24px;font-size:12px;color:#ffffff40}
.shake{animation:shake .35s}@keyframes shake{20%{transform:translateX(-8px)}50%{transform:translateX(6px)}80%{transform:translateX(-3px)}}
</style>"##;
fn page(body: &str, token: Option<&str>) -> Vec<u8> {
    let s = token.map_or(String::new(), |t| format!("<script src=\"/s/{t}/a.js\"></script>"));
    format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1,viewport-fit=cover\"><meta name=\"robots\" content=\"noindex\"><meta name=\"theme-color\" content=\"#050506\"><title>Sora</title>{STYLE}</head><body><main><div class=\"brand\"><i></i>Sora</div>{body}<div class=\"foot\">Encrypted on your network</div></main>{s}</body></html>").into_bytes()
}
fn stopped_page(why: &str) -> Vec<u8> { page(&format!("<div class=\"stage\"><h1>Sharing has stopped</h1><p class=\"sub\">{why}</p></div>"), None) }
fn done_page() -> Vec<u8> { page("<div class=\"stage\"><h1>All done</h1><p class=\"sub\">This link was used. Ask for a new QR code on the PC.</p></div>", None) }
fn unlock_page(token: &str, host: &str, wrong: bool) -> Vec<u8> {
    page(&format!("<div class=\"stage\"><h1>Enter the code</h1><p class=\"sub\">Shown on {}.</p><form method=\"post\" action=\"/s/{token}/unlock\"><input id=\"code\" class=\"{}\" name=\"code\" type=\"text\" inputmode=\"numeric\" pattern=\"[0-9]*\" maxlength=\"6\" autocomplete=\"off\" autocorrect=\"off\" placeholder=\"······\" aria-label=\"Code\" required autofocus><div class=\"track\"><b id=\"fill\"></b></div><p class=\"err\">{}</p><noscript><button class=\"go\">Open</button></noscript></form></div>", escape(host), if wrong { "shake" } else { "" }, if wrong { "That code is not right." } else { "" }), Some(token))
}
const ARROW: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M12 4v11m0 0 5-5m-5 5-5-5M5 20h14\"/></svg>";
const PLUS: &str = "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\"><path d=\"M12 5v14M5 12h14\"/></svg>";
/// What the PC calls this phone, and a way to change it (the browser cannot tell an iPhone 15 Pro from a 13).
const WHO: &str = "<p class=\"who\" id=\"who\">Showing as <b id=\"who-name\"></b><button id=\"who-edit\" type=\"button\">Rename</button></p><form class=\"who-form\" id=\"who-form\" hidden><input id=\"who-input\" maxlength=\"32\" autocomplete=\"off\" aria-label=\"Device name\"><button>Save</button></form>";
fn list_page(sh: &Shared, token: &str, upload: bool) -> Vec<u8> {
    let n = sh.files.len();
    let total: u64 = sh.files.iter().map(|f| f.size).sum();
    let (title, sub) = if n == 0 { ("Send to this PC".to_string(), format!("From your phone to {}.", escape(&sh.host))) } else { (if n == 1 { "1 file".to_string() } else { format!("{n} files") }, format!("{} · from {}", size_text(total), escape(&sh.host))) };
    let mut rows = String::new();
    for (i, f) in sh.files.iter().enumerate() {
        rows.push_str(&format!("<a class=\"file\" href=\"/s/{token}/f/{i}\"><span class=\"name\"><b>{}</b><span>{}</span></span><span class=\"dot\">{ARROW}</span></a>", escape(&f.name), size_text(f.size)));
    }
    let (off, hint) = if upload { ("", "Photos or files") } else { (" off", "Turn on \"Phone can send files here\" in Sora") };
    rows.push_str(&format!("<label class=\"file up{off}\" id=\"upload\"><span class=\"name\"><b>Send to this PC</b><span id=\"hint\">{hint}</span></span><span class=\"dot ghost\">{PLUS}</span><input id=\"pick\" type=\"file\" multiple{}></label><div class=\"xfer\" id=\"xfer\" data-state=\"wait\" hidden><div class=\"xtop\"><b id=\"xname\"></b><span id=\"xpct\"></span></div><div class=\"xtrack\"><i id=\"xbar\"></i></div><p id=\"xinfo\"></p></div>", if upload { "" } else { " disabled" }));
    page(&format!("<div class=\"stage top\"><h1>{title}</h1><p class=\"sub\">{sub}</p>{WHO}<div class=\"rows\">{rows}</div></div>"), Some(token))
}
fn script() -> Vec<u8> {
    br#"const t=location.pathname.split('/')[2],$=s=>document.querySelector(s),code=$('#code'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
if(code){const fill=$('#fill');code.oninput=()=>{code.value=code.value.replace(/\D/g,'').slice(0,6);fill.style.width=code.value.length/6*100+'%';if(code.value.length===6)code.form.submit()};code.focus();code.oninput()}
const who=$('#who');
if(who){
const names={S938:'S25 Ultra',S936:'S25+',S931:'S25',S928:'S24 Ultra',S926:'S24+',S921:'S24',S918:'S23 Ultra',S916:'S23+',S911:'S23',S908:'S22 Ultra',S906:'S22+',S901:'S22',F956:'Z Fold6',F741:'Z Flip6',F946:'Z Fold5',F731:'Z Flip5',A556:'A55',A546:'A54',A356:'A35',A156:'A15'};
const friendly=m=>{m=(m||'').trim();let x;if(!m||m==='K')return 'Android phone';
if(/^Pixel/i.test(m))return m;
if(x=m.match(/^SM-([A-Z]\d{3})/))return 'Samsung '+(names[x[1]]||m);
if(/^CPH/.test(m))return 'OPPO '+m;if(/^RMX/.test(m))return 'Realme '+m;if(/^V\d{4}/.test(m))return 'vivo '+m;
if(/^moto/i.test(m))return 'Motorola '+m;
if(/^(LE|NE|IN|KB|HD|GM)\d{4}/.test(m))return 'OnePlus '+m;
if(/^(M\d{4}[A-Z]\d{1,2}[A-Z]{0,2}|\d{7,8}[A-Z0-9]{1,3})$/.test(m))return 'Xiaomi '+m;
return m};
const guess=async()=>{const ua=navigator.userAgent;
if(/iPhone/.test(ua))return 'iPhone';if(/iPad/.test(ua)||(/Macintosh/.test(ua)&&navigator.maxTouchPoints>1))return 'iPad';
if(/Android/.test(ua)){let m='';try{m=(await navigator.userAgentData.getHighEntropyValues(['model'])).model}catch(e){}
if(!m){const y=ua.match(/Android[^;]*;\s*([^;)]+)/);m=y?y[1].replace(/\s*Build.*/,''):''}return friendly(m)}
if(/Macintosh/.test(ua))return 'Mac';if(/Windows/.test(ua))return 'Windows PC';return 'Phone'};
const tell=n=>fetch('/s/'+t+'/name',{method:'POST',headers:{'X-Sora':'1','Content-Type':'application/json'},body:JSON.stringify({name:n})}).catch(()=>{});
const named=n=>{$('#who-name').textContent=n;$('#who-input').value=n};
(async()=>{let n=null;try{n=localStorage.getItem('sora-device-name')}catch(e){}if(!n)n=await guess();named(n);tell(n)})();
$('#who-edit').onclick=()=>{$('#who-form').hidden=false;$('#who').hidden=true;$('#who-input').focus();$('#who-input').select()};
$('#who-form').onsubmit=e=>{e.preventDefault();const n=$('#who-input').value.trim();if(n){try{localStorage.setItem('sora-device-name',n)}catch(x){}named(n);tell(n)}$('#who-form').hidden=true;$('#who').hidden=false};
}
const pick=$('#pick');
if(pick){
const row=$('#upload'),hint=$('#hint'),x=$('#xfer'),busy=()=>x.dataset.state==='wait'||x.dataset.state==='send';
const poll=()=>{if(busy())return setTimeout(poll,2500);fetch('/s/'+t+'/s').then(r=>r.ok?r.json():Promise.reject()).then(j=>{row.classList.toggle('off',!j.upload);pick.disabled=!j.upload;hint.textContent=j.upload?'Photos or files':'Turn on "Phone can send files here" in Sora';setTimeout(poll,2500)}).catch(()=>{})};
setTimeout(poll,2500);
const show=(state,name,pct,info)=>{x.hidden=false;x.dataset.state=state;$('#xname').textContent=name;$('#xpct').textContent=pct;$('#xbar').style.transform=(state==='send'||state==='done')?'scaleX('+((parseFloat(pct)||0)/100)+')':'';$('#xinfo').textContent=info};
const fail=(name,why)=>show('fail',name,'',why);
pick.onchange=async()=>{
const list=[...pick.files];pick.value='';if(!list.length)return;
const total=list.reduce((n,f)=>n+f.size,0),label=list.length===1?list[0].name:list.length+' files';
show('wait',label,'','Waiting for approval on the PC');
let id;
try{const r=await fetch('/s/'+t+'/offer',{method:'POST',headers:{'X-Sora':'1','Content-Type':'application/json'},body:JSON.stringify({files:list.map(f=>({name:f.name,size:f.size}))})});
if(r.status===403)return fail(label,'The PC is not accepting files right now');if(!r.ok)return fail(label,'Could not ask the PC');id=(await r.json()).id}catch(e){return fail(label,'Connection lost')}
let st='pending';
for(let i=0;i<100&&st==='pending';i++){await sleep(700);try{const r=await fetch('/s/'+t+'/offer/'+id);st=r.ok?(await r.json()).status:'gone'}catch(e){st='gone'}}
if(st==='declined')return fail(label,'Declined on the PC');
if(st!=='accepted')return fail(label,'No answer from the PC');
let sent=0;
for(let i=0;i<list.length;i++){const f=list[i];
const ok=await new Promise(res=>{const q=new XMLHttpRequest();q.open('PUT','/s/'+t+'/u?o='+encodeURIComponent(id)+'&name='+encodeURIComponent(f.name));q.setRequestHeader('X-Sora','1');
q.upload.onprogress=e=>show('send',list.length===1?f.name:(i+1)+' of '+list.length+' \u00b7 '+f.name,Math.min(99,Math.floor((sent+e.loaded)/total*100))+'%','Sending to the PC');
q.onload=()=>res(q.status===201);q.onerror=()=>res(false);q.send(f)});
if(!ok)return fail(f.name,'Not sent. Ask the PC for a new QR code');sent+=f.size}
show('done',label,'100%','Sent to the PC')}}"#.to_vec()
}

// ---------------------------------------------------------------- http
struct Head { method: String, path: String, query: String, cookie: String, agent: String, length: Option<u64>, marked: bool }
fn read_head<R: BufRead>(r: &mut R) -> Option<Head> {
    let mut line = String::new();
    let mut total = 0;
    let mut next = |r: &mut R, line: &mut String| -> Option<()> { line.clear(); let n = Read::take(&mut *r, 4096).read_line(line).ok()?; total += n; (n > 0 && total <= 16384 && line.ends_with('\n')).then_some(()) };
    next(r, &mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let (path, query) = target.split_once('?').map_or((target.clone(), String::new()), |(p, q)| (p.to_string(), q.to_string()));
    let mut head = Head { method, path, query, cookie: String::new(), agent: String::new(), length: None, marked: false };
    for _ in 0..64 {
        next(r, &mut line)?;
        let l = line.trim_end();
        if l.is_empty() { return Some(head); }
        let (k, v) = l.split_once(':')?;
        match k.trim().to_ascii_lowercase().as_str() { "cookie" => head.cookie = v.trim().to_string(), "user-agent" => head.agent = v.trim().chars().take(300).collect(), "content-length" => head.length = v.trim().parse().ok(), "x-sora" => head.marked = true, _ => {} }
    }
    None
}
fn respond<W: Write>(w: &mut W, status: &str, kind: &str, extra: &[(&str, String)], len: u64, body: &[u8]) -> std::io::Result<()> {
    let mut h = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {len}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'self'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'\r\nConnection: close\r\n");
    for (k, v) in extra { h.push_str(&format!("{k}: {v}\r\n")); }
    h.push_str("\r\n");
    w.write_all(h.as_bytes())?; w.write_all(body)?; w.flush()
}
fn simple<W: Write>(w: &mut W, status: &str) { let _ = respond(w, status, "text/plain; charset=utf-8", &[], 0, b""); }
fn html<W: Write>(w: &mut W, status: &str, body: Vec<u8>) { let _ = respond(w, status, "text/html; charset=utf-8", &[], body.len() as u64, &body); }

fn sid_of(cookie: &str) -> Option<&str> { cookie.split(';').filter_map(|c| c.trim().strip_prefix("sid=")).next() }
/// A name the phone chose for itself: no control characters, at most 32 characters.
fn clean_device_name(raw: &str) -> Option<String> {
    let n: String = raw.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(32).collect();
    let n = n.trim().to_string();
    (!n.is_empty()).then_some(n)
}
/// Which device an unlocked cookie belongs to.
fn device_of(sh: &Shared, cookie: &str) -> String {
    let Some(sid) = cookie.split(';').filter_map(|c| c.trim().strip_prefix("sid=")).next() else { return "Phone".into() };
    sh.state.lock().ok().and_then(|s| s.sessions.iter().find(|(k, _)| same(k, sid)).map(|d| d.1.clone())).unwrap_or_else(|| "Phone".into())
}
fn authed(sh: &Shared, cookie: &str) -> bool {
    let Some(sid) = cookie.split(';').filter_map(|c| c.trim().strip_prefix("sid=")).next() else { return false };
    sh.state.lock().map_or(false, |s| s.sessions.iter().fold(false, |ok, (known, _)| same(known, sid) | ok))
}
fn handle<S: Read + Write>(sh: &Shared, r: &mut BufReader<S>) {
    let Some(head) = read_head(r) else { return };
    let token = sh.token();
    let prefix = format!("/s/{token}");
    // Anything but the exact token path looks the same as a wrong address.
    let rest = match head.path.strip_prefix("/s/").and_then(|p| p.split_once('/').or(Some((p, "")))) { Some((t, rest)) if same(t, &token) => rest.to_string(),
        Some((t, _)) if sh.state.lock().map_or(false, |s| !s.prev.is_empty() && same(t, &s.prev)) => return html(r.get_mut(), "410 Gone", done_page()),
        _ => return simple(r.get_mut(), "404 Not Found") };
    if !sh.alive() { return html(r.get_mut(), "410 Gone", stopped_page("Start it again in Sora.")); }
    let ok = authed(sh, &head.cookie);
    match (head.method.as_str(), rest.as_str()) {
        ("GET", "") if ok => { let up = sh.state.lock().map_or(false, |s| s.upload); html(r.get_mut(), "200 OK", list_page(sh, &token, up)) }
        ("GET", "") => html(r.get_mut(), "200 OK", unlock_page(&token, &sh.host, head.query == "e=1")),
        ("GET", "s") if ok => { let up = sh.state.lock().map_or(false, |s| s.upload); let b = format!("{{\"upload\":{up}}}"); let _ = respond(r.get_mut(), "200 OK", "application/json", &[], b.len() as u64, b.as_bytes()); }
        ("GET", "a.js") => { let b = script(); let _ = respond(r.get_mut(), "200 OK", "text/javascript; charset=utf-8", &[], b.len() as u64, &b); }
        ("POST", "unlock") => {
            let n = head.length.unwrap_or(u64::MAX);
            if n > 64 { return simple(r.get_mut(), "413 Payload Too Large"); }
            let mut body = vec![0u8; n as usize];
            if r.read_exact(&mut body).is_err() { return; }
            let given = String::from_utf8_lossy(&body).strip_prefix("code=").unwrap_or("").to_string();
            let Ok(mut st) = sh.state.lock() else { return };
            if same(&given, &st.code) {
                let sid = b64(&random(24));
                let device = unique_name(device_name(&head.agent), &st.sessions);
                st.sessions.push((sid.clone(), device)); if st.sessions.len() > MAX_SESSIONS { st.sessions.remove(0); }
                st.started = Instant::now();
                drop(st); sh.say("connected", serde_json::json!({ "devices": sh.devices() }));
                sh.rotate_code();
                let _ = respond(r.get_mut(), "303 See Other", "text/plain", &[("Location", prefix.clone()), ("Set-Cookie", format!("sid={sid}; Path={prefix}; Secure; HttpOnly; SameSite=Strict; Max-Age=900"))], 0, b"");
            } else {
                st.wrong += 1;
                let locked = st.wrong >= MAX_WRONG;
                if locked { st.dead = true; }
                drop(st);
                if locked { sh.say("locked", serde_json::json!({})); html(r.get_mut(), "410 Gone", stopped_page("Too many wrong codes.")); }
                else { let _ = respond(r.get_mut(), "303 See Other", "text/plain", &[("Location", format!("{prefix}?e=1"))], 0, b""); }
            }
        }
        ("GET", p) if p.starts_with("f/") => {
            if !ok { return simple(r.get_mut(), "403 Forbidden"); }
            let Some((idx, entry)) = p[2..].parse::<usize>().ok().and_then(|i| sh.files.get(i).map(|e| (i, e))) else { return simple(r.get_mut(), "404 Not Found") };
            let Ok(mut file) = File::open(&entry.path) else { return simple(r.get_mut(), "404 Not Found") };
            let Ok(len) = file.metadata().map(|m| if m.is_file() { m.len() } else { 0 }) else { return };
            let ascii: String = entry.name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' }).collect();
            let w = r.get_mut();
            if respond(w, "200 OK", "application/octet-stream", &[("Content-Disposition", format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{}", percent_encode(&entry.name)))], len, b"").is_err() { return; }
            let (mut buf, mut sent) = (vec![0u8; 64 * 1024], 0u64);
            while sent < len && sh.alive() {
                let Ok(n) = file.read(&mut buf) else { break };
                if n == 0 || w.write_all(&buf[..n.min((len - sent) as usize)]).is_err() { break; }
                sent += n as u64;
            }
            let _ = w.flush();
            if sent >= len {
                sh.say("downloaded", serde_json::json!({ "name": entry.name }));
                let all = sh.state.lock().map_or(false, |mut s| { if let Some(g) = s.got.get_mut(idx) { *g = true; } !s.got.is_empty() && s.got.iter().all(|g| *g) });
                if all { sh.rotate(); }
            }
        }
        ("POST", "name") => {
            if !ok || !head.marked { return simple(r.get_mut(), "403 Forbidden"); }
            let n = head.length.unwrap_or(u64::MAX);
            if n > 512 { return simple(r.get_mut(), "413 Payload Too Large"); }
            let mut body = vec![0u8; n as usize];
            if r.read_exact(&mut body).is_err() { return; }
            let name = serde_json::from_slice::<serde_json::Value>(&body).ok().and_then(|v| clean_device_name(v.get("name")?.as_str()?));
            let (Some(name), Some(sid)) = (name, sid_of(&head.cookie)) else { return simple(r.get_mut(), "400 Bad Request") };
            if let Ok(mut st) = sh.state.lock() {
                if let Some(i) = st.sessions.iter().position(|(k, _)| same(k, sid)) {
                    let others: Vec<(String, String)> = st.sessions.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| d.clone()).collect();
                    st.sessions[i].1 = unique_name(name, &others);
                }
            }
            sh.say("connected", serde_json::json!({ "devices": sh.devices() }));
            simple(r.get_mut(), "204 No Content");
        }
        ("POST", "offer") => {
            let open = ok && head.marked && sh.state.lock().map_or(false, |s| s.upload && s.offer.as_ref().map_or(true, |o| o.verdict != Verdict::Pending));
            if !open { return simple(r.get_mut(), "403 Forbidden"); }
            let n = head.length.unwrap_or(u64::MAX);
            if n > 16384 { return simple(r.get_mut(), "413 Payload Too Large"); }
            let mut body = vec![0u8; n as usize];
            if r.read_exact(&mut body).is_err() { return; }
            let list: Option<Vec<(String, u64)>> = serde_json::from_slice::<serde_json::Value>(&body).ok().and_then(|v| v.get("files")?.as_array().map(|a| a.iter().filter_map(|f| Some((safe_name(f.get("name")?.as_str()?), f.get("size")?.as_u64()?))).collect()));
            let Some(files) = list.filter(|l| (1..=MAX_UPLOADS as usize).contains(&l.len()) && l.iter().all(|f| (1..=MAX_UPLOAD).contains(&f.1))) else { return simple(r.get_mut(), "400 Bad Request") };
            let id = b64(&random(9));
            let device = device_of(sh, &head.cookie);
            let view = OfferView { count: files.len(), total: files.iter().map(|f| f.1).sum(), names: files.iter().take(3).map(|f| f.0.clone()).collect(), device: device.clone() };
            if let Ok(mut st) = sh.state.lock() { st.offer = Some(Offer { id: id.clone(), claimed: vec![false; files.len()], files, done: 0, bytes_done: 0, device, verdict: Verdict::Pending, at: Instant::now() }); }
            sh.say("offer", serde_json::json!({ "offer": view }));
            let b = format!("{{\"id\":\"{id}\"}}");
            let _ = respond(r.get_mut(), "200 OK", "application/json", &[], b.len() as u64, b.as_bytes());
        }
        ("GET", p) if p.starts_with("offer/") => {
            if !ok { return simple(r.get_mut(), "403 Forbidden"); }
            let asked = p[6..].to_string();
            if sh.state.lock().map_or(false, |s| s.offer.as_ref().map_or(false, |o| same(&o.id, &asked) && o.verdict == Verdict::Pending && o.at.elapsed() > Duration::from_secs(60))) { sh.decide(false); }
            let verdict = sh.state.lock().ok().and_then(|s| s.offer.as_ref().filter(|o| same(&o.id, &asked)).map(|o| o.verdict));
            let word = match verdict { Some(Verdict::Pending) => "pending", Some(Verdict::Accepted) => "accepted", _ => "declined" };
            let b = format!("{{\"status\":\"{word}\"}}");
            let _ = respond(r.get_mut(), "200 OK", "application/json", &[], b.len() as u64, b.as_bytes());
        }
        ("PUT", "u") => {
            let query = |key: &str| head.query.split('&').find_map(|kv| kv.strip_prefix(key)).unwrap_or("").to_string();
            let (offer_id, name) = (query("o="), safe_name(&percent_decode(&query("name="))));
            let Some(len) = head.length.filter(|n| (1..=MAX_UPLOAD).contains(n)) else { return simple(r.get_mut(), "411 Length Required") };
            // Only a file the PC accepted, once, with the size that was announced.
            let slot = if ok && head.marked { sh.state.lock().ok().and_then(|mut s| {
                if !s.upload { return None; }
                let o = s.offer.as_mut().filter(|o| o.verdict == Verdict::Accepted && same(&o.id, &offer_id))?;
                let i = (0..o.files.len()).find(|&i| !o.claimed[i] && o.files[i].0 == name && o.files[i].1 == len)?;
                o.claimed[i] = true; Some(i)
            }) } else { None };
            let Some(slot) = slot else { return simple(r.get_mut(), "403 Forbidden") };
            let release = || { if let Ok(mut s) = sh.state.lock() { if let Some(o) = s.offer.as_mut() { o.claimed[slot] = false; } } };
            let Ok((mut file, path)) = create_unique(&sh.inbox, &name) else { release(); return simple(r.get_mut(), "500 Internal Server Error") };
            let (before, total) = sh.state.lock().ok().and_then(|s| s.offer.as_ref().map(|o| (o.bytes_done, o.files.iter().map(|f| f.1).sum::<u64>()))).unwrap_or((0, len));
            let (mut src, mut buf, mut copied, mut last) = (Read::take(&mut *r, len), vec![0u8; 64 * 1024], 0u64, Instant::now());
            loop {
                let n = match src.read(&mut buf) { Ok(0) | Err(_) => break, Ok(n) => n };
                if file.write_all(&buf[..n]).is_err() { break; }
                copied += n as u64;
                if last.elapsed() >= Duration::from_millis(150) { last = Instant::now(); sh.say("progress", serde_json::json!({ "done": before + copied, "total": total, "name": name })); }
            }
            drop(file);
            if copied != len || !sh.alive() { let _ = std::fs::remove_file(&path); release(); return simple(r.get_mut(), "400 Bad Request"); }
            let finished = sh.state.lock().map_or(false, |mut s| s.offer.as_mut().map_or(false, |o| { o.done += 1; o.bytes_done += len; o.done == o.files.len() }));
            sh.say("progress", serde_json::json!({ "done": before + len, "total": total, "name": name }));
            sh.say("uploaded", serde_json::json!({ "name": path.file_name().map_or(name.clone(), |n| n.to_string_lossy().into_owned()) }));
            let _ = respond(r.get_mut(), "201 Created", "text/plain", &[], 0, b"");
            if finished { sh.rotate(); }
        }
        _ => simple(r.get_mut(), "404 Not Found"),
    }
}
fn serve(listener: TcpListener, config: Arc<ServerConfig>, sh: Arc<Shared>) {
    let _ = listener.set_nonblocking(true);
    let active = Arc::new(AtomicUsize::new(0));
    while sh.alive() {
        if sh.state.lock().map_or(false, |s| s.code_at.elapsed() >= sh.code_every) { sh.rotate_code(); }
        match listener.accept() {
            Ok((tcp, _)) => {
                if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS { continue; }
                active.fetch_add(1, Ordering::SeqCst);
                let (sh, config, active) = (sh.clone(), config.clone(), active.clone());
                std::thread::spawn(move || {
                    let _ = tcp.set_nonblocking(false);
                    let _ = tcp.set_read_timeout(Some(Duration::from_secs(20)));
                    let _ = tcp.set_write_timeout(Some(Duration::from_secs(20)));
                    if let Ok(conn) = ServerConnection::new(config) {
                        let mut r = BufReader::new(StreamOwned::new(conn, tcp));
                        handle(&sh, &mut r);
                        let s = r.get_mut(); s.conn.send_close_notify(); let _ = s.flush();
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    // Ended on its own (time out, too many wrong codes): tell the panel. Stopped from outside (Disconnect, a new link): stay quiet.
    if !sh.stop.swap(true, Ordering::SeqCst) { (sh.notify)("ended", serde_json::json!({})); }
}
fn config_from(chain: Vec<CertificateDer<'static>>, key: PrivateKeyDer<'static>) -> AppResult<Arc<ServerConfig>> {
    let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().map_err(err)?
        .with_no_client_auth().with_single_cert(chain, key).map_err(err)?;
    Ok(Arc::new(config))
}
/// The fallback: a certificate made for this session and this address only (the browser warns about it).
fn tls_config(ip: IpAddr) -> AppResult<Arc<ServerConfig>> {
    let ident = rcgen::generate_simple_self_signed(vec![ip.to_string()]).map_err(err)?;
    config_from(vec![CertificateDer::from(ident.cert.der().to_vec())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ident.key_pair.serialize_der())))
}
/// The PC's address on the local network (private IPv4, Wi-Fi or Ethernet).
fn lan_ip() -> Option<IpAddr> {
    if_addrs::get_if_addrs().ok()?.into_iter().filter(|i| !i.is_loopback()).map(|i| i.ip()).find(|ip| matches!(ip, IpAddr::V4(v) if v.is_private()))
}

// ---------------------------------------------------------------- commands
#[derive(Serialize)]
pub struct ShareInfo { url: String, code: String, width: usize, modules: String, seconds: u64, trusted: bool }
fn stop_current() { if let Ok(mut c) = CURRENT.lock() { if let Some(s) = c.take() { s.stop.store(true, Ordering::SeqCst); } } }
/// Starts (or restarts) the page for the files picked in Send. A phone may offer files back; they land in Downloads\Sora Received once the PC accepts.
#[tauri::command]
pub async fn web_share_start(app: tauri::AppHandle, paths: Vec<String>) -> AppResult<ShareInfo> {
    let queued = app.state::<SendQueue>().0.lock().map_err(err)?.clone();
    let files: Vec<Entry> = paths.iter().filter_map(|p| queued.iter().find(|f| &f.path == p)).filter_map(|f| {
        let path = PathBuf::from(&f.path);
        path.is_file().then(|| Entry { name: safe_name(&f.name), size: std::fs::metadata(&path).map_or(f.size, |m| m.len()), path })
    }).collect();
    // No files is fine: the page then only receives (phone to PC).
    let ip = lan_ip().ok_or("Connect to Wi-Fi or Ethernet first.")?;
    let handle = app.clone();
    let notify: Box<dyn Fn(&str, serde_json::Value) + Send + Sync> = Box::new(move |kind, data| { if kind == "offer" { crate::notch_ui::show_send(); } let _ = handle.emit("web-share", serde_json::json!({ "kind": kind, "data": data })); });
    let inbox = app.path().download_dir().map_err(err)?.join("Sora Received");
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "your PC".into());
    // With the share service: the hosted page and WebRTC, no warning on any network (no DNS, no certificate for this PC).
    if let Some(page) = crate::share_cert::page() {
        let (setup, link) = rtc::setup(ip, &page)?;
        stop_current();
        let count = files.len();
        let sh = Arc::new(Shared { code_every: CODE_EVERY, origin: String::new(), link: Some(link), host, files, state: Mutex::new(State::new(true, count)), stop: AtomicBool::new(false), inbox, notify });
        let info = sh.info().ok_or("Could not make the QR code.")?;
        *CURRENT.lock().map_err(err)? = Some(sh.clone());
        std::thread::spawn(move || rtc::serve(setup, sh));
        return Ok(info);
    }
    // A trusted name (no browser warning) when the service is set up and answers; otherwise the address and a self-signed certificate.
    let trusted = crate::share_cert::trusted(&app, ip).await;
    let host_name = trusted.as_ref().map_or_else(|| ip.to_string(), |t| t.host.clone());
    stop_current();
    let listener = TcpListener::bind((ip, 0)).map_err(err)?;
    let port = listener.local_addr().map_err(err)?.port();
    let count = files.len();
    let sh = Arc::new(Shared { code_every: CODE_EVERY, origin: format!("https://{host_name}:{port}"), link: None, host, files, state: Mutex::new(State::new(true, count)), stop: AtomicBool::new(false), inbox, notify });
    let info = sh.info().ok_or("Could not make the QR code.")?;
    *CURRENT.lock().map_err(err)? = Some(sh.clone());
    let config = match trusted { Some(t) => config_from(t.chain, t.key)?, None => tls_config(ip)? };
    std::thread::spawn(move || serve(listener, config, sh));
    Ok(info)
}
/// The phone may (or may no longer) send files to this PC.
/// "Not you?": a new QR code and code, and everybody who was connected is out.
#[tauri::command]
pub fn web_share_reset() { if let Ok(c) = CURRENT.lock() { if let Some(s) = c.as_ref() { s.rotate(); } } }
#[tauri::command]
pub fn web_share_stop() { stop_current() }
/// Warm-up when QR Code is picked in Send: the trusted name is looked up now, so the arrow shows the QR at once.
#[tauri::command]
pub async fn web_share_prepare(app: tauri::AppHandle) { if crate::share_cert::page().is_some() { return; } if let Some(ip) = lan_ip() { let _ = crate::share_cert::trusted(&app, ip).await; } }
#[derive(Serialize)]
pub struct OfferView { count: usize, total: u64, names: Vec<String>, device: String }
#[derive(Serialize)]
pub struct ShareState { info: ShareInfo, devices: Vec<String>, offer: Option<OfferView> }
/// What is being shared right now (the panel page asks when it opens: it may have been closed while something happened).
#[tauri::command]
pub fn web_share_state() -> Option<ShareState> {
    let sh = CURRENT.lock().ok()?.clone()?;
    if !sh.alive() { return None; }
    let (info, offer, devices) = (sh.info()?, sh.offer_view(), sh.devices());
    Some(ShareState { info, devices, offer })
}
/// Accept or decline what the phone offered to send.
#[tauri::command]
pub fn web_share_decide(accept: bool) { if let Ok(c) = CURRENT.lock() { if let Some(s) = c.as_ref() { s.decide(accept); } } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use rustls::{client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier}, pki_types::{ServerName, UnixTime}, ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
    #[derive(Debug)] struct Trust;
    impl ServerCertVerifier for Trust {
        fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> { Ok(ServerCertVerified::assertion()) }
        fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
        fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes() }
    }
    /// One request over TLS to the server; returns the status line, the headers and the body.
    fn ask(port: u16, request: &str, body: &[u8]) -> (String, String, Vec<u8>) {
        let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().unwrap()
            .dangerous().with_custom_certificate_verifier(Arc::new(Trust)).with_no_client_auth();
        let conn = ClientConnection::new(Arc::new(config), "127.0.0.1".try_into().unwrap()).unwrap();
        let mut s = StreamOwned::new(conn, TcpStream::connect(("127.0.0.1", port)).unwrap());
        s.write_all(request.as_bytes()).unwrap(); s.write_all(body).unwrap();
        let mut raw = Vec::new();
        let _ = s.read_to_end(&mut raw);
        let at = raw.windows(4).position(|w| w == b"\r\n\r\n").map_or(raw.len(), |i| i + 4);
        let head = String::from_utf8_lossy(&raw[..at]).into_owned();
        (head.lines().next().unwrap_or("").to_string(), head, raw[at..].to_vec())
    }
    fn start(upload: bool, dir: &Path) -> (u16, Arc<Shared>) {
        let file = dir.join("secret-plan.txt");
        std::fs::write(&file, b"the quick brown fox").unwrap();
        let sh = Arc::new(Shared { code_every: Duration::from_secs(3600), origin: "https://127.0.0.1".into(), link: None, host: "TEST-PC".into(), files: vec![Entry { path: file, name: "secret-plan.txt".into(), size: 19 }],
            state: Mutex::new({ let mut s = State::new(upload, 1); s.code = "123456".into(); s }), stop: AtomicBool::new(false), inbox: dir.join("in"), notify: Box::new(|_, _| {}) });
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (config, shared) = (tls_config("127.0.0.1".parse().unwrap()).unwrap(), sh.clone());
        std::thread::spawn(move || serve(listener, config, shared));
        (port, sh)
    }
    /// The link is replaced just after the response goes out: wait (up to 2 s) until it has been.
    fn settle(sh: &Shared, old: &str) { for _ in 0..40 { if sh.token() != old { return; } std::thread::sleep(Duration::from_millis(50)); } }
    fn cookie_of(head: &str) -> String { head.lines().find_map(|l| l.strip_prefix("Set-Cookie: ")).unwrap().split(';').next().unwrap().to_string() }
    #[test]
    fn a_replaced_session_stays_quiet_and_says_if_it_is_trusted() {
        let heard = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = heard.clone();
        let sh = Shared { code_every: Duration::from_secs(3600), origin: "https://192.168.1.5:5000".into(), link: None, host: "T".into(), files: vec![], state: Mutex::new(State::new(true, 0)), stop: AtomicBool::new(false), inbox: PathBuf::new(), notify: Box::new(move |k, _| log.lock().unwrap().push(k.into())) };
        assert!(!sh.info().unwrap().trusted, "a bare address is not trusted");
        sh.rotate_code();
        sh.stop.store(true, Ordering::SeqCst);
        sh.rotate();
        assert_eq!(*heard.lock().unwrap(), ["code"], "nothing after it was stopped: its late events would close the new link in the panel");
        let named = Shared { origin: "https://abc.share.soranotch.com:5000".into(), notify: Box::new(|_, _| {}), stop: AtomicBool::new(false), state: Mutex::new(State::new(true, 0)), files: vec![], ..sh };
        assert!(named.info().unwrap().trusted);
    }
    #[test]
    fn nothing_is_visible_before_the_code_and_five_wrong_codes_end_it() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(false, dir.path());
        let tok = sh.token();
        let t = &tok;
        let (status, _, _) = ask(port, "GET /s/wrong-token HTTP/1.1\r\nHost: x\r\n\r\n", b"");
        assert!(status.contains("404"), "{status}");
        let (status, head, body) = ask(port, &format!("GET /s/{t} HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        let text = String::from_utf8_lossy(&body);
        assert!(status.contains("200") && text.contains("Enter the code") && !text.contains("secret-plan"), "no file name before the code");
        assert!(head.contains("no-store") && head.contains("default-src 'none'") && head.contains("nosniff"));
        let (status, ..) = ask(port, &format!("GET /s/{t}/f/0 HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        assert!(status.contains("403"), "no download without the code: {status}");
        for _ in 0..4 {
            let (status, ..) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), b"code=000000");
            assert!(status.contains("303"), "{status}");
        }
        let (status, ..) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), b"code=000000");
        assert!(status.contains("410"), "the fifth wrong code ends the session: {status}");
        assert!(sh.state.lock().unwrap().dead);
        std::thread::sleep(Duration::from_millis(400));
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "the server is gone, the right code no longer helps");
    }
    #[test]
    fn a_share_without_files_is_a_receive_page() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let tok = sh.token();
        let (_, head, _) = ask(port, &format!("POST /s/{tok}/unlock HTTP/1.1
Host: x
Content-Length: 11

"), b"code=123456");
        let cookie = cookie_of(&head);
        let page = String::from_utf8_lossy(&ask(port, &format!("GET /s/{tok} HTTP/1.1
Host: x
Cookie: {cookie}

"), b"").2).into_owned();
        assert!(page.contains("id=\"upload\"") && page.contains("secret-plan"), "files and the upload row");
        let empty = Shared { code_every: Duration::from_secs(3600), origin: "https://127.0.0.1".into(), link: None, host: "TEST-PC".into(), files: vec![], state: Mutex::new(State::new(true, 0)), stop: AtomicBool::new(false), inbox: dir.path().join("in"), notify: Box::new(|_, _| {}) };
        let html = String::from_utf8_lossy(&list_page(&empty, "t", true)).into_owned();
        assert!(html.contains("Send to this PC") && html.contains("id=\"upload\"") && !html.contains("class=\"file\" href"), "no download rows, upload row only");
    }
    #[test]
    fn the_right_code_opens_the_files_and_uploads_need_permission() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(false, dir.path());
        let t = sh.token();
        let (status, head, _) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), b"code=123456");
        assert!(status.contains("303") && head.contains("HttpOnly") && head.contains("Secure") && head.contains("SameSite=Strict"));
        let cookie = cookie_of(&head);
        let (_, _, list) = ask(port, &format!("GET /s/{t} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(String::from_utf8_lossy(&list).contains("secret-plan.txt"));
        let (status, ..) = ask(port, &format!("GET /s/{t}/f/9 HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("404"));
        let body = r#"{"files":[{"name":"a.txt","size":2}]}"#;
        let offer = format!("POST /s/{t}/offer HTTP/1.1
Host: x
Cookie: {cookie}
X-Sora: 1
Content-Length: {}

", body.len());
        assert!(ask(port, &offer, body.as_bytes()).0.contains("403") && !dir.path().join("in").exists(), "no offers while the PC does not allow uploads");
        let state = format!("GET /s/{t}/s HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n");
        let (_, _, off) = ask(port, &state, b"");
        assert_eq!(off, b"{\"upload\":false}", "the page learns that the PC has not allowed uploads yet");
        assert!(String::from_utf8_lossy(&ask(port, &format!("GET /s/{t} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"").2).contains("id=\"upload\""), "the upload row is always on the page");
        assert!(ask(port, &format!("GET /s/{t}/s HTTP/1.1\r\nHost: x\r\n\r\n"), b"").0.contains("404"), "the state is only for the unlocked phone");
        sh.state.lock().unwrap().upload = true;
        assert_eq!(ask(port, &state, b"").2, b"{\"upload\":true}");
        // The download comes last: it uses the link up.
        let (status, head, file) = ask(port, &format!("GET /s/{t}/f/0 HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("200") && head.contains("attachment") && file == b"the quick brown fox");
        settle(&sh, &t);
        assert_ne!(sh.token(), t, "the link is replaced once the only file is downloaded");
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn a_phone_upload_needs_the_pc_to_accept_and_then_burns_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let t = sh.token();
        let (_, head, _) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), b"code=123456");
        let cookie = cookie_of(&head);
        let body = r#"{"files":[{"name":"a.txt","size":2},{"name":"b.txt","size":3}]}"#;
        let offer = format!("POST /s/{t}/offer HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\nX-Sora: 1\r\nContent-Length: {}\r\n\r\n", body.len());
        let (status, _, reply) = ask(port, &offer, body.as_bytes());
        assert!(status.contains("200"), "{status}");
        let id = String::from_utf8_lossy(&reply).split('"').nth(3).unwrap().to_string();
        let put = |name: &str, data: &[u8], o: &str| ask(port, &format!("PUT /s/{t}/u?name={name}&o={o} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\nX-Sora: 1\r\nContent-Length: {}\r\n\r\n", data.len()), data).0;
        let status_of = || String::from_utf8_lossy(&ask(port, &format!("GET /s/{t}/offer/{id} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"").2).into_owned();
        assert!(status_of().contains("pending"));
        // (A refused upload may be cut off before its answer is read: the server stops listening to a body it will not take.)
        assert!((0..4).any(|_| put("a.txt", b"hi", &id).contains("403")) && !dir.path().join("in").exists(), "nothing is written before the PC accepts");
        assert!(ask(port, &offer, body.as_bytes()).0.contains("403"), "one question at a time");
        sh.decide(true);
        assert!(status_of().contains("accepted"));
        assert!(put("a.txt", b"hi", "wrong").contains("403"), "only with the offer's own id");
        assert!(put("c.txt", b"hi", &id).contains("403"), "only the files that were announced");
        assert!(put("a.txt", b"hello", &id).contains("403"), "only with the announced size");
        assert!(put("a.txt", b"hi", &id).contains("201") && std::fs::read(dir.path().join("in").join("a.txt")).unwrap() == b"hi");
        assert!(put("a.txt", b"hi", &id).contains("403"), "each announced file once");
        assert_eq!(sh.token(), t, "the link lives until the last announced file is in");
        assert!(put("b.txt", b"abc", &id).contains("201"));
        settle(&sh, &t);
        assert_ne!(sh.token(), t, "the link and the code are replaced after the upload");
        let (status, ..) = ask(port, &format!("GET /s/{t} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("410"));
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn declining_burns_the_link_too() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let t = sh.token();
        let (_, head, _) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), b"code=123456");
        let cookie = cookie_of(&head);
        let body = r#"{"files":[{"name":"a.txt","size":2}]}"#;
        ask(port, &format!("POST /s/{t}/offer HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\nX-Sora: 1\r\nContent-Length: {}\r\n\r\n", body.len()), body.as_bytes());
        assert!(sh.offer_view().is_some_and(|o| o.count == 1 && o.total == 2));
        sh.decide(false);
        assert_ne!(sh.token(), t);
        assert!(!dir.path().join("in").exists());
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn the_code_changes_after_each_person_gets_in_and_stays_changed_for_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let t = sh.token();
        let unlock = |code: &str, agent: &str| ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nUser-Agent: {agent}\r\nContent-Length: 11\r\n\r\n"), format!("code={code}").as_bytes());
        let (status, head, _) = unlock("123456", "Mozilla/5.0 (iPhone)");
        assert!(status.contains("303") && head.contains("Set-Cookie"));
        let fresh = sh.state.lock().unwrap().code.clone();
        assert_ne!(fresh, "123456", "the code the first person used is gone");
        let (_, refused, _) = unlock("123456", "Mozilla/5.0 (Linux; Android 14; Pixel 8)");
        assert!(refused.contains("?e=1") && !refused.contains("Set-Cookie"), "the old code no longer opens it");
        assert!(sh.devices() == vec!["iPhone".to_string()], "only the first person is in so far");
        let (status, ..) = unlock(&fresh, "Mozilla/5.0 (Linux; Android 14; Pixel 8)");
        assert!(status.contains("303"));
        assert_eq!(sh.devices(), vec!["iPhone".to_string(), "Pixel 8".to_string()], "both stay connected");
        assert_ne!(sh.state.lock().unwrap().code, fresh, "and the code moved on again");
        assert_eq!(sh.token(), t, "the link itself stays");
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn the_code_also_changes_by_itself() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let _ = port;
        let first = sh.state.lock().unwrap().code.clone();
        let sh2 = Arc::new(Shared { code_every: Duration::from_millis(250), origin: "https://127.0.0.1".into(), link: None, host: "T".into(), files: vec![], state: Mutex::new(State::new(true, 0)), stop: AtomicBool::new(false), inbox: dir.path().join("in"), notify: Box::new(|_, _| {}) });
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let (config, shared) = (tls_config("127.0.0.1".parse().unwrap()).unwrap(), sh2.clone());
        std::thread::spawn(move || serve(listener, config, shared));
        let c0 = sh2.state.lock().unwrap().code.clone();
        let t0 = sh2.token();
        std::thread::sleep(Duration::from_millis(1200));
        assert_ne!(sh2.state.lock().unwrap().code, c0, "a new code without anyone coming in");
        assert_eq!(sh2.token(), t0, "the link does not change");
        assert!(sh2.info().unwrap().seconds <= 1, "the countdown the PC shows is the time to the next code");
        assert_eq!(sh.state.lock().unwrap().code, first, "a share with a long interval keeps its code");
        sh.stop.store(true, Ordering::SeqCst); sh2.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn a_phone_can_tell_the_pc_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(true, dir.path());
        let t = sh.token();
        let (_, head, _) = ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nUser-Agent: Mozilla/5.0 (iPhone; CPU iPhone OS 17_4)\r\nContent-Length: 11\r\n\r\n"), b"code=123456");
        let cookie = cookie_of(&head);
        assert_eq!(sh.devices(), vec!["iPhone".to_string()], "first the browser's own guess");
        let body = br#"{"name":"iPhone 15 Pro"}"#;
        let post = |cookie: &str, marked: bool| ask(port, &format!("POST /s/{t}/name HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n{}Content-Length: {}\r\n\r\n", if marked { "X-Sora: 1\r\n" } else { "" }, body.len()), body).0;
        assert!(post(&cookie, false).contains("403") && post("sid=nope", true).contains("403"), "only an unlocked phone, with the header");
        assert!(post(&cookie, true).contains("204"));
        assert_eq!(sh.devices(), vec!["iPhone 15 Pro".to_string()]);
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn the_link_and_the_code_work_once() {
        let dir = tempfile::tempdir().unwrap();
        let (port, sh) = start(false, dir.path());
        let (old, old_code) = (sh.token(), sh.state.lock().unwrap().code.clone());
        let unlock = |t: &str, code: &str| ask(port, &format!("POST /s/{t}/unlock HTTP/1.1\r\nHost: x\r\nContent-Length: 11\r\n\r\n"), format!("code={code}").as_bytes());
        let (_, head, _) = unlock(&old, "123456");
        let cookie = cookie_of(&head);
        let (status, ..) = ask(port, &format!("GET /s/{old}/f/0 HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("200"));
        settle(&sh, &old);
        let (fresh, fresh_code) = (sh.token(), sh.state.lock().unwrap().code.clone());
        assert_ne!(fresh, old, "a new link after the only file was downloaded");
        assert_ne!(fresh_code, old_code);
        let (status, _, page) = ask(port, &format!("GET /s/{old} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("410") && String::from_utf8_lossy(&page).contains("link was used"), "the old link says it was used: {status}");
        let (status, ..) = ask(port, &format!("GET /s/{old}/f/0 HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("410"), "the old cookie opens nothing: {status}");
        let (status, ..) = ask(port, &format!("GET /s/{fresh}/f/0 HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n\r\n"), b"");
        assert!(status.contains("403"), "the old cookie does not open the new link: {status}");
        sh.stop.store(true, Ordering::SeqCst);
    }
    /// A live server for driving the pages with a real browser: SORA_DUMP=<folder> cargo test serve_for_browser -- --ignored
    #[test]
    #[ignore]
    fn serve_for_browser() {
        let Ok(dir) = std::env::var("SORA_DUMP") else { return };
        let (port, sh) = start(true, Path::new(&dir));
        std::fs::write(Path::new(&dir).join("live.txt"), format!("{port} {} 123456", sh.token())).unwrap();
        // Plays the PC: accepts whatever the phone offers after a moment.
        for _ in 0..300 { if sh.offer_view().is_some() { std::thread::sleep(Duration::from_millis(1800)); sh.decide(true); } std::thread::sleep(Duration::from_millis(200)); }
    }
    /// Writes the phone pages to a folder for a look in a browser: SORA_DUMP=<folder> cargo test dump_pages -- --ignored
    #[test]
    #[ignore]
    fn dump_pages() {
        let Ok(dir) = std::env::var("SORA_DUMP") else { return };
        let dir = Path::new(&dir);
        let tmp = tempfile::tempdir().unwrap();
        let (_, sh) = start(true, tmp.path());
        let t = sh.token();
        std::fs::write(dir.join("unlock.html"), unlock_page(&t, "DESKTOP-ALEX", false)).unwrap();
        std::fs::write(dir.join("unlock-wrong.html"), unlock_page(&t, "DESKTOP-ALEX", true)).unwrap();
        std::fs::write(dir.join("list.html"), list_page(&sh, &t, true)).unwrap();
        std::fs::write(dir.join("done.html"), done_page()).unwrap();
        sh.stop.store(true, Ordering::SeqCst);
    }
    #[test]
    fn the_qr_holds_the_whole_address() {
        let url = format!("https://192.168.100.200:50123/s/{}", b64(&random(24)));
        let qr = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::M).unwrap();
        assert!(qr.width() >= 29 && qr.width() <= 45, "a version 3-6 code, easy to scan: {}", qr.width());
        assert_eq!(qr.to_colors().len(), qr.width() * qr.width());
        assert!(lan_ip().map_or(true, |ip| matches!(ip, IpAddr::V4(v) if v.is_private())), "only a private LAN address is ever used");
    }
    #[test]
    fn devices_are_named_from_their_browser_and_told_apart() {
        assert_eq!(device_name("Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 Version/17.4 Mobile/15E148 Safari/604.1"), "iPhone");
        assert_eq!(device_name("Mozilla/5.0 (Linux; Android 14; Pixel 8 Build/UP1A.231105.001) AppleWebKit/537.36 Chrome/124.0 Mobile Safari/537.36"), "Pixel 8");
        assert_eq!(device_name("Mozilla/5.0 (Linux; Android 14; SM-S918B) AppleWebKit/537.36 Chrome/124.0 Mobile Safari/537.36"), "SM-S918B");
        assert_eq!(device_name("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 Chrome/124.0 Mobile Safari/537.36"), "Android phone", "Chrome's reduced user agent has no model");
        assert_eq!(device_name("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/17.4 Safari/605.1.15"), "Mac");
        assert_eq!(device_name(""), "Phone");
        let taken = vec![("a".to_string(), "iPhone".to_string()), ("b".to_string(), "iPhone 2".to_string())];
        assert_eq!(unique_name("iPhone".into(), &taken), "iPhone 3");
        assert_eq!(unique_name("Pixel 8".into(), &taken), "Pixel 8");
    }
    #[test]
    fn names_are_made_safe_and_phones_can_name_themselves() {
        assert_eq!(clean_device_name("  iPhone 15 Pro\n "), Some("iPhone 15 Pro".to_string()));
        assert_eq!(clean_device_name("\u{7}\u{0}"), None);
        assert_eq!(clean_device_name(&"x".repeat(80)).unwrap().chars().count(), 32);
        assert_eq!(sid_of("a=1; sid=abc; b=2"), Some("abc"));
        assert_eq!(safe_name("..\\..\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(safe_name("a/b:c*d?.txt"), "bcd.txt");
        assert_eq!(safe_name("CON.txt"), "_CON.txt");
        assert_eq!(safe_name("  ..  "), "file");
        assert_eq!(safe_name(&"x".repeat(300)).len(), 120);
        assert_eq!(percent_decode("a%20b%2Fc+d%zz"), "a b/c d%zz");
        assert_eq!(percent_decode(&percent_encode("héllo wörld.png")), "héllo wörld.png");
    }
    #[test]
    fn codes_and_tokens_are_random_and_the_right_size() {
        let c = new_code();
        assert!(c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()));
        assert_ne!(b64(&random(24)), b64(&random(24)));
        assert_eq!(b64(&random(24)).len(), 32);
        assert!(same("123456", "123456") && !same("123456", "123457") && !same("12345", "123456"));
    }
}
