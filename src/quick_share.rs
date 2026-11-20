//! Quick Share (Google's Nearby Share) sender: files from Send go to an Android phone on the same Wi-Fi.
//! Written from the Nearby Connections / Nearby Sharing wire formats Google publishes in google/nearby (Apache-2.0) and the
//! community notes of NearDrop and rquickshare: mDNS discovery (plus a BLE nudge that makes phones start advertising), the UKEY2
//! handshake, the AES-256-CBC + HMAC-SHA256 secure channel and the sharing frames (paired key, introduction, accept, payloads).
//! Only the phone's "Everyone" visibility can work: contacts-only needs Google account certificates.
#![cfg_attr(not(win), allow(dead_code))]
use std::{collections::HashMap, io::{Read, Write}, net::{SocketAddr, TcpStream}, path::Path, time::{Duration, Instant}};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::Engine;
use ring::{agreement, digest, hkdf, hmac, rand::{SecureRandom, SystemRandom}};
use crate::{err, AppResult};
// What differs per OS lives in win/quick_share.rs and mac/quick_share.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/quick_share.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/quick_share.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;

pub const SERVICE: &str = "_FC9F5ED42C8A._tcp.local.";
const NEXT_PROTOCOL: &str = "AES_256_CBC-HMAC_SHA256";
/// SHA-256("D2D") and SHA-256("SecureMessage"), the HKDF salts of the D2D and SecureMessage layers.
const D2D_SALT: &str = "82AA55A0D397F88346CA1CEE8D3909B95F13FA7DEB1D4AB38376B8256DA85510";
const SM_SALT: &str = "BF9D2A53C63616D75DB0A7165B91C1EF73E537F2427405FA23610A4BE657642E";
const CHUNK: usize = 512 * 1024;

