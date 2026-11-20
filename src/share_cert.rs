//! A trusted HTTPS name for the QR page, so a phone's browser shows no certificate warning.
//!
//! Each install gets its own name, `<16 characters>.<domain>`, derived from its own key. A small service (share-service/)
//! points that name at this PC's home-network address in DNS and gets a Let's Encrypt certificate for it from a CSR made
//! here: the certificate's private key never leaves this PC, and no file name, file or code ever goes to the service.
//! The service sees an install id, a home-network address and a CSR. Without a service address in `share_service.txt`, or
//! when anything fails (service down, DNS blocked by the router), `trusted` gives None and web_share keeps the old
//! self-signed certificate: the page then works as before, with the browser's warning.
use std::{net::IpAddr, path::PathBuf, sync::Mutex, time::{Duration, SystemTime, UNIX_EPOCH}};
use base64::Engine;
use ring::{digest, rand::SystemRandom, signature::{Ed25519KeyPair, KeyPair}};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// "<service address> <domain>", e.g. "https://send.soranotch.com share.soranotch.com". Empty: the feature is off.
const CONFIG: &str = include_str!("share_service.txt");

pub struct Trusted { pub host: String, pub chain: Vec<CertificateDer<'static>>, pub key: PrivateKeyDer<'static> }

#[derive(Serialize, Deserialize)]
struct Stored { host: String, chain_pem: String, key_der: String, not_before: i64, not_after: i64 }
#[derive(Deserialize)]
struct Issued { chain: String, host: String, #[serde(rename = "notBefore")] not_before: i64, #[serde(rename = "notAfter")] not_after: i64 }

/// The last answer for one home address, so the QR shows at once: a name that works (no service call needed until then) or a
/// network that cannot have one (router "rebind protection", service down). A restart of Sora or a new address asks again.
struct Memo { ip: IpAddr, until: i64, ok: bool }
static MEMO: Mutex<Option<Memo>> = Mutex::new(None);
/// One try at a time: the click that comes while the warm-up is still asking waits for it and then uses its answer.
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const OK_FOR: i64 = 10 * 60;
const BLOCKED_FOR: i64 = 30 * 60;
const FAILED_FOR: i64 = 2 * 60;
fn memo(ip: IpAddr) -> Option<bool> { MEMO.lock().ok()?.as_ref().filter(|m| m.ip == ip && now() < m.until).map(|m| m.ok) }
fn set_memo(ip: IpAddr, ok: bool, for_secs: i64) { if let Ok(mut m) = MEMO.lock() { *m = Some(Memo { ip, until: now() + for_secs, ok }); } }

fn b64() -> base64::engine::GeneralPurpose { base64::engine::general_purpose::STANDARD }
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) }
/// The page a phone opens for Send > QR Code (rtc_share.rs), on the share service. None: the feature is off.
pub fn page() -> Option<String> { Some(format!("{}/", config()?.0)) }
fn config() -> Option<(&'static str, &'static str)> { let mut p = CONFIG.split_whitespace(); Some((p.next()?.trim_end_matches('/'), p.next()?)) }
fn dir(app: &AppHandle) -> Option<PathBuf> { let d = app.path().app_data_dir().ok()?; std::fs::create_dir_all(&d).ok()?; Some(d) }

/// Lower-case base32 without padding (the same as share-service/lib/core.js).
pub(crate) fn base32(bytes: &[u8]) -> String {
    const A: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let (mut bits, mut value, mut out) = (0u32, 0u32, String::new());
    for &b in bytes { value = (value << 8) | b as u32; bits += 8; while bits >= 5 { out.push(A[((value >> (bits - 5)) & 31) as usize] as char); bits -= 5; } }
    if bits > 0 { out.push(A[((value << (5 - bits)) & 31) as usize] as char); }
    out
}
/// The install's name: 16 characters of the hash of its public key.
pub(crate) fn install_name(public: &[u8]) -> String { base32(digest::digest(&digest::SHA256, public).as_ref())[..16].to_string() }

fn only_private_v4(ip: IpAddr) -> bool { matches!(ip, IpAddr::V4(v) if v.is_private()) }

