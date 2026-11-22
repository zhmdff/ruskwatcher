use collector::{AppSample, Sample};
use rusqlite::Connection;
use std::path::Path;

mod query;

const SCHEMA: &str = include_str!("../schema.sql");
const DAY: i64 = 86_400;

pub struct Store {
    conn: Connection,
}

pub struct FindingInput<'a> {
    pub ts: u64,
    pub kind: &'a str,
    pub severity: i64,
    pub metric: &'a str,
    pub value: f64,
    pub threshold: f64,
    pub pre_window_sec: i64,
    pub sys_json: &'a str,
    pub detail_json: &'a str,
    pub procs: &'a [AppSample],
    /// name of the app blamed, if any
    pub culprit: Option<&'a str>,
}

pub struct StartupItem<'a> {
    pub source: &'a str,
    pub location: &'a str,
    pub name: &'a str,
    pub command: String,
    pub enabled: bool,
}

pub struct BootInfo {
    pub boot_ts: u64,
    pub duration_ms: Option<i64>,
    pub main_path_ms: Option<i64>,
    pub post_boot_ms: Option<i64>,
}

pub struct DriveInfo {
    pub model: String,
    pub serial_hash: String,
    pub kind: String,
    pub capacity_bytes: u64,
    pub temp_c: Option<f64>,
    pub percent_used: Option<f64>,
    pub host_writes_bytes: Option<u64>,
    pub host_reads_bytes: Option<u64>,
    pub available_spare: Option<f64>,
    pub media_errors: Option<u64>,
    pub power_on_hours: Option<u64>,
}

