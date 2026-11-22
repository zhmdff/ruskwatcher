//! GPU utilisation (system + per process) and VRAM via PDH "GPU Engine" / "GPU Adapter Memory".
//! Vendor-neutral and works without admin. AMD ADLX / vendor temperatures are not covered.
use std::collections::HashMap;
use windows::core::{w, PCWSTR};
use windows::Win32::System::Performance::*;

pub struct GpuSample {
    pub total_percent: f64,
    pub vram_used_bytes: u64,
    pub per_pid: HashMap<u32, f64>,
}

pub struct GpuSampler {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    mem: PDH_HCOUNTER,
}

fn add(query: PDH_HQUERY, path: PCWSTR) -> Result<PDH_HCOUNTER, windows::core::Error> {
    let mut c = PDH_HCOUNTER::default();
    let r = unsafe { PdhAddEnglishCounterW(query, path, 0, &mut c) };
    if r != 0 {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            r as i32,
        )));
    }
    Ok(c)
}

fn read(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
    let (mut size, mut count) = (0u32, 0u32);
    let r = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
    };
    if r != PDH_MORE_DATA || size == 0 {
        return Vec::new();
    }
    // 8-byte aligned backing store; item name pointers point back into this buffer
    let mut buf = vec![0u64; size as usize / 8 + 1];
    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    let r = unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items))
    };
    if r != 0 {
        return Vec::new();
    }
    (0..count as usize)
        .map(|i| unsafe {
            let it = &*items.add(i);
            (
                it.szName.to_string().unwrap_or_default(),
                it.FmtValue.Anonymous.doubleValue,
            )
        })
        .collect()
}

impl GpuSampler {
    /// Errors when the machine has no GPU performance counters.
    pub fn new() -> Result<Self, windows::core::Error> {
        let mut query = PDH_HQUERY::default();
        let r = unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) };
        if r != 0 {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                r as i32,
            )));
        }
        let engine = add(query, w!("\\GPU Engine(*)\\Utilization Percentage"))?;
        let mem = add(query, w!("\\GPU Adapter Memory(*)\\Dedicated Usage"))?;
        unsafe { PdhCollectQueryData(query) }; // prime: rates need two points
        Ok(Self { query, engine, mem })
    }

    pub fn sample(&mut self) -> GpuSample {
        unsafe { PdhCollectQueryData(self.query) };
        // (pid, engine type) -> summed utilisation
        let mut by_engine: HashMap<(u32, String), f64> = HashMap::new();
        for (name, v) in read(self.engine) {
            let pid = name
                .strip_prefix("pid_")
                .and_then(|s| s.split('_').next())
                .and_then(|s| s.parse().ok());
            let ty = name.rsplit("engtype_").next().unwrap_or("").to_string();
            if let Some(pid) = pid {
                *by_engine.entry((pid, ty)).or_default() += v;
            }
        }
        let mut per_pid: HashMap<u32, f64> = HashMap::new();
        let mut by_type: HashMap<&str, f64> = HashMap::new();
        for ((pid, ty), v) in &by_engine {
            let e = per_pid.entry(*pid).or_default();
            *e = e.max(*v);
            *by_type.entry(ty).or_default() += v;
        }
        let total_percent = by_type.values().cloned().fold(0.0, f64::max).min(100.0);
        let vram_used_bytes = read(self.mem).iter().map(|(_, v)| *v as u64).sum();
        GpuSample {
            total_percent,
            vram_used_bytes,
            per_pid,
        }
    }
}

impl Drop for GpuSampler {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}