/// This install's key: made once, kept in the app's data folder. It only proves to the service which install is asking.
fn device_key(app: &AppHandle) -> Option<Ed25519KeyPair> {
    let file = dir(app)?.join("share-device.key");
    if let Some(k) = std::fs::read(&file).ok().and_then(|b| Ed25519KeyPair::from_pkcs8(&b).ok()) { return Some(k); }
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).ok()?;
    std::fs::write(&file, pkcs8.as_ref()).ok()?;
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).ok()
}
/// The request body: who (public key), when, which home address and which CSR, signed with the device key.
fn signed_body(key: &Ed25519KeyPair, ip: IpAddr, csr: Option<&str>) -> serde_json::Value {
    let ts = now() * 1000 + 0;
    let sig = key.sign(format!("{ts}\n{ip}\n{}", csr.unwrap_or("")).as_bytes());
    let mut body = serde_json::json!({ "pub": b64().encode(key.public_key().as_ref()), "ts": ts, "ip": ip.to_string(), "sig": b64().encode(sig.as_ref()) });
    if let Some(c) = csr { body["csr"] = c.into(); }
    body
}

fn pem_certs(pem: &str) -> Vec<CertificateDer<'static>> {
    pem.split("-----BEGIN CERTIFICATE-----").skip(1).filter_map(|part| {
        let body: String = part.split("-----END CERTIFICATE-----").next()?.split_whitespace().collect();
        b64().decode(body).ok().map(CertificateDer::from)
    }).collect()
}
fn usable(s: &Stored) -> bool { let life = (s.not_after - s.not_before).max(1); s.not_after - now() > life / 3 && !pem_certs(&s.chain_pem).is_empty() }
fn to_trusted(s: &Stored) -> Option<Trusted> {
    let key = b64().decode(&s.key_der).ok()?;
    Some(Trusted { host: s.host.clone(), chain: pem_certs(&s.chain_pem), key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)) })
}

