mod drive;
mod etw;
mod gpu;
mod image;
mod process;
mod startup;
pub use drive::{drives, DriveHealth};
pub use etw::{Etw, IoCounters};
pub use gpu::{GpuSample, GpuSampler};
pub use image::is_signed;
pub use process::{aggregate, AppSample, ProcessSample, ProcessSampler};
pub use startup::{boot_info, is_elevated, startup_entries, BootTimes, StartupEntry};

use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Clone)]
pub struct Sample {
    pub timestamp: u64,
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub gpu_percent: f64,
    pub vram_used_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub net_in_bytes: u64,
    pub net_out_bytes: u64,
}

struct CpuTimes {
    idle: u64,
    total: u64,
}

pub struct Sampler {
    prev: CpuTimes,
}

pub fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

pub fn memory() -> Result<MemoryInfo, windows::core::Error> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut status)? };
    Ok(MemoryInfo {
        total_bytes: status.ullTotalPhys,
        used_bytes: status.ullTotalPhys - status.ullAvailPhys,
    })
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn read_cpu_times() -> Result<CpuTimes, windows::core::Error> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user))? };
    Ok(CpuTimes {
        idle: filetime_to_u64(idle),
        total: filetime_to_u64(kernel) + filetime_to_u64(user),
    })
}

impl Sampler {
    pub fn new() -> Result<Self, windows::core::Error> {
        Ok(Self {
            prev: read_cpu_times()?,
        })
    }

    pub fn sample(&mut self) -> Result<Sample, windows::core::Error> {
        let now = read_cpu_times()?;
        let total_delta = now.total.saturating_sub(self.prev.total);
        let idle_delta = now.idle.saturating_sub(self.prev.idle);
        self.prev = now;

        let cpu_percent = if total_delta == 0 {
            0.0
        } else {
            (total_delta - idle_delta) as f64 * 100.0 / total_delta as f64
        };

        let mem = memory()?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(Sample {
            timestamp,
            cpu_percent,
            mem_used_bytes: mem.used_bytes,
            mem_total_bytes: mem.total_bytes,
            gpu_percent: 0.0,
            vram_used_bytes: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            net_in_bytes: 0,
            net_out_bytes: 0,
        })
    }
}
