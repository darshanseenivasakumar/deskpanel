//! System stats sampled at 1 Hz, and only while the panel is visible.
//! GPU numbers come from the Windows graphics kernel (D3DKMT), the same source Task Manager uses.
//! NVML / nvidia-smi was rejected: querying it wakes a sleeping Optimus dGPU and costs battery.
use std::time::Instant;

use windows::Wdk::Graphics::Direct3D::*;
use windows::Win32::Foundation::{FILETIME, LUID};
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::System::Power::*;
use windows::Win32::System::SystemInformation::*;
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Debug, Clone, PartialEq, serde::Serialize, Default)]
pub struct StatsInfo {
    pub cpu: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub gpu: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub down_bps: f64,
    pub up_bps: f64,
    pub battery: Option<u8>,
    pub charging: bool,
}

struct Gpu {
    luid: LUID,
    handle: u32,
    nodes: u32,
    last: Option<(Instant, Vec<i64>)>,
}

impl Gpu {
    /// The NVIDIA adapter (vendor 0x10DE); falls back to the first hardware adapter.
    fn open() -> Option<Gpu> {
        unsafe {
            let f: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let mut pick: Option<LUID> = None;
            let mut i = 0;
            while let Ok(a) = f.EnumAdapters1(i) {
                i += 1;
                let Ok(d) = a.GetDesc1() else { continue };
                if (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                    continue;
                }
                if d.VendorId == 0x10DE {
                    pick = Some(d.AdapterLuid);
                    break;
                }
                if pick.is_none() {
                    pick = Some(d.AdapterLuid);
                }
            }
            let luid = pick?;
            let mut o = D3DKMT_OPENADAPTERFROMLUID { AdapterLuid: luid, hAdapter: 0 };
            D3DKMTOpenAdapterFromLuid(&mut o).ok().ok()?;
            let mut q: D3DKMT_QUERYSTATISTICS = std::mem::zeroed();
            q.Type = D3DKMT_QUERYSTATISTICS_ADAPTER;
            q.AdapterLuid = luid;
            D3DKMTQueryStatistics(&q).ok().ok()?;
            let nodes = q.QueryResult.AdapterInformation.NodeCount;
            crate::log!("stats: gpu luid={:x}:{:x} nodes={nodes}", luid.HighPart, luid.LowPart);
            Some(Gpu { luid, handle: o.hAdapter, nodes, last: None })
        }
    }

    /// Busiest engine's share of wall time since the last sample (like Task Manager's GPU %).
    fn utilisation(&mut self) -> Option<f32> {
        let now = Instant::now();
        let mut times = Vec::with_capacity(self.nodes as usize);
        for n in 0..self.nodes {
            unsafe {
                let mut q: D3DKMT_QUERYSTATISTICS = std::mem::zeroed();
                q.Type = D3DKMT_QUERYSTATISTICS_NODE;
                q.AdapterLuid = self.luid;
                q.Anonymous.QueryNode.NodeId = n;
                if D3DKMTQueryStatistics(&q).ok().is_err() {
                    return None;
                }
                times.push(q.QueryResult.NodeInformation.GlobalInformation.RunningTime);
            }
        }
        let out = self.last.as_ref().map(|(t0, prev)| {
            let wall = now.duration_since(*t0).as_nanos() as f64 / 100.0; // 100 ns units
            let busiest = times.iter().zip(prev).map(|(a, b)| (a - b).max(0) as f64).fold(0.0, f64::max);
            ((busiest / wall) * 100.0).clamp(0.0, 100.0) as f32
        });
        self.last = Some((now, times));
        out
    }

    /// Driver-reported temperature; 0 (shown as "—") while the dGPU is asleep.
    fn temperature(&self) -> Option<f32> {
        unsafe {
            let mut pd: D3DKMT_ADAPTER_PERFDATA = std::mem::zeroed();
            let mut q = D3DKMT_QUERYADAPTERINFO {
                hAdapter: self.handle,
                Type: KMTQAITYPE_ADAPTERPERFDATA,
                pPrivateDriverData: &mut pd as *mut _ as *mut core::ffi::c_void,
                PrivateDriverDataSize: std::mem::size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
            };
            D3DKMTQueryAdapterInfo(&mut q).ok().ok()?;
            (pd.Temperature > 0).then(|| pd.Temperature as f32 / 10.0)
        }
    }
}

pub struct Stats {
    last_cpu: Option<(u64, u64)>,
    last_net: Option<(Instant, u64, u64)>,
    gpu: Option<Gpu>,
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

impl Stats {
    pub fn new() -> Stats {
        Stats { last_cpu: None, last_net: None, gpu: Gpu::open() }
    }

    fn cpu(&mut self) -> f32 {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
            return 0.0;
        }
        let (idle, total) = (ft(idle), ft(kernel) + ft(user)); // kernel time includes idle
        let pct = match self.last_cpu {
            Some((i0, t0)) if total > t0 => 100.0 * (1.0 - (idle - i0) as f64 / (total - t0) as f64),
            _ => 0.0,
        };
        self.last_cpu = Some((idle, total));
        pct.clamp(0.0, 100.0) as f32
    }

    fn net(&mut self) -> (f64, f64) {
        let (mut rx, mut tx) = (0u64, 0u64);
        unsafe {
            let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
            if GetIfTable2(&mut table).is_err() || table.is_null() {
                return (0.0, 0.0);
            }
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
            for r in rows {
                let hardware = (r.InterfaceAndOperStatusFlags._bitfield & 1) != 0;
                if hardware && r.OperStatus == windows::Win32::NetworkManagement::Ndis::IfOperStatusUp && r.Type != 24 {
                    rx += r.InOctets;
                    tx += r.OutOctets;
                }
            }
            FreeMibTable(table as *const _);
        }
        let now = Instant::now();
        let out = match self.last_net {
            Some((t0, r0, x0)) => {
                let s = now.duration_since(t0).as_secs_f64().max(0.001);
                ((rx.saturating_sub(r0)) as f64 / s, (tx.saturating_sub(x0)) as f64 / s)
            }
            None => (0.0, 0.0),
        };
        self.last_net = Some((now, rx, tx));
        out
    }

    pub fn sample(&mut self) -> StatsInfo {
        let cpu = self.cpu();
        let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        let _ = unsafe { GlobalMemoryStatusEx(&mut mem) };
        let gb = |b: u64| (b as f64 / (1u64 << 30) as f64) as f32;
        let mut ps = SYSTEM_POWER_STATUS::default();
        let _ = unsafe { GetSystemPowerStatus(&mut ps) };
        let has_battery = ps.BatteryFlag != 128 && ps.BatteryLifePercent != 255;
        let (down_bps, up_bps) = self.net();
        let (gpu, gpu_temp_c) = match self.gpu.as_mut() {
            Some(g) => (g.utilisation(), g.temperature()),
            None => (None, None),
        };
        StatsInfo {
            cpu,
            ram_used_gb: gb(mem.ullTotalPhys - mem.ullAvailPhys),
            ram_total_gb: gb(mem.ullTotalPhys),
            gpu,
            gpu_temp_c,
            down_bps,
            up_bps,
            battery: has_battery.then_some(ps.BatteryLifePercent),
            charging: ps.ACLineStatus == 1,
        }
    }
}
