//! The macOS side of sysmon (sysmon.rs), its `os` module. Everything else there is shared.
//! CPU from the kernel's tick counters (host_statistics), memory counted as Activity Monitor's "Memory Used" (app, wired and
//! compressed pages, from vm_stat). shortcut: no GPU, disk or network figures on macOS yet; upgrade with IOKit and
//! getifaddrs counters.
#![allow(unused_imports)]
use super::*;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CpuLoad { ticks: [u32; 4] } // user, system, idle, nice
extern "C" {
    fn mach_host_self() -> u32;
    fn host_statistics(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
}
const HOST_CPU_LOAD_INFO: i32 = 3;
static LAST: Mutex<Option<CpuLoad>> = Mutex::new(None);

fn cpu_ticks() -> Option<CpuLoad> {
    let mut load = CpuLoad::default();
    let mut count = 4u32;
    (unsafe { host_statistics(mach_host_self(), HOST_CPU_LOAD_INFO, &mut load as *mut CpuLoad as *mut i32, &mut count) } == 0).then_some(load)
}
/// Busy share of the ticks between two readings, in percent (pure; tested).
pub(super) fn busy(before: [u32; 4], now: [u32; 4]) -> Option<f64> {
    let d: Vec<f64> = (0..4).map(|i| now[i].wrapping_sub(before[i]) as f64).collect();
    let total: f64 = d.iter().sum();
    (total > 0.0).then(|| (total - d[2]) / total * 100.0)
}
/// Activity Monitor's "Memory Used" in bytes from `vm_stat` output: anonymous (app) + wired + compressor pages (pure; tested).
pub(super) fn used_from_vm_stat(text: &str) -> Option<u64> {
    let page = text.lines().next()?.split("page size of ").nth(1)?.split_whitespace().next()?.parse::<u64>().ok()?;
    let count = |name: &str| text.lines().find(|l| l.starts_with(name)).and_then(|l| l.rsplit(':').next()).and_then(|v| v.trim().trim_end_matches('.').parse::<u64>().ok());
    let app = count("Anonymous pages")?.saturating_sub(count("Pages purgeable").unwrap_or(0));
    Some((app + count("Pages wired down")? + count("Pages occupied by compressor").unwrap_or(0)) * page)
}
fn total_memory() -> u64 {
    std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}
pub(super) fn read() -> Stats {
    let mut s = Stats::default();
    s.mem_total = { static TOTAL: std::sync::OnceLock<u64> = std::sync::OnceLock::new(); *TOTAL.get_or_init(total_memory) };
    s.mem_used = std::process::Command::new("vm_stat").output().ok().and_then(|o| used_from_vm_stat(&String::from_utf8_lossy(&o.stdout))).unwrap_or(0).min(s.mem_total);
    if let Some(now) = cpu_ticks() {
        let mut last = LAST.lock().ok();
        if let Some(before) = last.as_ref().and_then(|l| **l) { s.cpu = busy(before.ticks, now.ticks).map(|v| v.clamp(0.0, 100.0)); }
        if let Some(l) = last.as_mut() { **l = Some(now); }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_and_memory() {
        assert_eq!(super::busy([10, 10, 80, 0], [20, 20, 140, 0]), Some(25.0));
        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               10.\nPages wired down:                        100.\nAnonymous pages:                         300.\nPages purgeable:                          50.\nPages occupied by compressor:             20.\n";
        assert_eq!(super::used_from_vm_stat(vm), Some((250 + 100 + 20) * 16384));
    }
}