async fn post(client: &reqwest::Client, url: String, body: &serde_json::Value) -> Result<reqwest::Response, String> {
    let r = client.post(url).json(body).send().await.map_err(|e| format!("no answer ({})", if e.is_timeout() { "timeout" } else if e.is_connect() { "cannot connect" } else { "network error" }))?;
    if r.status().is_success() { Ok(r) } else { Err(format!("the service answered {}", r.status().as_u16())) }
}
/// Why the last try gave no trusted name (no secrets), for support: share-cert.log in the app's data folder.
fn why(app: &AppHandle, text: &str) { if let Some(d) = dir(app) { let _ = std::fs::write(d.join("share-cert.log"), format!("{} {text}
", now())); } }
async fn local_lookup(host: &str) -> Vec<IpAddr> {
    let h = format!("{host}:443");
    tauri::async_runtime::spawn_blocking(move || std::net::ToSocketAddrs::to_socket_addrs(&h).map(|a| a.map(|s| s.ip()).collect::<Vec<_>>()).unwrap_or_default()).await.unwrap_or_default()
}
/// What public DNS (Cloudflare, DNS over HTTPS) says the name points at. The name is already public in DNS; nothing else is sent.
async fn public_lookup(client: &reqwest::Client, host: &str) -> Vec<IpAddr> {
    let Ok(r) = client.get("https://cloudflare-dns.com/dns-query").query(&[("name", host), ("type", "A")]).header("accept", "application/dns-json").send().await else { return vec![] };
    let Ok(v) = r.json::<serde_json::Value>().await else { return vec![] };
    v["Answer"].as_array().map_or(vec![], |a| a.iter().filter_map(|x| x["data"].as_str()?.parse().ok()).collect())
}
/// Waits (up to 5 s) until this PC's own resolver returns the home address for the name: a router that blocks public names
/// pointing at private addresses ("DNS rebind protection") would stop the phone just the same. When public DNS already has the
/// address and this network's DNS gives nothing, the router filters it: waiting would not help, so it says so at once.
async fn resolves(client: &reqwest::Client, host: &str, ip: IpAddr) -> bool {
    for i in 0..5 {
        let found = local_lookup(host).await;
        if found.contains(&ip) { return true; }
        if i == 0 && found.is_empty() && public_lookup(client, host).await.contains(&ip) { return false; }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    false
}

/// A trusted name and certificate for this PC at `ip`, or None (then the caller keeps the self-signed one).
pub async fn trusted(app: &AppHandle, ip: IpAddr) -> Option<Trusted> {
    config()?;
    if !only_private_v4(ip) { return None; }
    let _one = BUSY.lock().await;
    match memo(ip) {
        Some(false) => return None,
        // The name already points here and the certificate is on disk: no service call, no DNS wait.
        Some(true) => if let Some(t) = saved(app).as_ref().and_then(to_trusted) { return Some(t) },
        None => {}
    }
    match try_trusted(app, ip).await {
        Ok(t) => { why(app, "ok"); set_memo(ip, true, OK_FOR); Some(t) }
        Err(e) => { set_memo(ip, false, if e.starts_with(BLOCKED) { BLOCKED_FOR } else { FAILED_FOR }); why(app, &e); None }
    }
}
const BLOCKED: &str = "this network's DNS does not return the name";
/// This install's name and its stored certificate, when that is still good.
fn saved(app: &AppHandle) -> Option<Stored> {
    let (_, domain) = config()?;
    let host = format!("{}.{domain}", install_name(device_key(app)?.public_key().as_ref()));
    let text = std::fs::read_to_string(dir(app)?.join("share-cert.json")).ok()?;
    serde_json::from_str::<Stored>(&text).ok().filter(|s| s.host == host && usable(s))
}
async fn try_trusted(app: &AppHandle, ip: IpAddr) -> Result<Trusted, String> {
    let (service, domain) = config().ok_or("off")?;
    let key = device_key(app).ok_or("no device key")?;
    let host = format!("{}.{domain}", install_name(key.public_key().as_ref()));
    let file = dir(app).ok_or("no data folder")?.join("share-cert.json");
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().map_err(|e| e.to_string())?;
    let stored = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Stored>(&t).ok()).filter(|s| s.host == host && usable(s));
    let stored = match stored {
        // A certificate still good: only the name's address is refreshed.
        // Public DNS already has this address: nothing to change, no service call (the slow part of showing the QR).
        Some(s) => { if !public_lookup(&client, &host).await.contains(&ip) { post(&client, format!("{service}/api/ip"), &signed_body(&key, ip, None)).await?; } s }
        None => {
            let pair = rcgen::KeyPair::generate().map_err(|e| format!("key: {e}"))?;
            let mut params = rcgen::CertificateParams::new(vec![host.clone()]).map_err(|e| format!("csr: {e}"))?;
            // Only the host: no default subject name (the service refuses a CSR that names anything else).
            params.distinguished_name = rcgen::DistinguishedName::new();
            params.distinguished_name.push(rcgen::DnType::CommonName, host.clone());
            let csr = params.serialize_request(&pair).map_err(|e| format!("csr: {e}"))?;
            let csr = b64().encode(csr.der().as_ref());
            // Issuing takes a while (DNS proof, Let's Encrypt); this happens once in about two months.
            let slow = reqwest::Client::builder().timeout(Duration::from_secs(75)).build().map_err(|e| e.to_string())?;
            let issued: Issued = post(&slow, format!("{service}/api/cert"), &signed_body(&key, ip, Some(&csr))).await?.json().await.map_err(|e| format!("answer not understood: {e}"))?;
            if issued.host != host { return Err("the service named another host".into()); }
            let s = Stored { host: host.clone(), chain_pem: issued.chain, key_der: b64().encode(pair.serialize_der()), not_before: issued.not_before, not_after: issued.not_after };
            std::fs::write(&file, serde_json::to_string(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            s
        }
    };
    if !resolves(&client, &host, ip).await { return Err(format!("{BLOCKED} (router rebind protection?)")); }
    to_trusted(&stored).ok_or_else(|| "stored certificate unreadable".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_match_the_service() {
        assert_eq!(base32(b"foobar"), "mzxw6ytboi", "same as share-service/test/core.test.js");
        let n = install_name(&[1u8; 32]);
        assert_eq!(n.len(), 16);
        assert!(n.bytes().all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)));
        assert_ne!(n, install_name(&[2u8; 32]));
    }
    #[test]
    fn only_home_addresses() {
        for ok in ["10.1.2.3", "192.168.0.9", "172.20.0.1"] { assert!(only_private_v4(ok.parse().unwrap()), "{ok}"); }
        for bad in ["8.8.8.8", "127.0.0.1", "169.254.1.1", "::1"] { assert!(!only_private_v4(bad.parse().unwrap()), "{bad}"); }
    }
    #[test]
    fn a_certificate_is_renewed_in_its_last_third() {
        let day = 86_400;
        let fresh = Stored { host: "h".into(), chain_pem: "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----".into(), key_der: String::new(), not_before: now() - day, not_after: now() + 89 * day };
        assert!(usable(&fresh));
        let old = Stored { not_before: now() - 70 * day, not_after: now() + 20 * day, ..fresh };
        assert!(!usable(&old), "20 of 90 days left: renew");
        assert!(pem_certs("-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nBBBB\n-----END CERTIFICATE-----").len() == 2);
    }
    #[test]
    fn the_request_is_signed_over_time_address_and_csr() {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let b = signed_body(&key, ip, Some("Y3Ny"));
        let msg = format!("{}\n{}\n{}", b["ts"], b["ip"].as_str().unwrap(), "Y3Ny");
        let sig = b64().decode(b["sig"].as_str().unwrap()).unwrap();
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key.public_key().as_ref()).verify(msg.as_bytes(), &sig).unwrap();
        assert!(signed_body(&key, ip, None).get("csr").is_none());
    }
}
