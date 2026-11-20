//! The macOS side of quick_share (quick_share.rs), its `os` module. Everything else there is shared.
//! Phones are found with Bonjour through macOS's own `dns-sd` tool: browse the Quick Share service, then look up each
//! instance's host, port and "n" TXT record. shortcut: no Bluetooth nudge on macOS yet, so the phone's Quick Share screen
//! must be open (it then advertises by itself); upgrade with a CoreBluetooth advertisement like the Windows one.
#![allow(unused_imports)]
use super::*;
use std::{io::BufRead, process::{Child, Command, Stdio}, sync::mpsc};

/// Runs `dns-sd` with `args`, sending each output line until `wait` is over; the tool never ends by itself.
fn dns_sd(args: &[&str], wait: Duration, mut line: impl FnMut(&str) -> bool) {
    let Ok(mut child) = Command::new("dns-sd").args(args).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null()).spawn() else { return };
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || { for l in std::io::BufReader::new(out).lines().map_while(Result::ok) { if tx.send(l).is_err() { break; } } });
    }
    let end = Instant::now() + wait;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left.min(Duration::from_millis(100))) { Ok(l) => if !line(&l) { break }, Err(mpsc::RecvTimeoutError::Timeout) => {}, Err(_) => break }
    }
    stop(child);
}
fn stop(mut c: Child) { let _ = c.kill(); let _ = c.wait(); }

/// The instance name of an "Add" line of `dns-sd -B` (pure; tested).
pub(super) fn browsed(line: &str) -> Option<String> {
    let f: Vec<&str> = line.split_whitespace().collect();
    (f.len() >= 7 && f[1] == "Add" && f[5].starts_with("_FC9F5ED42C8A")).then(|| f[6..].join(" "))
}
/// "host:port" of a `dns-sd -L` "can be reached at" line, and the "n" value of its TXT line (pure; tested).
pub(super) fn reached(line: &str) -> Option<(String, u16)> {
    let at = line.split(" can be reached at ").nth(1)?.split_whitespace().next()?;
    let (host, port) = at.rsplit_once(':')?;
    Some((host.trim_end_matches('.').to_string(), port.parse().ok()?))
}
pub(super) fn txt_n(line: &str) -> Option<String> { line.split_whitespace().find_map(|kv| kv.strip_prefix("n=")).map(|n| n.replace("\\", "")) }

pub fn discover_with(wait: Duration, mut seen: impl FnMut(&str)) -> AppResult<Vec<Found>> {
    if Command::new("dns-sd").arg("-h").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err() { return Err("macOS network discovery is unavailable.".into()); }
    let service = SERVICE.trim_end_matches(".local.");
    let mut instances: Vec<String> = Vec::new();
    // Most of the wait browses; each phone found then gets a short lookup.
    dns_sd(&["-B", service, "local."], wait.mul_f32(0.6), |l| { if let Some(i) = browsed(l) { seen(&format!("PTR {i}")); if !instances.contains(&i) { instances.push(i); } } true });
    let mut found: HashMap<String, Found> = HashMap::new();
    for instance in instances {
        let (mut target, mut n) = (None, None);
        dns_sd(&["-L", &instance, service, "local."], Duration::from_millis(2500), |l| {
            if let Some(t) = reached(l) { seen(&format!("SRV {instance} -> {}:{}", t.0, t.1)); target = Some(t); }
            if let Some(v) = txt_n(l) { seen(&format!("TXT {instance} n={v}")); n = Some(v); }
            !(target.is_some() && n.is_some())
        });
        let (Some((host, port)), Some(n)) = (target, n) else { continue };
        let Some((endpoint, name, kind)) = parse_advert(&instance, &n) else { continue };
        // The system resolver answers .local names over mDNS; the phone listens on IPv4.
        let Some(addr) = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)).ok().and_then(|mut a| a.find(|a| a.is_ipv4())) else { continue };
        seen(&format!("A {host} = {}", addr.ip()));
        found.insert(addr.ip().to_string(), Found { endpoint, name, host: host_name(&host), addr, kind });
    }
    Ok(found.into_values().collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_dns_sd() {
        assert_eq!(super::browsed("14:00:00.123  Add        2   4 local.               _FC9F5ED42C8A._tcp.  IwAAAAD8n14AAA").as_deref(), Some("IwAAAAD8n14AAA"));
        assert!(super::browsed("Timestamp     A/R    Flags  if Domain               Service Type         Instance Name").is_none());
        assert_eq!(super::reached("14:00:00.200  IwA._FC9F5ED42C8A._tcp.local. can be reached at Pixel-8.local.:47653 (interface 4)"), Some(("Pixel-8.local".into(), 47653)));
        assert_eq!(super::txt_n(" n=BBBBQUJD"), Some("BBBBQUJD".into()));
    }
}