pub struct SysExtras {
    pub gpu_percent: f64,
    pub vram_used_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub net_in_bytes: u64,
    pub net_out_bytes: u64,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            conn.execute_batch(SCHEMA)?;
        }
        Ok(Self { conn })
    }

    /// Read-only handle for the UI; WAL lets it run beside the writer.
    pub fn open_readonly(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        Ok(Self { conn })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO setting (key, value) VALUES (?1, ?2)",
            (key, value),
        )?;
        Ok(())
    }

    pub fn insert_samples(&mut self, samples: &[Sample]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO sys_sample
                 (tier, ts, cpu_avg, cpu_max, mem_used_avg, mem_used_max,
                  gpu_avg, gpu_max, vram_used_max, disk_read_bytes, disk_write_bytes,
                  net_in_bytes, net_out_bytes)
                 VALUES (0, ?1, ?2, ?2, ?3, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for s in samples {
                stmt.execute((
                    s.timestamp as i64,
                    s.cpu_percent,
                    s.mem_used_bytes as i64,
                    s.gpu_percent,
                    s.vram_used_bytes as i64,
                    s.disk_read_bytes as i64,
                    s.disk_write_bytes as i64,
                    s.net_in_bytes as i64,
                    s.net_out_bytes as i64,
                ))?;
            }
        }
        tx.commit()
    }

    pub fn insert_apps(&mut self, batches: &[(u64, Vec<AppSample>)]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO proc_sample
                 (tier, ts, process_id, cpu_avg, cpu_max, mem_private_max, mem_working_max,
                  handles_max, disk_read_bytes, disk_write_bytes, net_in_bytes, net_out_bytes,
                  gpu_avg)
                 VALUES (0, ?1, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for (ts, apps) in batches {
                for a in apps {
                    let id = upsert_process(&tx, a, *ts)?;
                    insert.execute((
                        *ts as i64,
                        id,
                        a.cpu_percent,
                        a.private_bytes as i64,
                        a.working_set_bytes as i64,
                        a.handles as i64,
                        a.read_bytes as i64,
                        a.write_bytes as i64,
                        a.net_in_bytes as i64,
                        a.net_out_bytes as i64,
                        a.gpu_percent,
                    ))?;
                }
            }
        }
        tx.commit()
    }

    /// Roll tier 0 -> 1 (1 min) -> 2 (1 h) and prune old rows. Idempotent; call every few minutes.
    pub fn maintain(&mut self, now: u64) -> rusqlite::Result<()> {
        for (src, dst, bucket) in [(0i64, 1i64, 60i64), (1, 2, 3600)] {
            let key = format!("rollup_{dst}");
            let from: i64 = self
                .conn
                .query_row("SELECT value FROM setting WHERE key = ?1", [&key], |r| {
                    r.get::<_, String>(0)
                })
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            // 120 s lag so late-flushed rows are included
            let to = (now as i64 - 120) / bucket * bucket;
            if to <= from {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute(
                &format!(
                    "INSERT OR REPLACE INTO sys_sample
                     (tier, ts, cpu_avg, cpu_max, cpu_clock_avg, mem_used_avg, mem_used_max,
                      commit_used_max, disk_read_bytes, disk_write_bytes, disk_busy_max,
                      net_in_bytes, net_out_bytes, gpu_avg, gpu_max, vram_used_max,
                      cpu_temp_max, gpu_temp_max, ssd_temp_max, throttled)
                     SELECT {dst}, ts / {bucket} * {bucket}, AVG(cpu_avg), MAX(cpu_max),
                      AVG(cpu_clock_avg), AVG(mem_used_avg), MAX(mem_used_max), MAX(commit_used_max),
                      SUM(disk_read_bytes), SUM(disk_write_bytes), MAX(disk_busy_max),
                      SUM(net_in_bytes), SUM(net_out_bytes), AVG(gpu_avg), MAX(gpu_max),
                      MAX(vram_used_max), MAX(cpu_temp_max), MAX(gpu_temp_max), MAX(ssd_temp_max),
                      MAX(throttled)
                     FROM sys_sample WHERE tier = {src} AND ts >= ?1 AND ts < ?2
                     GROUP BY ts / {bucket}"
                ),
                (from, to),
            )?;
            tx.execute(
                &format!(
                    "INSERT OR REPLACE INTO proc_sample
                     (tier, ts, process_id, cpu_avg, cpu_max, mem_private_max, mem_working_max,
                      handles_max, disk_read_bytes, disk_write_bytes, net_in_bytes, net_out_bytes,
                      gpu_avg, foreground_sec)
                     SELECT {dst}, ts / {bucket} * {bucket}, process_id, AVG(cpu_avg), MAX(cpu_max),
                      MAX(mem_private_max), MAX(mem_working_max), MAX(handles_max),
                      SUM(disk_read_bytes), SUM(disk_write_bytes), SUM(net_in_bytes),
                      SUM(net_out_bytes), AVG(gpu_avg), SUM(foreground_sec)
                     FROM proc_sample WHERE tier = {src} AND ts >= ?1 AND ts < ?2
                     GROUP BY ts / {bucket}, process_id"
                ),
                (from, to),
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO setting (key, value) VALUES (?1, ?2)",
                (&key, to.to_string()),
            )?;
            tx.commit()?;
        }
        for (tier, keep) in [(0i64, DAY), (1, 30 * DAY), (2, 365 * DAY)] {
            let cutoff = now as i64 - keep;
            self.conn.execute(
                "DELETE FROM sys_sample WHERE tier = ?1 AND ts < ?2",
                (tier, cutoff),
            )?;
            self.conn.execute(
                "DELETE FROM proc_sample WHERE tier = ?1 AND ts < ?2",
                (tier, cutoff),
            )?;
        }
        self.conn.execute(
            "DELETE FROM snapshot WHERE ts < ?1",
            [now as i64 - 30 * DAY],
        )?;
        Ok(())
    }

    /// Store a spike snapshot plus the finding that references it. Returns the finding id.
    pub fn insert_finding(&mut self, f: &FindingInput) -> rusqlite::Result<i64> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshot (ts, trigger_metric, trigger_value, threshold, pre_window_sec, sys_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                f.ts as i64,
                f.metric,
                f.value,
                f.threshold,
                f.pre_window_sec,
                f.sys_json,
            ),
        )?;
        let snap = tx.last_insert_rowid();
        let mut culprit = None;
        for a in f.procs {
            let id = upsert_process(&tx, a, f.ts)?;
            if f.culprit == Some(a.name.as_str()) {
                culprit = Some(id);
            }
            tx.execute(
                "INSERT OR IGNORE INTO snapshot_proc
                 (snapshot_id, process_id, pid, cpu, mem_private, disk_read_rate, disk_write_rate,
                  net_rate, gpu)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    snap,
                    id,
                    a.top_pid,
                    a.cpu_percent,
                    a.private_bytes as i64,
                    a.read_bytes as i64,
                    a.write_bytes as i64,
                    (a.net_in_bytes + a.net_out_bytes) as i64,
                    a.gpu_percent,
                ),
            )?;
        }
        tx.execute(
            "INSERT INTO finding (ts_start, kind, severity, process_id, snapshot_id, detail_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                f.ts as i64,
                f.kind,
                f.severity,
                culprit,
                snap,
                f.detail_json,
            ),
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// Sync startup entries. First call only baselines; later calls log added/changed/removed.
    pub fn sync_startup(&mut self, entries: &[StartupItem], now: u64) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        let baseline: bool =
            tx.query_row("SELECT COUNT(*) = 0 FROM startup_entry", [], |r| r.get(0))?;
        let now = now as i64;
        for e in entries {
            let existing: Option<(i64, String, Option<i64>)> = tx
                .query_row(
                    "SELECT id, command, removed_at FROM startup_entry
                     WHERE source = ?1 AND location = ?2 AND name = ?3",
                    (e.source, e.location, e.name),
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok();
            match existing {
                None => {
                    tx.execute(
                        "INSERT INTO startup_entry
                         (source, location, name, command, enabled, first_seen, last_seen)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                        (e.source, e.location, e.name, &e.command, e.enabled, now),
                    )?;
                    if !baseline {
                        let id = tx.last_insert_rowid();
                        tx.execute(
                            "INSERT INTO startup_change (ts, entry_id, change, new_command)
                             VALUES (?1, ?2, 'added', ?3)",
                            (now, id, &e.command),
                        )?;
                    }
                }
                Some((id, old, removed)) => {
                    if old != e.command {
                        tx.execute(
                            "INSERT INTO startup_change (ts, entry_id, change, old_command, new_command)
                             VALUES (?1, ?2, 'changed', ?3, ?4)",
                            (now, id, &old, &e.command),
                        )?;
                    } else if removed.is_some() {
                        tx.execute(
                            "INSERT INTO startup_change (ts, entry_id, change, new_command)
                             VALUES (?1, ?2, 'added', ?3)",
                            (now, id, &e.command),
                        )?;
                    }
                    tx.execute(
                        "UPDATE startup_entry
                         SET command = ?1, enabled = ?2, last_seen = ?3, removed_at = NULL
                         WHERE id = ?4",
                        (&e.command, e.enabled, now, id),
                    )?;
                }
            }
        }
        // anything not touched this round has disappeared
        let gone: Vec<(i64, String)> = {
            let mut st = tx.prepare(
                "SELECT id, command FROM startup_entry WHERE last_seen < ?1 AND removed_at IS NULL",
            )?;
            let rows = st.query_map([now], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        for (id, cmd) in gone {
            tx.execute(
                "UPDATE startup_entry SET removed_at = ?1 WHERE id = ?2",
                (now, id),
            )?;
            tx.execute(
                "INSERT INTO startup_change (ts, entry_id, change, old_command)
                 VALUES (?1, ?2, 'removed', ?3)",
                (now, id, cmd),
            )?;
        }
        tx.commit()
    }

    pub fn record_boot(&mut self, b: &BootInfo) -> rusqlite::Result<()> {
        // boot_ts is derived from uptime and drifts by a second or two between calls, so match nearby rows
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM boot WHERE ABS(boot_ts - ?1) < 120",
                [b.boot_ts as i64],
                |r| r.get(0),
            )
            .ok();
        match existing {
            Some(id) => self.conn.execute(
                "UPDATE boot SET duration_ms = COALESCE(?1, duration_ms),
                   main_path_ms = COALESCE(?2, main_path_ms),
                   post_boot_ms = COALESCE(?3, post_boot_ms) WHERE id = ?4",
                (b.duration_ms, b.main_path_ms, b.post_boot_ms, id),
            )?,
            None => self.conn.execute(
                "INSERT INTO boot (boot_ts, duration_ms, main_path_ms, post_boot_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                (
                    b.boot_ts as i64,
                    b.duration_ms,
                    b.main_path_ms,
                    b.post_boot_ms,
                ),
            )?,
        };
        Ok(())
    }

    pub fn insert_drive_health(&mut self, d: &DriveInfo, ts: u64) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        let id: i64 = tx.query_row(
            "INSERT INTO drive (model, serial_hash, kind, capacity_bytes, first_seen)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(serial_hash) DO UPDATE SET model = excluded.model, kind = excluded.kind
             RETURNING id",
            (
                &d.model,
                &d.serial_hash,
                &d.kind,
                d.capacity_bytes as i64,
                ts as i64,
            ),
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO drive_health
             (drive_id, ts, temp_c, percent_used, host_writes_bytes, host_reads_bytes,
              available_spare, media_errors, power_on_hours)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (
                id,
                ts as i64,
                d.temp_c,
                d.percent_used,
                d.host_writes_bytes.map(|v| v as i64),
                d.host_reads_bytes.map(|v| v as i64),
                d.available_spare,
                d.media_errors.map(|v| v as i64),
                d.power_on_hours.map(|v| v as i64),
            ),
        )?;
        // keep the hottest solid-state reading on the system row so graphs can overlay it
        if let (Some(t), false) = (d.temp_c, d.kind == "hdd") {
            tx.execute(
                "UPDATE sys_sample SET ssd_temp_max = MAX(COALESCE(ssd_temp_max, 0), ?1)
                 WHERE tier = 0 AND ts = (SELECT MAX(ts) FROM sys_sample WHERE tier = 0)",
                [t],
            )?;
        }
        tx.commit()
    }
}