// ---------------------------------------------------------------- protobuf, by hand (only a handful of messages are needed)
#[derive(Default, Clone)]
struct Pb(Vec<u8>);
fn put_varint(out: &mut Vec<u8>, mut v: u64) { loop { let b = (v & 0x7f) as u8; v >>= 7; if v == 0 { out.push(b); break; } out.push(b | 0x80); } }
impl Pb {
    fn new() -> Self { Self::default() }
    fn int(mut self, field: u32, v: u64) -> Self { put_varint(&mut self.0, (field as u64) << 3); put_varint(&mut self.0, v); self }
    fn bytes(mut self, field: u32, v: &[u8]) -> Self { put_varint(&mut self.0, ((field as u64) << 3) | 2); put_varint(&mut self.0, v.len() as u64); self.0.extend_from_slice(v); self }
    fn str(self, field: u32, v: &str) -> Self { self.bytes(field, v.as_bytes()) }
    fn msg(self, field: u32, m: Pb) -> Self { self.bytes(field, &m.0) }
}
enum Val<'a> { Int(u64), Bytes(&'a [u8]) }
struct Fields<'a>(Vec<(u32, Val<'a>)>);
fn get_varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in 0..10 { let x = *b.get(*i)?; *i += 1; v |= ((x & 0x7f) as u64) << (7 * shift); if x & 0x80 == 0 { return Some(v); } }
    None
}
impl<'a> Fields<'a> {
    fn parse(b: &'a [u8]) -> Option<Self> {
        let (mut i, mut out) = (0, Vec::new());
        while i < b.len() {
            let key = get_varint(b, &mut i)?;
            let field = (key >> 3) as u32;
            match key & 7 {
                0 => out.push((field, Val::Int(get_varint(b, &mut i)?))),
                2 => { let n = get_varint(b, &mut i)? as usize; let end = i.checked_add(n).filter(|e| *e <= b.len())?; out.push((field, Val::Bytes(&b[i..end]))); i = end; }
                1 => i += 8,
                5 => i += 4,
                _ => return None,
            }
        }
        (i == b.len()).then_some(Self(out))
    }
    fn int(&self, field: u32) -> Option<u64> { self.0.iter().find_map(|(k, v)| match v { Val::Int(x) if *k == field => Some(*x), _ => None }) }
    fn bytes(&self, field: u32) -> Option<&'a [u8]> { self.0.iter().find_map(|(k, v)| match v { Val::Bytes(x) if *k == field => Some(*x), _ => None }) }
    #[cfg(test)]
    fn all(&self, field: u32) -> Vec<Fields<'a>> { self.0.iter().filter_map(|(k, v)| match v { Val::Bytes(x) if *k == field => Fields::parse(x), _ => None }).collect() }
    fn msg(&self, field: u32) -> Option<Fields<'a>> { Fields::parse(self.bytes(field)?) }
}
/// Both OfflineFrame (Nearby Connections) and Frame (Nearby Sharing) are { version: V1, v1: ... }.
fn framed(v1: Pb) -> Vec<u8> { Pb::new().int(1, 1).msg(2, v1).0 }
/// The V1 frame inside an OfflineFrame or sharing Frame, and its type.
fn v1_of<'a>(frame: &'a [u8]) -> Option<(u64, Fields<'a>)> { let v1 = Fields::parse(frame)?.msg(2)?; Some((v1.int(1)?, v1)) }

// ---------------------------------------------------------------- discovery
/// `name` is what the device itself advertises (its Quick Share name); `host` is its network host name, a hint when it advertises none.
pub struct Found { pub endpoint: String, pub name: String, pub host: Option<String>, pub addr: SocketAddr, pub kind: &'static str }
fn b64(s: &str) -> Option<Vec<u8>> { base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok() }
/// The mDNS instance name (PCP 0x23, 4-byte endpoint id, service hash FC9F5E, 2 zero bytes) and the "n" TXT record
/// (flags with the device type, 16 random bytes, then name length and name): the endpoint id and a display name.
/// Phones that are not visible to everyone leave the name out: they get a generic one from the device type.
/// Returns the endpoint id, the name and the kind ("phone", "tablet" or "laptop").
fn parse_advert(instance: &str, n: &str) -> Option<(String, String, &'static str)> {
    let id = b64(instance)?;
    if id.len() < 10 || id[0] != 0x23 || id[5..8] != [0xFC, 0x9F, 0x5E] { return None; }
    let info = b64(n)?;
    if info.len() < 17 { return None; }
    let named = info.get(17).and_then(|len| info.get(18..18 + *len as usize)).and_then(|b| String::from_utf8(b.to_vec()).ok()).filter(|s| !s.trim().is_empty());
    let (kind, generic) = match (info[0] >> 1) & 7 { 2 => ("tablet", "Android tablet"), 3 => ("laptop", "Android laptop"), _ => ("phone", "Android phone") };
    Some((String::from_utf8_lossy(&id[1..5]).into_owned(), named.unwrap_or_else(|| generic.into()), kind))
}
/// The phone's network host name as a readable device name ("Pixel-8.local." -> "Pixel 8"), or None for the anonymous ones
/// Android uses when it has no name ("Android-3f9a1c7e", a bare hex string).
fn host_name(target: &str) -> Option<String> {
    let label = target.trim_end_matches('.').split('.').next()?.trim();
    let lower = label.to_ascii_lowercase();
    let hexish = |s: &str| s.len() >= 6 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let hash = lower.strip_prefix("android").map(|r| r.trim_start_matches(['-', '_', ' '])).map_or(false, |r| r.is_empty() || hexish(r));
    if label.chars().count() < 3 || hash || hexish(&lower) || lower == "localhost" { return None; }
    Some(label.replace(['-', '_'], " ").chars().take(40).collect())
}

/// Phones visible to everyone on this network, through Windows' own mDNS (the DNS Client service). Its replies pass the firewall
/// on Public networks, where an app's own mDNS socket hears nothing back.
pub fn discover(wait: Duration) -> AppResult<Vec<Found>> {
    discover_with(wait, |_| {})
}

// ---------------------------------------------------------------- framing and keys
fn lost(e: std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) { "The phone stopped responding.".into() } else { "Connection to the phone was lost.".into() }
}
fn write_frame(s: &mut TcpStream, b: &[u8]) -> AppResult<()> {
    let mut v = Vec::with_capacity(b.len() + 4);
    v.extend_from_slice(&(b.len() as u32).to_be_bytes());
    v.extend_from_slice(b);
    s.write_all(&v).map_err(lost)
}
fn read_frame(s: &mut TcpStream) -> AppResult<Vec<u8>> {
    let mut n = [0u8; 4];
    s.read_exact(&mut n).map_err(lost)?;
    let n = u32::from_be_bytes(n) as usize;
    if n > 8 << 20 { return Err("Quick Share: the phone sent an oversized message.".into()); }
    let mut b = vec![0; n];
    s.read_exact(&mut b).map_err(lost)?;
    Ok(b)
}
fn unhex(s: &str) -> Vec<u8> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0)).collect() }
struct Len(usize);
impl hkdf::KeyType for Len { fn len(&self) -> usize { self.0 } }
fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(ikm);
    if let Ok(okm) = prk.expand(&[info], Len(32)) { let _ = okm.fill(&mut out); }
    out
}
#[derive(Clone, Copy)]
struct Keys { enc: [u8; 32], sign: [u8; 32] }
/// From SHA-256 of the ECDH secret and the two init messages: the auth string (for the code both screens show) and each side's keys.
fn derive(dhs: &[u8], info: &[u8]) -> (Vec<u8>, Keys, Keys) {
    let auth = hkdf32(dhs, b"UKEY2 v1 auth", info);
    let next = hkdf32(dhs, b"UKEY2 v1 next", info);
    let (d2d, sm) = (unhex(D2D_SALT), unhex(SM_SALT));
    let side = |name: &[u8]| { let k = hkdf32(&next, &d2d, name); Keys { enc: hkdf32(&k, &sm, b"ENC:2"), sign: hkdf32(&k, &sm, b"SIG:1") } };
    (auth.to_vec(), side(b"client"), side(b"server"))
}
/// The 4-digit code Android shows on its accept screen, from the auth string.
fn pin(auth: &[u8]) -> String {
    let (mut hash, mut mult) = (0i32, 1i32);
    for &b in auth { hash = (hash + (b as i8 as i32) * mult) % 9973; mult = (mult * 31) % 9973; }
    format!("{:04}", hash.abs())
}
/// Java BigInteger bytes: a leading zero keeps a coordinate with its top bit set positive.
fn signed(c: &[u8]) -> Vec<u8> { if c[0] & 0x80 != 0 { [&[0u8][..], c].concat() } else { c.to_vec() } }
fn coord(c: &[u8]) -> Option<[u8; 32]> {
    let skip = c.len().saturating_sub(32);
    if c[..skip].iter().any(|b| *b != 0) { return None; }
    let c = &c[skip..];
    let mut out = [0u8; 32];
    out[32 - c.len()..].copy_from_slice(c);
    Some(out)
}
/// GenericPublicKey { type: EC_P256, ec_p256_public_key { x, y } } from an uncompressed point, and back.
fn key_proto(point: &[u8]) -> Pb { Pb::new().int(1, 1).msg(2, Pb::new().bytes(1, &signed(&point[1..33])).bytes(2, &signed(&point[33..65]))) }
fn key_point(b: &[u8]) -> Option<Vec<u8>> {
    let k = Fields::parse(b)?;
    if k.int(1) != Some(1) { return None; }
    let ec = k.msg(2)?;
    let mut p = vec![4u8];
    p.extend(coord(ec.bytes(1)?)?);
    p.extend(coord(ec.bytes(2)?)?);
    Some(p)
}
/// UKEY2 as the client (P-256, SHA-512 commitment): returns the auth string, then our keys and the phone's.
fn client_handshake(s: &mut TcpStream, rng: &SystemRandom) -> AppResult<(Vec<u8>, Keys, Keys)> {
    let refused = || "The phone refused the secure handshake.".to_string();
    let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, rng).map_err(|_| refused())?;
    let public = private.compute_public_key().map_err(|_| refused())?;
    let finish = Pb::new().int(1, 4).msg(2, Pb::new().msg(1, key_proto(public.as_ref()))).0;
    let mut random = [0u8; 32];
    rng.fill(&mut random).map_err(|_| refused())?;
    let commitment = digest::digest(&digest::SHA512, &finish);
    let init = Pb::new().int(1, 2).msg(2, Pb::new().int(1, 1).bytes(2, &random).msg(3, Pb::new().int(1, 100).bytes(2, commitment.as_ref())).str(4, NEXT_PROTOCOL)).0;
    write_frame(s, &init)?;
    let reply = read_frame(s)?;
    let point = Fields::parse(&reply).filter(|m| m.int(1) == Some(3)).and_then(|m| m.msg(2)).filter(|m| m.int(3) == Some(100))
        .and_then(|m| m.bytes(4).and_then(key_point)).ok_or_else(refused)?;
    write_frame(s, &finish)?;
    let dhs = agreement::agree_ephemeral(private, &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, &point), |k| digest::digest(&digest::SHA256, k)).map_err(|_| refused())?;
    let (auth, client, server) = derive(dhs.as_ref(), &[init.as_slice(), &reply].concat());
    Ok((auth, client, server))
}

