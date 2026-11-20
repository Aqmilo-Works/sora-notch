//! A small task manager for Home's System widget: CPU, GPU, memory, disk activity and network speed, with Windows' own
//! performance counters (PDH, the source Task Manager reads) and the memory status, counted the way Task Manager counts
//! them. Counters are read in English, so they work in every Windows language. Nothing is kept or sent anywhere.
//! Cost: opening the query takes ~150 ms once; each reading after that is 1-3 ms, and the page asks only while Home is open.
#![cfg_attr(not(win), allow(dead_code, unused_imports))]
use std::sync::Mutex;
use serde::Serialize;
// What differs per OS lives in win/sysmon.rs and mac/sysmon.rs (picked by cfg(win) from build.rs).
#[cfg(win)]
#[path = "win/sysmon.rs"]
mod os;
#[cfg(not(win))]
#[path = "mac/sysmon.rs"]
mod os;
#[allow(unused_imports)]
pub(crate) use os::*;


/// cpu_ghz: the CPU's clock now. gpu_temp: °C of the hottest GPU that reports one (Task Manager's source, WDDM 2.9+
/// drivers; integrated GPUs usually report none).
#[derive(Serialize, Default, Debug, PartialEq)]
pub struct Stats { cpu: Option<f64>, cpu_ghz: Option<f64>, gpu: Option<f64>, gpu_temp: Option<f64>, disk: Option<f64>, down: Option<f64>, up: Option<f64>, mem_used: u64, mem_total: u64 }

/// GHz from the base clock in MHz and the performance percentage.
fn ghz(base_mhz: Option<f64>, performance: Option<f64>) -> Option<f64> {
    let (b, p) = (base_mhz?, performance?);
    (b > 0.0 && p > 0.0).then(|| b * p / 100.0 / 1000.0)
}


/// Task Manager's GPU figure: each engine's use summed over the processes using it, then the busiest engine of any GPU.
/// Instances look like "pid_1234_luid_0x0_0xD1A5_phys_0_eng_3_engtype_3D".
fn gpu_busy(items: &[(String, f64)]) -> Option<f64> {
    let mut engines: std::collections::HashMap<&str, f64> = Default::default();
    for (name, v) in items { if let Some(i) = name.find("luid_") { *engines.entry(&name[i..]).or_default() += v; } }
    engines.into_values().reduce(f64::max).or(if items.is_empty() { None } else { Some(0.0) })
}
/// Task Manager's disk "active time" for the busiest physical disk.
fn disk_busy(items: &[(String, f64)]) -> Option<f64> {
    items.iter().filter(|(n, _)| n != "_Total").map(|(_, idle)| 100.0 - idle).reduce(f64::max)
}
/// Bytes per second over the real network cards. Loopback, tunnels and virtual switches (Hyper-V, WSL, VPN adapters)
/// carry the same bytes a second time.
fn net_total(items: &[(String, f64)]) -> Option<f64> {
    const SKIP: [&str; 11] = ["loopback", "isatap", "teredo", "vethernet", "hyper-v", "virtual", "tap-", "wintun", "wireguard", "wan miniport", "kernel debug"];
    let real: Vec<f64> = items.iter().filter(|(n, _)| { let n = n.to_lowercase(); !SKIP.iter().any(|s| n.contains(s)) }).map(|x| x.1).collect();
    (!real.is_empty()).then(|| real.iter().sum())
}

/// One reading, off the main thread: a plain (sync) command runs on Tauri's main thread, and while PDH collects (151 ms the
/// first time, longer when the GPU is busy) the notch's frames wait there too, since it clips the panel's window each frame.
#[tauri::command]
pub async fn system_stats() -> Stats { tauri::async_runtime::spawn_blocking(read).await.unwrap_or_default() }

#[cfg(test)]
mod tests {
    use super::*;
    fn v(list: &[(&str, f64)]) -> Vec<(String, f64)> { list.iter().map(|(n, x)| (n.to_string(), *x)).collect() }

    #[test]
    fn gpu_is_the_busiest_engine_summed_over_processes() {
        let list = v(&[("pid_1_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 20.0), ("pid_2_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 15.0),
            ("pid_2_luid_0x0_0xA_phys_0_eng_5_engtype_VideoDecode", 50.0), ("pid_3_luid_0x0_0xB_phys_0_eng_0_engtype_3D", 30.0)]);
        assert_eq!(gpu_busy(&list), Some(50.0));
        assert_eq!(gpu_busy(&[]), None);
    }

    #[test]
    fn disk_is_the_busiest_disk_and_net_skips_virtual_cards() {
        assert_eq!(disk_busy(&v(&[("0 D:", 100.0), ("2 C:", 63.0), ("_Total", 87.0)])), Some(37.0));
        let net = v(&[("Realtek PCIe GBE Family Controller", 1000.0), ("vEthernet (WSL)", 900.0), ("Hyper-V Virtual Ethernet Adapter", 900.0), ("Software Loopback Interface 1", 50.0)]);
        assert_eq!(net_total(&net), Some(1000.0));
        assert_eq!(net_total(&v(&[("vEthernet (WSL)", 9.0)])), None);
    }

    #[test]
    fn speed_is_base_clock_times_performance() {
        assert_eq!(ghz(Some(3000.0), Some(140.0)), Some(4.2));
        assert_eq!(ghz(Some(3000.0), None), None);
        assert_eq!(ghz(Some(0.0), Some(100.0)), None);
    }

    #[test]
    #[ignore = "reads this PC's counters"]
    fn reads_this_pc() {
        let _ = read();
        std::thread::sleep(std::time::Duration::from_millis(600));
        let s = read();
        println!("{s:?}");
        assert!(s.mem_total > 0 && s.mem_used <= s.mem_total);
        assert!(s.cpu.is_some());
    }
}
