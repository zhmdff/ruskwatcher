use crate::cpu_count;
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Instant;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::E_FAIL;

const STATUS_INFO_LENGTH_MISMATCH: u32 = 0xC000_0004;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct RawProcess {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    threads_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name: UnicodeString,
    base_priority: i32,
    pid: usize,
    parent_pid: usize,
    handle_count: u32,
    session_id: u32,
    unique_process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set: usize,
    working_set: usize,
    quota_peak_paged: usize,
    quota_paged: usize,
    quota_peak_nonpaged: usize,
    quota_nonpaged: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
    read_ops: i64,
    write_ops: i64,
    other_ops: i64,
    read_bytes: i64,
    write_bytes: i64,
    other_bytes: i64,
}

struct RawEntry {
    pid: u32,
    name: String,
    create_time: i64,
    cpu_time: u64,
    private_bytes: u64,
    working_set_bytes: u64,
    handles: u32,
    read_bytes: u64,
    write_bytes: u64,
}

struct PrevCounters {
    cpu_time: u64,
    read_bytes: u64,
    write_bytes: u64,
}

pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub private_bytes: u64,
    pub working_set_bytes: u64,
    pub handles: u32,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

pub struct ProcessSampler {
    buf: Vec<u64>,
    prev: HashMap<(u32, i64), PrevCounters>,
    last: Instant,
    cpus: u64,
}

fn query_processes(buf: &mut Vec<u64>) -> Result<(), windows::core::Error> {
    for _ in 0..4 {
        let mut needed: u32 = 0;
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 8) as u32,
                &mut needed,
            )
        };
        if status.0 as u32 == STATUS_INFO_LENGTH_MISMATCH {
            buf.resize((needed as usize + 65536) / 8 + 1, 0);
            continue;
        }
        return status.ok();
    }
    Err(windows::core::Error::from(E_FAIL))
}

fn parse(buf: &[u64]) -> Vec<RawEntry> {
    let base = buf.as_ptr() as *const u8;
    let mut out = Vec::new();
    let mut offset = 0usize;
    loop {
        let raw = unsafe { std::ptr::read_unaligned(base.add(offset) as *const RawProcess) };
        let name = if raw.pid == 0 {
            "Idle".to_string()
        } else if raw.image_name.buffer.is_null() || raw.image_name.length == 0 {
            String::new()
        } else {
            let units = unsafe {
                std::slice::from_raw_parts(
                    raw.image_name.buffer,
                    raw.image_name.length as usize / 2,
                )
            };
            String::from_utf16_lossy(units)
        };
        out.push(RawEntry {
            pid: raw.pid as u32,
            name,
            create_time: raw.create_time,
            cpu_time: raw.user_time as u64 + raw.kernel_time as u64,
            private_bytes: raw.private_page_count as u64,
            working_set_bytes: raw.working_set as u64,
            handles: raw.handle_count,
            read_bytes: raw.read_bytes as u64,
            write_bytes: raw.write_bytes as u64,
        });
        if raw.next_entry_offset == 0 {
            break;
        }
        offset += raw.next_entry_offset as usize;
    }
    out
}

impl ProcessSampler {
    pub fn new() -> Result<Self, windows::core::Error> {
        let mut sampler = Self {
            buf: vec![0; 65536],
            prev: HashMap::new(),
            last: Instant::now(),
            cpus: cpu_count() as u64,
        };
        sampler.sample()?;
        Ok(sampler)
    }

    pub fn sample(&mut self) -> Result<Vec<ProcessSample>, windows::core::Error> {
        query_processes(&mut self.buf)?;
        let entries = parse(&self.buf);

        let now = Instant::now();
        let wall = (now.duration_since(self.last).as_nanos() / 100) as u64;
        self.last = now;
        let capacity = wall.saturating_mul(self.cpus).max(1);

        let mut next = HashMap::with_capacity(entries.len());
        let mut out = Vec::with_capacity(entries.len());

        for e in entries {
            let key = (e.pid, e.create_time);
            let (prev_cpu, prev_read, prev_write) = match self.prev.get(&key) {
                Some(p) => (p.cpu_time, p.read_bytes, p.write_bytes),
                None => (0, 0, 0),
            };

            if e.pid != 0 {
                out.push(ProcessSample {
                    pid: e.pid,
                    name: e.name,
                    cpu_percent: e.cpu_time.saturating_sub(prev_cpu) as f64 * 100.0
                        / capacity as f64,
                    private_bytes: e.private_bytes,
                    working_set_bytes: e.working_set_bytes,
                    handles: e.handles,
                    read_bytes: e.read_bytes.saturating_sub(prev_read),
                    write_bytes: e.write_bytes.saturating_sub(prev_write),
                });
            }

            next.insert(
                key,
                PrevCounters {
                    cpu_time: e.cpu_time,
                    read_bytes: e.read_bytes,
                    write_bytes: e.write_bytes,
                },
            );
        }

        self.prev = next;
        Ok(out)
    }
}
const MIN_CPU_PERCENT: f64 = 0.5;
const MIN_IO_BYTES: u64 = 256 * 1024;

pub struct AppSample {
    pub name: String,
    pub instances: u32,
    pub cpu_percent: f64,
    pub private_bytes: u64,
    pub working_set_bytes: u64,
    pub handles: u32,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

pub fn aggregate(procs: Vec<ProcessSample>) -> Vec<AppSample> {
    let mut apps: HashMap<String, AppSample> = HashMap::new();
    for p in procs {
        if p.name.is_empty() {
            continue;
        }
        let app = apps.entry(p.name.clone()).or_insert_with(|| AppSample {
            name: p.name.clone(),
            instances: 0,
            cpu_percent: 0.0,
            private_bytes: 0,
            working_set_bytes: 0,
            handles: 0,
            read_bytes: 0,
            write_bytes: 0,
        });
        app.instances += 1;
        app.cpu_percent += p.cpu_percent;
        app.private_bytes += p.private_bytes;
        app.working_set_bytes += p.working_set_bytes;
        app.handles += p.handles;
        app.read_bytes += p.read_bytes;
        app.write_bytes += p.write_bytes;
    }
    apps.into_values()
        .filter(|a| {
            a.cpu_percent >= MIN_CPU_PERCENT || a.read_bytes + a.write_bytes >= MIN_IO_BYTES
        })
        .collect()
}
