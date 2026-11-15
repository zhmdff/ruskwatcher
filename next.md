# TaskWatch: What's Next

## Where you are

| Done    | What works                                         |
| ------- | -------------------------------------------------- |
| Step 1  | Workspace, CPU count, RAM                          |
| Step 2  | `Sampler` with CPU % and a timed loop              |
| Step 3  | SQLite `store`, batched writes to `sys_sample`     |
| Step 4a | `ProcessSampler` reading all processes in one call |

## Step 4b: Save per-process data

### Problem

Process samples are only printed. Two issues must be solved before saving them:

- **Many instances share one name** (`chrome.exe` can have 30). `proc_sample` has one row per process per timestamp, so instances are summed per name.
- **Storing every process every 2 s is too much data** (tens of millions of rows per week). Only active processes are kept.

### 4b.1 Add to the end of `crates/collector/src/process.rs`

```rust
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
        .filter(|a| a.cpu_percent >= MIN_CPU_PERCENT || a.read_bytes + a.write_bytes >= MIN_IO_BYTES)
        .collect()
}
```

### 4b.2 Change the re-export line in `crates/collector/src/lib.rs`

```rust
pub use process::{aggregate, AppSample, ProcessSample, ProcessSampler};
```

### 4b.3 Edit `crates/store/src/lib.rs`

Change the first line to:

```rust
use collector::{AppSample, Sample};
```

Add this method inside `impl Store`, after `insert_samples`:

```rust
    pub fn insert_apps(&mut self, batches: &[(u64, Vec<AppSample>)]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut upsert = tx.prepare_cached(
                "INSERT INTO process (exe_path, name, first_seen, last_seen)
                 VALUES (?1, ?1, ?2, ?2)
                 ON CONFLICT(exe_path) DO UPDATE SET last_seen = excluded.last_seen
                 RETURNING id",
            )?;
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO proc_sample
                 (tier, ts, process_id, cpu_avg, cpu_max, mem_private_max, mem_working_max,
                  handles_max, disk_read_bytes, disk_write_bytes)
                 VALUES (0, ?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (ts, apps) in batches {
                for a in apps {
                    let id: i64 = upsert.query_row((&a.name, *ts as i64), |row| row.get(0))?;
                    insert.execute((
                        *ts as i64,
                        id,
                        a.cpu_percent,
                        a.private_bytes as i64,
                        a.working_set_bytes as i64,
                        a.handles as i64,
                        a.read_bytes as i64,
                        a.write_bytes as i64,
                    ))?;
                }
            }
        }
        tx.commit()
    }
```

### 4b.4 Replace `crates/service/src/main.rs`

```rust
use collector::AppSample;
use std::{path::Path, thread, time::Duration};

const FLUSH_EVERY: usize = 5;

fn main() {
    let mut sampler = match collector::Sampler::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start sampler: {e}");
            return;
        }
    };

    let mut proc_sampler = match collector::ProcessSampler::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start process sampler: {e}");
            return;
        }
    };

    let mut store = match store::Store::open(Path::new("taskwatch.db")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            return;
        }
    };

    let mut sys_buffer = Vec::new();
    let mut app_buffer: Vec<(u64, Vec<AppSample>)> = Vec::new();

    loop {
        thread::sleep(Duration::from_secs(2));

        let sample = match sampler.sample() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Sample failed: {e}");
                continue;
            }
        };
        let ts = sample.timestamp;
        sys_buffer.push(sample);

        match proc_sampler.sample() {
            Ok(procs) => app_buffer.push((ts, collector::aggregate(procs))),
            Err(e) => eprintln!("Process sample failed: {e}"),
        }

        if sys_buffer.len() >= FLUSH_EVERY {
            let rows: usize = app_buffer.iter().map(|(_, apps)| apps.len()).sum();
            if let Err(e) = store.insert_samples(&sys_buffer) {
                eprintln!("Saving system samples failed: {e}");
            }
            if let Err(e) = store.insert_apps(&app_buffer) {
                eprintln!("Saving app samples failed: {e}");
            }
            println!("Flushed {} samples, {} app rows", sys_buffer.len(), rows);
            sys_buffer.clear();
            app_buffer.clear();
        }
    }
}
```

### 4b.5 Run and verify

```
cargo run -p service
```

Let it flush a few times, stop with Ctrl+C, then:

```
sqlite3 taskwatch.db "SELECT p.name, ROUND(s.cpu_avg,1) AS cpu, s.disk_write_bytes FROM proc_sample s JOIN process p ON p.id = s.process_id ORDER BY s.ts DESC, s.cpu_avg DESC LIMIT 10;"
```

### 4b.6 Measure the service's own cost

```
cargo run --release -p service
```

In Task Manager, find `service.exe` (Details tab) and watch it for a minute. Targets: **under 0.1% CPU and under 15 MB RAM**. Note the numbers; they are your baseline for every later step.

### Concepts in this step

- **`HashMap::entry().or_insert_with()`:** get the existing value for a key, or build it if missing. Used to sum instances of the same app.
- **`into_values().filter().collect()`:** consumes the map, keeps only matching items, and gathers them into a new `Vec`.
- **Tuple destructuring:** `for (ts, apps) in batches` unpacks each `(u64, Vec<AppSample>)` pair.
- **`continue`:** skips to the next loop iteration, so a failed sample doesn't stop the program.
- **`if let Err(e) = ...`:** handle only the error case when the success value isn't needed.
- **SQL upsert with `RETURNING`:** one statement inserts the process or updates `last_seen`, and gives back its `id`.

### Known limits (fixed in step 5)

- Identity is the process name, because `exe_path` temporarily holds the name. Two different programs named the same are merged.
- Read/write bytes include all I/O (files, network, pipes), not only disk. Accurate disk and network I/O come with ETW in step 8.

## Roadmap

| Step | Task                                                                                               | Rust concepts                        |
| ---- | -------------------------------------------------------------------------------------------------- | ------------------------------------ |
| 5    | Full exe path (`QueryFullProcessImageNameW`), signature check (`WinVerifyTrust`), process grouping | `unsafe` wrappers, `Option`, caching |
| 6    | Adaptive sampling: idle, normal, spike states; ring buffer of the last 30 s                        | `enum`, state machines, `VecDeque`   |
| 7    | Rollups to 1-minute and 1-hour tiers; retention deletes                                            | SQL aggregation, scheduled jobs      |
| 8    | ETW: accurate per-process disk and network I/O                                                     | threads, callbacks, channels         |
| 9    | GPU (PDH counters, AMD ADLX), NVMe health and temps, Intel CPU temps (optional module)             | FFI, feature flags                   |
| 10   | Detection rules and spike snapshots into `finding` and `snapshot`                                  | traits, iterators                    |
| 11   | Startup entries, diffs, boot time analysis                                                         | registry API, `serde`                |
| 12   | Run as a Windows service, installer, admin elevation                                               | `windows-service` crate              |
| 13   | Tauri UI: graphs, per-app page, spike click-through                                                | JS to Rust commands                  |
| 14   | v1.5: Event Log correlation, SSD lifespan projection                                               | `evtx`/Event Log API                 |

## Habits to keep

- Run `cargo fmt` and `cargo clippy` after every step.
- Commit after every working step: `git add . && git commit -m "step 4b"`.
- Re-measure the service's own CPU and RAM after every step.
