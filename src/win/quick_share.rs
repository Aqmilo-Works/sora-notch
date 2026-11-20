//! The Windows side of quick_share (quick_share.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

/// Phones only advertise over mDNS once something nearby asks over Bluetooth: this BLE advertisement (service 0xFE2C with the
/// Nearby Sharing service data) is that nudge. Best effort: without Bluetooth the phone's Quick Share screen must be open.
pub(super) struct Nudge(pub(super) Option<windows::Devices::Bluetooth::Advertisement::BluetoothLEAdvertisementPublisher>);
impl Nudge {
    pub(super) fn start() -> Self {
        Self((|| -> windows::core::Result<_> {
            use windows::{Devices::Bluetooth::Advertisement::*, Storage::Streams::DataWriter};
            let mut data = vec![0x2c, 0xfe, 0xfc, 0x12, 0x8e, 0x01, 0x42, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let mut salt = [0u8; 10];
            let _ = SystemRandom::new().fill(&mut salt);
            data.extend_from_slice(&salt);
            let writer = DataWriter::new()?;
            writer.WriteBytes(&data)?;
            let section = BluetoothLEAdvertisementDataSection::Create(0x16, &writer.DetachBuffer()?)?;
            let publisher = BluetoothLEAdvertisementPublisher::new()?;
            publisher.Advertisement()?.DataSections()?.Append(&section)?;
            publisher.Start()?;
            Ok(publisher)
        })().ok())
    }
}
impl Drop for Nudge { fn drop(&mut self) { if let Some(p) = &self.0 { let _ = p.Stop(); } } }
/// `seen` gets every record the network answered with (diagnostics).
pub fn discover_with(wait: Duration, mut seen: impl FnMut(&str)) -> AppResult<Vec<Found>> {
    use std::{collections::HashSet, ffi::c_void, net::Ipv4Addr, sync::mpsc};
    use windows::{core::{PCWSTR, PWSTR}, Win32::NetworkManagement::Dns::*};
    /// What an answer told us: an instance (PTR), where it listens (SRV), its TXT "n", a host's address (A).
    enum Rec { Ptr(String), Srv(String, String, u16), Txt(String, String), A(String, Ipv4Addr) }
    unsafe fn wide(p: PWSTR) -> String { if p.is_null() { String::new() } else { p.to_string().unwrap_or_default() } }
    // Case is kept: the instance label is base64.
    fn key(name: &str) -> String { name.trim_end_matches('.').to_string() }
    unsafe extern "system" fn browsed(_: u32, ctx: *const c_void, records: *const DNS_RECORDW) {
        if records.is_null() { return; }
        let tx = &*(ctx as *const mpsc::Sender<Rec>);
        let mut r = records;
        while !r.is_null() {
            let (name, d) = (wide((*r).pName), &(*r).Data);
            let _ = tx.send(match (*r).wType {
                12 => Rec::Ptr(wide(d.PTR.pNameHost)),
                33 => Rec::Srv(name, wide(d.SRV.pNameTarget), d.SRV.wPort),
                16 => { let t = &d.TXT; let list = std::slice::from_raw_parts(t.pStringArray.as_ptr(), t.dwStringCount as usize);
                        match list.iter().map(|p| wide(*p)).find_map(|kv| kv.strip_prefix("n=").map(str::to_owned)) { Some(n) => Rec::Txt(name, n), None => { r = (*r).pNext; continue } } }
                1 => Rec::A(name, Ipv4Addr::from(d.A.IpAddress.to_ne_bytes())),
                _ => { r = (*r).pNext; continue }
            });
            r = (*r).pNext;
        }
        DnsFree(Some(records as *const c_void), DnsFreeRecordList);
    }
    unsafe extern "system" fn resolved(_: u32, ctx: *const c_void, inst: *const DNS_SERVICE_INSTANCE) {
        if inst.is_null() { return; }
        let tx = &*(ctx as *const mpsc::Sender<Rec>);
        let i = &*inst;
        let name = wide(i.pszInstanceName);
        for k in 0..i.dwPropertyCount as usize { if wide(*i.keys.add(k)) == "n" { let _ = tx.send(Rec::Txt(name.clone(), wide(*i.values.add(k)))); } }
        if !i.ip4Address.is_null() { let host = format!("{name}#host"); let _ = tx.send(Rec::Srv(name.clone(), host.clone(), i.wPort)); let _ = tx.send(Rec::A(host, Ipv4Addr::from((*i.ip4Address).to_ne_bytes()))); }
        DnsServiceFreeInstance(inst);
    }
    const PENDING: i32 = 9506; // DNS_REQUEST_PENDING
    let _nudge = Nudge::start();
    let (tx, rx) = mpsc::channel::<Rec>();
    // shortcut: the callback context is leaked (a few bytes per scan): Windows may still call back after a cancel returns.
    let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
    let query: Vec<u16> = SERVICE.trim_end_matches('.').encode_utf16().chain([0]).collect();
    let request = DNS_SERVICE_BROWSE_REQUEST { Version: DNS_QUERY_REQUEST_VERSION1.0, InterfaceIndex: 0, QueryName: PCWSTR(query.as_ptr()), Anonymous: DNS_SERVICE_BROWSE_REQUEST_0 { pBrowseCallback: Some(browsed) }, pQueryContext: ctx };
    let mut browse = DNS_SERVICE_CANCEL::default();
    if unsafe { DnsServiceBrowse(&request, &mut browse) } != PENDING { return Err("Windows network discovery is unavailable.".into()); }
    let end = Instant::now() + wait;
    let (mut asked, mut pending, mut buffers) = (HashSet::new(), Vec::new(), Vec::new());
    let (mut srv, mut txt, mut hosts) = (HashMap::<String, (String, u16)>::new(), HashMap::<String, String>::new(), HashMap::<String, Ipv4Addr>::new());
    let mut found = HashMap::<String, Found>::new();
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let Ok(rec) = rx.recv_timeout(left.min(Duration::from_millis(60))) else { continue };
        match rec {
            Rec::Ptr(instance) => {
                seen(&format!("PTR {instance}"));
                if asked.insert(key(&instance)) {
                    // Ask for SRV, TXT and the address too, in case the answer did not carry them.
                    let mut name: Vec<u16> = instance.encode_utf16().chain([0]).collect();
                    let request = DNS_SERVICE_RESOLVE_REQUEST { Version: DNS_QUERY_REQUEST_VERSION1.0, InterfaceIndex: 0, QueryName: PWSTR(name.as_mut_ptr()), pResolveCompletionCallback: Some(resolved), pQueryContext: ctx };
                    let mut cancel = DNS_SERVICE_CANCEL::default();
                    let status = unsafe { DnsServiceResolve(&request, &mut cancel) };
                    if status == PENDING { pending.push(cancel); } else { seen(&format!("resolve {instance}: status {status}")); }
                    buffers.push(name);
                }
            }
            Rec::Srv(instance, target, port) => { seen(&format!("SRV {instance} -> {target}:{port}")); srv.insert(key(&instance), (key(&target), port)); }
            Rec::Txt(instance, n) => { seen(&format!("TXT {instance} n={n}")); txt.insert(key(&instance), n); }
            Rec::A(host, ip) => { seen(&format!("A {host} = {ip}")); hosts.insert(key(&host), ip); }
        }
        for (instance, (target, port)) in &srv {
            let (Some(n), Some(ip)) = (txt.get(instance), hosts.get(target)) else { continue };
            let Some((endpoint, name, kind)) = parse_advert(instance.split('.').next().unwrap_or_default(), n) else { continue };
            let addr = SocketAddr::from((*ip, *port));
            // A name the device did not give is not made up from its host name: it stays "Android phone" and the host name rides along as a hint.
            let host = host_name(target);
            found.insert(addr.ip().to_string(), Found { endpoint, name, host, addr, kind });
        }
    }
    unsafe {
        let _ = DnsServiceBrowseCancel(&browse);
        for c in &pending { let _ = DnsServiceResolveCancel(c); }
    }
    drop(buffers);
    Ok(found.into_values().collect())
}