fn upsert_process(conn: &Connection, a: &AppSample, ts: u64) -> rusqlite::Result<i64> {
    let key = if a.exe_path.is_empty() {
        &a.name
    } else {
        &a.exe_path
    };
    let group = a.name.trim_end_matches(".exe").to_lowercase();
    let (id, signed): (i64, Option<i64>) = conn
        .prepare_cached(
            "INSERT INTO process (exe_path, name, group_name, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(exe_path) DO UPDATE SET last_seen = excluded.last_seen
             RETURNING id, signed",
        )?
        .query_row((key, &a.name, group, ts as i64), |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    // shortcut: sync check on first sight (~ms per new exe), move to a worker if flushes stall
    if signed.is_none() && !a.exe_path.is_empty() {
        conn.prepare_cached("UPDATE process SET signed = ?1 WHERE id = ?2")?
            .execute((collector::is_signed(&a.exe_path), id))?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, cpu: f64) -> AppSample {
        AppSample {
            name: name.into(),
            exe_path: String::new(),
            instances: 1,
            top_pid: 1,
            cpu_percent: cpu,
            private_bytes: 1,
            working_set_bytes: 1,
            handles: 1,
            read_bytes: 0,
            write_bytes: 0,
            net_in_bytes: 0,
            net_out_bytes: 0,
            gpu_percent: 0.0,
        }
    }

    fn mem() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn rollup_creates_minute_and_prunes() {
        let mut s = mem();
        let batch: Vec<_> = (0..30)
            .map(|i| (1000 + i * 2, vec![app("a.exe", 10.0)]))
            .collect();
        s.insert_apps(&batch).unwrap();
        s.maintain(1000 + 60 + 200).unwrap();
        let n: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM proc_sample WHERE tier = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(n >= 1);
        // tier-0 rows older than a day are pruned
        s.maintain(1000 + 2 * DAY as u64).unwrap();
        let n0: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM proc_sample WHERE tier = 0", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n0, 0);
    }

    #[test]
    fn startup_baseline_then_changes() {
        let mut s = mem();
        let item = |cmd: &str| StartupItem {
            source: "run",
            location: "HKCU",
            name: "x",
            command: cmd.into(),
            enabled: true,
        };
        s.sync_startup(&[item("a")], 10).unwrap();
        let c = |s: &Store| -> i64 {
            s.conn
                .query_row("SELECT COUNT(*) FROM startup_change", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(c(&s), 0);
        s.sync_startup(&[item("b")], 20).unwrap();
        assert_eq!(c(&s), 1);
        s.sync_startup(&[], 30).unwrap();
        assert_eq!(c(&s), 2);
    }

    #[test]
    fn boot_rows_merge_when_timestamp_drifts() {
        let mut s = mem();
        let b = |ts, d| BootInfo {
            boot_ts: ts,
            duration_ms: d,
            main_path_ms: None,
            post_boot_ms: None,
        };
        s.record_boot(&b(1000, None)).unwrap();
        s.record_boot(&b(1001, Some(5))).unwrap();
        let (n, d): (i64, Option<i64>) = s
            .conn
            .query_row("SELECT COUNT(*), MAX(duration_ms) FROM boot", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((n, d), (1, Some(5)));
    }

    #[test]
    fn finding_links_snapshot_and_culprit() {
        let mut s = mem();
        let procs = vec![app("hog.exe", 90.0)];
        let id = s
            .insert_finding(&FindingInput {
                ts: 5,
                kind: "cpu_spike",
                severity: 2,
                metric: "cpu",
                value: 95.0,
                threshold: 90.0,
                pre_window_sec: 30,
                sys_json: "{}",
                detail_json: "{}",
                procs: &procs,
                culprit: Some("hog.exe"),
            })
            .unwrap();
        let pid: Option<i64> = s
            .conn
            .query_row("SELECT process_id FROM finding WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(pid.is_some());
    }
}
