use crate::adaptive::{Frame, Mode, Ring};
use crate::detect::Detector;
use collector::AppSample;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use store::{BootInfo, DriveInfo, FindingInput, StartupItem, Store};

const FLUSH_EVERY: Duration = Duration::from_secs(10);
const MAINTAIN_EVERY: Duration = Duration::from_secs(300);
const DRIVE_EVERY: Duration = Duration::from_secs(600);
const STARTUP_EVERY: Duration = Duration::from_secs(3600);
const RING_SECS: u64 = 30;
const SNAPSHOT_APPS: usize = 10;

/// Sleep that wakes early when `stop` is set.
fn nap(d: Duration, stop: &AtomicBool) {
    let end = Instant::now() + d;
    while !stop.load(Ordering::Relaxed) && Instant::now() < end {
        thread::sleep(Duration::from_millis(100));
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn log(msg: impl AsRef<str>) {
    println!("{}", msg.as_ref());
}

fn sync_startup(store: &mut Store, now: u64) {
    let entries = collector::startup_entries();
    let items: Vec<StartupItem> = entries
        .iter()
        .map(|e| StartupItem {
            source: &e.source,
            location: &e.location,
            name: &e.name,
            command: e.command.clone(),
            enabled: e.enabled,
        })
        .collect();
    if let Err(e) = store.sync_startup(&items, now) {
        eprintln!("Startup sync failed: {e}");
    }
}

fn record_boot(store: &mut Store) {
    let b = collector::boot_info();
    let r = store.record_boot(&BootInfo {
        boot_ts: b.boot_ts,
        duration_ms: b.duration_ms,
        main_path_ms: b.main_path_ms,
        post_boot_ms: b.post_boot_ms,
    });
    if let Err(e) = r {
        eprintln!("Boot record failed: {e}");
    }
}

fn record_drives(store: &mut Store, now: u64) {
    for d in collector::drives() {
        let r = store.insert_drive_health(
            &DriveInfo {
                model: d.model,
                serial_hash: d.serial_hash,
                kind: d.kind,
                capacity_bytes: d.capacity_bytes,
                temp_c: d.temp_c,
                percent_used: d.percent_used,
                host_writes_bytes: d.host_writes_bytes,
                host_reads_bytes: d.host_reads_bytes,
                available_spare: d.available_spare,
                media_errors: d.media_errors,
                power_on_hours: d.power_on_hours,
            },
            now,
        );
        if let Err(e) = r {
            eprintln!("Drive health save failed: {e}");
        }
    }
}

/// Runs until `stop` is set; flushes buffered samples before returning.
pub fn run(stop: Arc<AtomicBool>, db: &Path) -> Result<(), String> {
    let mut sampler = collector::Sampler::new().map_err(|e| format!("sampler: {e}"))?;
    let mut procs =
        collector::ProcessSampler::new().map_err(|e| format!("process sampler: {e}"))?;
    let mut store = Store::open(db).map_err(|e| format!("database {}: {e}", db.display()))?;

    // let the UI show totals and collector privileges without touching Windows APIs itself
    let _ = store.set_setting(
        "mem_total",
        &collector::memory()
            .map(|m| m.total_bytes)
            .unwrap_or(0)
            .to_string(),
    );
    let _ = store.set_setting("elevated", if collector::is_elevated() { "1" } else { "0" });

    // optional sources: carry on without them
    let etw = match collector::Etw::start() {
        Ok(e) => Some(e),
        Err(e) => {
            log(format!(
                "ETW off (run elevated or as a service for per-app disk/network): {e}"
            ));
            None
        }
    };
    let mut gpu = match collector::GpuSampler::new() {
        Ok(g) => Some(g),
        Err(e) => {
            log(format!("GPU counters unavailable: {e}"));
            None
        }
    };

    let mut sys_buf = Vec::new();
    let mut app_buf: Vec<(u64, Vec<AppSample>)> = Vec::new();
    let mut ring = Ring::new(RING_SECS);
    let mut detector = Detector::new();
    let mut mode = Mode::Normal;

    let start = now_secs();
    sync_startup(&mut store, start);
    record_boot(&mut store);
    record_drives(&mut store, start);
    if let Err(e) = store.maintain(start) {
        eprintln!("Maintenance failed: {e}");
    }

    let (mut last_flush, mut last_maint, mut last_drive, mut last_startup) = (
        Instant::now(),
        Instant::now(),
        Instant::now(),
        Instant::now(),
    );

    let flush = |store: &mut Store,
                 sys: &mut Vec<collector::Sample>,
                 apps: &mut Vec<(u64, Vec<AppSample>)>| {
        if sys.is_empty() {
            return;
        }
        let rows: usize = apps.iter().map(|(_, a)| a.len()).sum();
        if let Err(e) = store.insert_samples(sys) {
            eprintln!("Saving system samples failed: {e}");
        }
        if let Err(e) = store.insert_apps(apps) {
            eprintln!("Saving app samples failed: {e}");
        }
        log(format!("Flushed {} samples, {rows} app rows", sys.len()));
        sys.clear();
        apps.clear();
    };

    while !stop.load(Ordering::Relaxed) {
        nap(mode.interval(), &stop);
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let mut sample = match sampler.sample() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Sample failed: {e}");
                continue;
            }
        };
        let mut plist = match procs.sample() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Process sample failed: {e}");
                Vec::new()
            }
        };

        if let Some(etw) = &etw {
            let io = etw.drain();
            for c in io.values() {
                sample.disk_read_bytes += c.disk_read;
                sample.disk_write_bytes += c.disk_write;
                sample.net_in_bytes += c.net_in;
                sample.net_out_bytes += c.net_out;
            }
            for p in &mut plist {
                // ETW disk numbers are physical I/O and replace the NtQuery all-I/O counters
                let c = io.get(&p.pid).copied().unwrap_or_default();
                p.read_bytes = c.disk_read;
                p.write_bytes = c.disk_write;
                p.net_in_bytes = c.net_in;
                p.net_out_bytes = c.net_out;
            }
        }
        if let Some(g) = &mut gpu {
            let s = g.sample();
            sample.gpu_percent = s.total_percent;
            sample.vram_used_bytes = s.vram_used_bytes;
            for p in &mut plist {
                p.gpu_percent = s.per_pid.get(&p.pid).copied().unwrap_or(0.0);
            }
        }

        let ts = sample.timestamp;
        mode = mode.next(sample.cpu_percent);
        let apps = collector::aggregate(plist);
        ring.push(Frame {
            sample: sample.clone(),
            apps: apps.clone(),
        });
        sys_buf.push(sample);
        app_buf.push((ts, apps));

        for f in detector.run(&ring) {
            let Some(latest) = ring.latest() else {
                continue;
            };
            let mut procs: Vec<AppSample> = latest.apps.clone();
            procs.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
            procs.truncate(SNAPSHOT_APPS);
            // keep the blamed app even when it is not a top-CPU one
            if let Some(c) = &f.culprit {
                if !procs.iter().any(|p| &p.name == c) {
                    procs.extend(latest.apps.iter().find(|p| &p.name == c).cloned());
                }
            }
            let s = &latest.sample;
            let sys_json = json!({
                "mode": mode.name(),
                "cpu_percent": s.cpu_percent,
                "mem_used_bytes": s.mem_used_bytes,
                "mem_total_bytes": s.mem_total_bytes,
                "gpu_percent": s.gpu_percent,
            })
            .to_string();
            let detail = json!({ "summary": f.detail }).to_string();
            let r = store.insert_finding(&FindingInput {
                ts,
                kind: f.kind,
                severity: f.severity,
                metric: f.metric,
                value: f.value,
                threshold: f.threshold,
                pre_window_sec: ring.span_secs() as i64,
                sys_json: &sys_json,
                detail_json: &detail,
                procs: &procs,
                culprit: f.culprit.as_deref(),
            });
            match r {
                Ok(_) => log(format!("FINDING {}: {}", f.kind, f.detail)),
                Err(e) => eprintln!("Saving finding failed: {e}"),
            }
        }

        if last_flush.elapsed() >= FLUSH_EVERY {
            flush(&mut store, &mut sys_buf, &mut app_buf);
            last_flush = Instant::now();
        }
        if last_maint.elapsed() >= MAINTAIN_EVERY {
            if let Err(e) = store.maintain(now_secs()) {
                eprintln!("Maintenance failed: {e}");
            }
            // event 100 lands a few minutes after boot; keep retrying for the first hour
            if now_secs().saturating_sub(start) < 3600 {
                record_boot(&mut store);
            }
            last_maint = Instant::now();
        }
        if last_drive.elapsed() >= DRIVE_EVERY {
            record_drives(&mut store, now_secs());
            last_drive = Instant::now();
        }
        if last_startup.elapsed() >= STARTUP_EVERY {
            sync_startup(&mut store, now_secs());
            last_startup = Instant::now();
        }
    }

    flush(&mut store, &mut sys_buf, &mut app_buf);
    Ok(())
}