// ---------------------------------------------------------------- the secure channel (SecureMessage / DeviceToDeviceMessage)
type Enc = cbc::Encryptor<aes::Aes256>;
type Dec = cbc::Decryptor<aes::Aes256>;
struct Channel { s: TcpStream, out: Keys, inn: Keys, seq: u64, rng: SystemRandom, pending: HashMap<u64, Vec<u8>> }
impl Channel {
    fn new(s: TcpStream, out: Keys, inn: Keys) -> Self { Self { s, out, inn, seq: 0, rng: SystemRandom::new(), pending: HashMap::new() } }
    fn random(&self, n: usize) -> Vec<u8> { let mut v = vec![0u8; n]; let _ = self.rng.fill(&mut v); v }
    /// A positive random 63-bit id (payload and file ids).
    fn id(&self) -> u64 { let b = self.random(8); u64::from_le_bytes(b.try_into().unwrap_or_default()) >> 1 }
    fn send(&mut self, message: &[u8]) -> AppResult<()> {
        self.seq += 1;
        let d2d = Pb::new().bytes(1, message).int(2, self.seq).0;
        let iv: [u8; 16] = self.random(16).try_into().unwrap_or_default();
        let body = Enc::new(&self.out.enc.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(&d2d);
        // Header: HMAC_SHA256, AES_256_CBC, iv, GcmMetadata { DEVICE_TO_DEVICE_MESSAGE, version 1 }.
        let header = Pb::new().int(1, 1).int(2, 2).bytes(5, &iv).msg(6, Pb::new().int(1, 13).int(2, 1));
        let hb = Pb::new().msg(1, header).bytes(2, &body).0;
        let sig = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &self.out.sign), &hb);
        write_frame(&mut self.s, &Pb::new().bytes(1, &hb).bytes(2, sig.as_ref()).0)
    }
    fn recv(&mut self) -> AppResult<Vec<u8>> {
        let raw = read_frame(&mut self.s)?;
        let bad = || "Quick Share: unreadable message from the phone.".to_string();
        let sm = Fields::parse(&raw).ok_or_else(bad)?;
        let (hb, sig) = (sm.bytes(1).ok_or_else(bad)?, sm.bytes(2).ok_or_else(bad)?);
        hmac::verify(&hmac::Key::new(hmac::HMAC_SHA256, &self.inn.sign), hb, sig).map_err(|_| "Quick Share: message signature mismatch.")?;
        let f = Fields::parse(hb).ok_or_else(bad)?;
        let iv: [u8; 16] = f.msg(1).and_then(|h| h.bytes(5)).and_then(|v| v.try_into().ok()).ok_or_else(bad)?;
        let plain = Dec::new(&self.inn.enc.into(), &iv.into()).decrypt_padded_vec_mut::<Pkcs7>(f.bytes(2).ok_or_else(bad)?).map_err(|_| bad())?;
        Ok(Fields::parse(&plain).and_then(|d| d.bytes(1)).ok_or_else(bad)?.to_vec())
    }
    /// One payload-transfer OfflineFrame (packet DATA) carrying a chunk of a BYTES or FILE payload.
    fn payload(&mut self, header: Pb, flags: u64, offset: u64, body: &[u8]) -> AppResult<()> {
        let mut chunk = Pb::new().int(1, flags).int(2, offset);
        if !body.is_empty() { chunk = chunk.bytes(3, body); }
        self.send(&framed(Pb::new().int(1, 3).msg(4, Pb::new().int(1, 1).msg(2, header).msg(3, chunk))))
    }
    /// A sharing frame travels as a BYTES payload: the whole frame, then an empty last chunk.
    fn send_sharing(&mut self, v1: Pb) -> AppResult<()> {
        let body = framed(v1);
        let header = Pb::new().int(1, self.id()).int(2, 1).int(3, body.len() as u64);
        self.payload(header.clone(), 0, 0, &body)?;
        self.payload(header, 1, body.len() as u64, &[])
    }
    /// The next complete sharing frame; keep-alives are answered on the way.
    fn recv_sharing(&mut self) -> AppResult<Vec<u8>> {
        loop {
            let raw = self.recv()?;
            let Some((kind, v1)) = v1_of(&raw) else { continue };
            match kind {
                3 => {
                    let Some((h, c)) = v1.msg(4).and_then(|t| Some((t.msg(2)?, t.msg(3)?))) else { continue };
                    if h.int(2) != Some(1) { continue; }
                    let id = h.int(1).unwrap_or_default();
                    let buf = self.pending.entry(id).or_default();
                    buf.extend_from_slice(c.bytes(3).unwrap_or_default());
                    if buf.len() > 1 << 20 { return Err("Quick Share: the phone sent an oversized message.".into()); }
                    if c.int(1).unwrap_or_default() & 1 == 1 { return Ok(self.pending.remove(&id).unwrap_or_default()); }
                }
                // Answer a keep-alive (but never an answer: that would ping-pong forever).
                5 if v1.msg(6).and_then(|k| k.int(1)) != Some(1) => self.send(&framed(Pb::new().int(1, 5).msg(6, Pb::new().int(1, 1))))?,
                6 => return Err("The phone ended the transfer.".into()),
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------- sending
pub struct Item { pub path: String, pub name: String, pub size: u64 }
pub enum Event { Code(String), Progress { index: usize, name: String, sent: u64, total: u64 } }
pub struct Outcome { pub completed: Vec<String>, pub declined: bool, pub error: Option<String> }
/// The sharing FileMetadata type (UNKNOWN 0, IMAGE 1, VIDEO 2, APP 3, AUDIO 4) and MIME type, from the extension.
fn file_kind(name: &str) -> (u64, &'static str) {
    match Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default().as_str() {
        "jpg" | "jpeg" => (1, "image/jpeg"), "png" => (1, "image/png"), "gif" => (1, "image/gif"), "webp" => (1, "image/webp"), "heic" => (1, "image/heic"), "bmp" => (1, "image/bmp"),
        "mp4" => (2, "video/mp4"), "mov" => (2, "video/quicktime"), "mkv" => (2, "video/x-matroska"), "webm" => (2, "video/webm"), "3gp" => (2, "video/3gpp"),
        "mp3" => (4, "audio/mpeg"), "m4a" => (4, "audio/mp4"), "wav" => (4, "audio/wav"), "flac" => (4, "audio/flac"), "ogg" | "opus" => (4, "audio/ogg"),
        "apk" => (3, "application/vnd.android.package-archive"), "pdf" => (0, "application/pdf"), "txt" => (0, "text/plain"), "zip" => (0, "application/zip"),
        _ => (0, "application/octet-stream"),
    }
}
/// Our EndpointInfo: device type laptop (3 << 1), 16 random bytes, the name.
fn endpoint_info(name: &str, rng: &SystemRandom) -> Vec<u8> {
    let mut v = vec![3u8 << 1];
    let mut salt = [0u8; 16];
    let _ = rng.fill(&mut salt);
    v.extend_from_slice(&salt);
    let name: String = name.chars().take(32).collect();
    v.push(name.len() as u8);
    v.extend_from_slice(name.as_bytes());
    v
}
fn endpoint_id(rng: &SystemRandom) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut r = [0u8; 4];
    let _ = rng.fill(&mut r);
    r.iter().map(|b| ABC[*b as usize % ABC.len()] as char).collect()
}

/// Connects, pairs, asks the phone (its user accepts on screen) and streams the files. Err only before anything was sent.
pub fn send(addr: SocketAddr, me: &str, items: Vec<Item>, mut event: impl FnMut(Event)) -> AppResult<Outcome> {
    let rng = SystemRandom::new();
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|_| "Phone unreachable · same Wi-Fi?")?;
    let _ = s.set_nodelay(true);
    s.set_read_timeout(Some(Duration::from_secs(20))).map_err(err)?;
    s.set_write_timeout(Some(Duration::from_secs(60))).map_err(err)?;
    // ConnectionRequest: endpoint id, name, EndpointInfo, medium WIFI_LAN.
    let request = Pb::new().str(1, &endpoint_id(&rng)).str(2, me).int(5, 5).bytes(6, &endpoint_info(me, &rng));
    write_frame(&mut s, &framed(Pb::new().int(1, 1).msg(2, request)))?;
    let (auth, ours, theirs) = client_handshake(&mut s, &rng)?;
    // ConnectionResponse in the clear (ACCEPT, os LINUX), then the phone's own.
    write_frame(&mut s, &framed(Pb::new().int(1, 2).msg(3, Pb::new().int(1, 0).int(3, 1).msg(4, Pb::new().int(1, 100)))))?;
    let answer = read_frame(&mut s)?;
    let accepted = v1_of(&answer).filter(|(k, _)| *k == 2).and_then(|(_, v)| v.msg(3)).map_or(false, |r| r.int(3) == Some(1) || (r.int(3).is_none() && r.int(1) == Some(0)));
    if !accepted { return Err("The phone refused the connection.".into()); }
    event(Event::Code(pin(&auth)));

    let mut ch = Channel::new(s, ours, theirs);
    // Paired-key frames carry contact certificates we do not have: random bytes, then "UNABLE" so the phone treats us as a stranger.
    let pke = Pb::new().bytes(1, &ch.random(72)).bytes(2, &ch.random(6));
    ch.send_sharing(Pb::new().int(1, 3).msg(4, pke))?;
    let ids: Vec<(u64, u64)> = items.iter().map(|_| (ch.id(), ch.id())).collect();
    let mut introduced = false;
    loop {
        let frame = ch.recv_sharing()?;
        let Some((kind, v1)) = v1_of(&frame) else { continue };
        match kind {
            3 => ch.send_sharing(Pb::new().int(1, 4).msg(5, Pb::new().int(1, 3)))?,
            4 if !introduced => {
                let mut intro = Pb::new();
                for (item, (payload, id)) in items.iter().zip(&ids) {
                    let (type_, mime) = file_kind(&item.name);
                    intro = intro.msg(1, Pb::new().str(1, &item.name).int(2, type_).int(3, *payload).int(4, item.size).str(5, mime).int(6, *id));
                }
                ch.send_sharing(Pb::new().int(1, 1).msg(2, intro))?;
                introduced = true;
                // The phone's user now decides.
                ch.s.set_read_timeout(Some(Duration::from_secs(300))).map_err(err)?;
            }
            2 => {
                if v1.msg(3).and_then(|r| r.int(1)) == Some(1) { break; }
                return Ok(Outcome { completed: vec![], declined: true, error: None });
            }
            6 => return Ok(Outcome { completed: vec![], declined: true, error: None }),
            _ => {}
        }
    }

    let total: u64 = items.iter().map(|i| i.size).sum();
    let (mut done, mut completed, mut error) = (0u64, Vec::new(), None);
    let mut buf = vec![0u8; CHUNK];
    'files: for (index, (item, (payload, _))) in items.iter().zip(&ids).enumerate() {
        let header = || Pb::new().int(1, *payload).int(2, 2).int(3, item.size).int(4, 0).str(5, &item.name);
        let mut file = match std::fs::File::open(&item.path) { Ok(f) => f, Err(_) => { error = Some(format!("Cannot open {}.", item.name)); break } };
        let (mut offset, mut last) = (0u64, Instant::now());
        event(Event::Progress { index: index + 1, name: item.name.clone(), sent: done, total });
        loop {
            let n = match file.read(&mut buf) { Ok(n) => n, Err(_) => { error = Some(format!("Cannot read {}.", item.name)); break 'files } };
            let sent = if n == 0 { ch.payload(header(), 1, offset, &[]) } else { ch.payload(header(), 0, offset, &buf[..n]) };
            if sent.is_err() { error = Some(format!("Connection lost during {}.", item.name)); break 'files; }
            if n == 0 { break; }
            offset += n as u64;
            if last.elapsed() > Duration::from_millis(130) { event(Event::Progress { index: index + 1, name: item.name.clone(), sent: done + offset, total }); last = Instant::now(); }
        }
        done += offset;
        completed.push(item.path.clone());
    }
    event(Event::Progress { index: items.len(), name: String::new(), sent: done, total });
    // Let the phone finish writing (it closes or says goodbye), then say goodbye ourselves.
    let _ = ch.s.set_read_timeout(Some(Duration::from_secs(15)));
    if error.is_none() { let until = Instant::now() + Duration::from_secs(15); while Instant::now() < until { if ch.recv().map(|f| v1_of(&f).map(|(k, _)| k) == Some(6)).unwrap_or(true) { break; } } }
    let _ = ch.send(&framed(Pb::new().int(1, 6).msg(7, Pb::new())));
    let _ = ch.s.shutdown(std::net::Shutdown::Both);
    Ok(Outcome { completed, declined: false, error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn host_names_make_better_device_names_than_a_generic_label() {
        assert_eq!(host_name("Pixel-8.local.").as_deref(), Some("Pixel 8"));
        assert_eq!(host_name("Galaxy-S23-Ultra.local").as_deref(), Some("Galaxy S23 Ultra"));
        assert_eq!(host_name("Alex_s-Phone.local.").as_deref(), Some("Alex s Phone"));
        for anonymous in ["Android-3f9a1c7e.local.", "android.local", "3f9a1c7e5b2d.local.", "localhost", "ab.local"] { assert_eq!(host_name(anonymous), None, "{anonymous}"); }
    }
    #[test]
    fn salts_are_the_sha256_of_their_names() {
        assert_eq!(unhex(D2D_SALT), digest::digest(&digest::SHA256, b"D2D").as_ref());
        assert_eq!(unhex(SM_SALT), digest::digest(&digest::SHA256, b"SecureMessage").as_ref());
    }
    #[test]
    fn protobuf_round_trip_and_adverts() {
        let m = Pb::new().int(1, 300).str(2, "hi").msg(3, Pb::new().int(1, u64::MAX >> 1)).0;
        let f = Fields::parse(&m).unwrap();
        assert_eq!(f.int(1), Some(300));
        assert_eq!(f.bytes(2), Some(&b"hi"[..]));
        assert_eq!(f.msg(3).unwrap().int(1), Some(u64::MAX >> 1));
        assert!(Fields::parse(&[0x0a, 0x05, 1]).is_none(), "truncated length is rejected");
        let id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0x23, b'A', b'B', b'C', b'D', 0xFC, 0x9F, 0x5E, 0, 0]);
        let info = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(endpoint_info("Pixel 8", &SystemRandom::new()));
        assert_eq!(parse_advert(&id, &info), Some(("ABCD".into(), "Pixel 8".into(), "laptop")));
        assert_eq!(parse_advert(&id[1..], &info), None);
        // A real phone's advert, not visible to everyone: no name, device type phone.
        assert_eq!(parse_advert("IzVSMlP8n14AAA", "MpyMpADMCpyP8b5gmOOwGdM"), Some(("5R2S".into(), "Android phone".into(), "phone")));
        assert_eq!(coord(&[0, 0x80, 1]).unwrap()[30..], [0x80, 1]);
        assert_eq!(signed(&[0x80, 1]), vec![0, 0x80, 1]);
        assert_eq!(pin(&[0; 32]), "0000");
    }

    /// A pretend phone: the receiving side of the whole exchange, so a full transfer runs over loopback.
    fn phone(listener: TcpListener, accept: bool) -> (String, Vec<(String, Vec<u8>)>) {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let request = read_frame(&mut s).unwrap();
        assert_eq!(v1_of(&request).unwrap().0, 1);
        let init = read_frame(&mut s).unwrap();
        let commitment = Fields::parse(&init).unwrap().msg(2).unwrap().msg(3).unwrap().bytes(2).unwrap().to_vec();
        let rng = SystemRandom::new();
        let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let public = private.compute_public_key().unwrap();
        let server_init = Pb::new().int(1, 3).msg(2, Pb::new().int(1, 1).bytes(2, &[7; 32]).int(3, 100).msg(4, key_proto(public.as_ref()))).0;
        write_frame(&mut s, &server_init).unwrap();
        let finish = read_frame(&mut s).unwrap();
        assert_eq!(digest::digest(&digest::SHA512, &finish).as_ref(), &commitment[..], "client commitment");
        let point = key_point(Fields::parse(&finish).unwrap().msg(2).unwrap().bytes(1).unwrap()).unwrap();
        let dhs = agreement::agree_ephemeral(private, &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, &point), |k| digest::digest(&digest::SHA256, k)).unwrap();
        let (auth, client, server) = derive(dhs.as_ref(), &[init.as_slice(), &server_init].concat());
        assert_eq!(v1_of(&read_frame(&mut s).unwrap()).unwrap().0, 2);
        write_frame(&mut s, &framed(Pb::new().int(1, 2).msg(3, Pb::new().int(1, 0).int(3, 1).msg(4, Pb::new().int(1, 1))))).unwrap();
        let mut ch = Channel::new(s, server, client);
        let kind = |b: &[u8]| v1_of(b).unwrap().0;
        assert_eq!(kind(&ch.recv_sharing().unwrap()), 3);
        ch.send(&framed(Pb::new().int(1, 5).msg(6, Pb::new()))).unwrap(); // a keep-alive the sender must answer
        ch.send_sharing(Pb::new().int(1, 3).msg(4, Pb::new().bytes(1, &[1; 72]).bytes(2, &[2; 6]))).unwrap();
        assert_eq!(kind(&ch.recv_sharing().unwrap()), 4);
        ch.send_sharing(Pb::new().int(1, 4).msg(5, Pb::new().int(1, 3))).unwrap();
        let intro = ch.recv_sharing().unwrap();
        let (k, v1) = v1_of(&intro).unwrap();
        assert_eq!(k, 1);
        let files: Vec<(u64, String, u64)> = v1.msg(2).unwrap().all(1).iter().map(|f| (f.int(3).unwrap(), String::from_utf8(f.bytes(1).unwrap().to_vec()).unwrap(), f.int(4).unwrap())).collect();
        ch.send_sharing(Pb::new().int(1, 2).msg(3, Pb::new().int(1, if accept { 1 } else { 2 }))).unwrap();
        let mut got: HashMap<u64, Vec<u8>> = HashMap::new();
        let mut finished = 0;
        while accept && finished < files.len() {
            let raw = match ch.recv() { Ok(r) => r, Err(_) => break };
            let Some((3, v1)) = v1_of(&raw) else { continue };
            let t = v1.msg(4).unwrap();
            let (h, c) = (t.msg(2).unwrap(), t.msg(3).unwrap());
            if h.int(2) != Some(2) { continue; }
            let id = h.int(1).unwrap();
            assert_eq!(c.int(2).unwrap() as usize, got.get(&id).map_or(0, |b| b.len()), "offsets are contiguous");
            got.entry(id).or_default().extend_from_slice(c.bytes(3).unwrap_or_default());
            if c.int(1) == Some(1) { finished += 1; }
        }
        if accept { ch.send(&framed(Pb::new().int(1, 6).msg(7, Pb::new()))).unwrap(); }
        let out = files.into_iter().map(|(id, name, size)| { let b = got.remove(&id).unwrap_or_default(); assert_eq!(b.len() as u64, if accept { size } else { 0 }); (name, b) }).collect();
        (pin(&auth), out)
    }

    #[cfg(win)]
    /// Live check on this machine's network: cargo test --release quick_share::tests::scan_now -- --ignored --nocapture
    #[test]
    #[ignore]
    fn scan_now() {
        println!("bluetooth nudge: {}", if Nudge::start().0.is_some() { "advertising" } else { "unavailable" });
        let found = discover_with(Duration::from_secs(25), |name| println!("answered: {name}")).unwrap();
        for f in &found { println!("phone: {} [{}] at {}", f.name, f.endpoint, f.addr); }
        println!("{} phone(s)", found.len());
    }
    /// Live send of a tiny file to the first phone found: cargo test --release quick_share::tests::send_now -- --ignored --nocapture
    #[test]
    #[ignore]
    fn send_now() {
        let phone = discover(Duration::from_secs(12)).unwrap().into_iter().next().expect("no phone found");
        println!("sending to {} at {}", phone.name, phone.addr);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sora-test.txt");
        std::fs::write(&path, b"Hello from Sora (Quick Share test).").unwrap();
        let items = vec![Item { path: path.to_string_lossy().into(), name: "sora-test.txt".into(), size: 35 }];
        let out = send(phone.addr, "Sora test", items, |e| match e { Event::Code(c) => println!("code {c}"), Event::Progress { sent, total, .. } => println!("progress {sent}/{total}") });
        match out { Ok(o) => println!("completed {:?} declined {} error {:?}", o.completed.len(), o.declined, o.error), Err(e) => println!("error: {e}") }
    }
    #[test]
    fn full_transfer_over_loopback() {
        let dir = tempfile::tempdir().unwrap();
        let big: Vec<u8> = (0..(CHUNK * 2 + 777)).map(|i| (i * 31 % 251) as u8).collect();
        let (a, b) = (dir.path().join("photo.jpg"), dir.path().join("notes.txt"));
        std::fs::write(&a, &big).unwrap();
        std::fs::write(&b, b"hello phone").unwrap();
        for accept in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let phone = std::thread::spawn(move || phone(listener, accept));
            let items = vec![
                Item { path: a.to_string_lossy().into(), name: "photo.jpg".into(), size: big.len() as u64 },
                Item { path: b.to_string_lossy().into(), name: "notes.txt".into(), size: 11 },
            ];
            let mut code = String::new();
            let mut last = 0;
            let out = send(addr, "Sora test", items, |e| match e { Event::Code(c) => code = c, Event::Progress { sent, .. } => last = sent }).unwrap();
            let (phone_code, files) = phone.join().unwrap();
            assert_eq!(code, phone_code, "both sides show the same code");
            assert_eq!(code.len(), 4);
            if accept {
                assert!(out.error.is_none() && !out.declined && out.completed.len() == 2);
                assert_eq!(last, big.len() as u64 + 11);
                assert_eq!(files[0], ("photo.jpg".into(), big.clone()));
                assert_eq!(files[1], ("notes.txt".into(), b"hello phone".to_vec()));
            } else {
                assert!(out.declined && out.completed.is_empty());
            }
        }
    }
}
