//! The Windows side of sysmon (sysmon.rs), its `os` module. Everything else there is shared.
#![allow(unused_imports)]
use super::*;

use windows::{core::PCWSTR, Win32::System::{Performance::*, SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX}}};
/// The open query and its counters, in the order of PATHS.
pub(super) struct Query { query: isize, counters: [isize; 8], gpus: Vec<u32> }
unsafe impl Send for Query {}
pub(super) static QUERY: Mutex<Option<Query>> = Mutex::new(None);
pub(super) const PATHS: [&str; 8] = [
    r"\Processor Information(_Total)\% Processor Utility",
    // Every engine of every GPU (3D, copy, video decode/encode, compute), per process.
    r"\GPU Engine(*)\Utilization Percentage",
    r"\PhysicalDisk(*)\% Idle Time",
    r"\Network Interface(*)\Bytes Received/sec",
    r"\Network Interface(*)\Bytes Sent/sec",
    r"\Memory\Modified Page List Bytes",
    // Task Manager's CPU "Speed": the base clock (MHz) times the share of it the cores run at now (above 100 with turbo).
    r"\Processor Information(_Total)\Processor Frequency",
    r"\Processor Information(_Total)\% Processor Performance",
];
/// The display adapters, opened once (a handle each, kept for the life of the app).
pub(super) unsafe fn gpu_adapters() -> Vec<u32> {
    use windows::Wdk::Graphics::Direct3D::*;
    let mut e = D3DKMT_ENUMADAPTERS2::default();
    if D3DKMTEnumAdapters2(&mut e).is_err() || e.NumAdapters == 0 { return vec![]; }
    let mut list = vec![D3DKMT_ADAPTERINFO::default(); e.NumAdapters as usize];
    e.pAdapters = list.as_mut_ptr();
    if D3DKMTEnumAdapters2(&mut e).is_err() { return vec![]; }
    list.truncate(e.NumAdapters as usize);
    list.iter().map(|a| a.hAdapter).collect()
}
/// The hottest reported GPU temperature, from the kernel's adapter performance data (tenths of a degree).
pub(super) unsafe fn gpu_temp(adapters: &[u32]) -> Option<f64> {
    use windows::Wdk::Graphics::Direct3D::*;
    adapters.iter().filter_map(|&h| {
        let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
        let mut q = D3DKMT_QUERYADAPTERINFO { hAdapter: h, Type: KMTQAITYPE_ADAPTERPERFDATA, pPrivateDriverData: &mut perf as *mut _ as *mut _, PrivateDriverDataSize: std::mem::size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32 };
        (D3DKMTQueryAdapterInfo(&mut q).is_ok() && perf.Temperature > 0).then(|| perf.Temperature as f64 / 10.0)
    }).filter(|t| *t < 150.0).reduce(f64::max)
}
pub(super) fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }
pub(super) unsafe fn open() -> Option<Query> {
    let mut query = 0isize;
    if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 { return None; }
    let mut counters = [0isize; 8];
    for (c, path) in counters.iter_mut().zip(PATHS) {
        let p = wide(path);
        // A counter this PC does not have (no GPU counters on old drivers) stays empty; the others still work.
        let _ = PdhAddEnglishCounterW(query, PCWSTR(p.as_ptr()), 0, c);
    }
    let _ = PdhCollectQueryData(query);
    Some(Query { query, counters, gpus: gpu_adapters() })
}
/// Every instance of a counter with a valid value: (instance name, value).
pub(super) unsafe fn items(counter: isize) -> Vec<(String, f64)> {
    if counter == 0 { return vec![]; }
    let (mut size, mut count) = (0u32, 0u32);
    let _ = PdhGetFormattedCounterArrayW(counter, PDH_FMT(PDH_FMT_DOUBLE.0 | 0x8000), &mut size, &mut count, None);
    if size == 0 { return vec![]; }
    let mut buf = vec![0u8; size as usize + 64];
    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    if PdhGetFormattedCounterArrayW(counter, PDH_FMT(PDH_FMT_DOUBLE.0 | 0x8000), &mut size, &mut count, Some(items)) != 0 { return vec![]; }
    std::slice::from_raw_parts(items, count as usize).iter().filter(|it| it.FmtValue.CStatus == 0)
        .map(|it| (it.szName.to_string().unwrap_or_default(), it.FmtValue.Anonymous.doubleValue)).collect()
}
/// One reading. Rates (CPU, disk, network) are measured since the previous call, so the first one may be empty.
pub(super) fn read() -> Stats {
    let mut s = Stats::default();
    unsafe {
        let Ok(mut q) = QUERY.lock() else { return s };
        if q.is_none() { *q = open(); }
        let q = q.as_ref();
        let collected = q.is_some_and(|q| PdhCollectQueryData(q.query) == 0);
        let get = |i: usize| if collected { items(q.unwrap().counters[i]) } else { vec![] };
        let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if GlobalMemoryStatusEx(&mut m).is_ok() {
            // Task Manager's "In use" leaves out the modified pages too (written to disk soon, then free).
            let modified = get(5).first().map_or(0, |x| x.1.max(0.0) as u64);
            s.mem_total = m.ullTotalPhys;
            s.mem_used = m.ullTotalPhys.saturating_sub(m.ullAvailPhys).saturating_sub(modified);
        }
        if !collected { return s; }
        let pct = |v: Option<f64>| v.map(|v| v.clamp(0.0, 100.0));
        s.cpu = pct(get(0).first().map(|x| x.1));
        s.cpu_ghz = ghz(get(6).first().map(|x| x.1), get(7).first().map(|x| x.1));
        s.gpu_temp = gpu_temp(&q.unwrap().gpus);
        s.gpu = pct(gpu_busy(&get(1)));
        s.disk = pct(disk_busy(&get(2)));
        s.down = net_total(&get(3));
        s.up = net_total(&get(4));
    }
    s
}
