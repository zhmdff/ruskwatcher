//! Read queries behind the UI dashboard.
use crate::Store;
use rusqlite::params;
use serde_json::{json, Value};

const MAX_POINTS: i64 = 300;

impl Store {
    /// Everything the dashboard shows for the last `range` seconds, as one JSON document.
    pub fn dashboard(&self, range: u64, now: u64) -> rusqlite::Result<Value> {
        let range = range.clamp(60, 365 * 86_400) as i64;
        let now = now as i64;
        let from = now - range;
        // tier 0 is kept 1 day, tier 1 30 days, tier 2 a year
        let tier: i64 = if range <= 86_400 {
            0
        } else if range <= 30 * 86_400 {
            1
        } else {
            2
        };
        let bucket = (range / MAX_POINTS).max(1);

        let series: Vec<Value> = self
            .conn
            .prepare(
                "SELECT ts / ?1 * ?1, AVG(cpu_avg), AVG(mem_used_avg), AVG(gpu_avg),
                        SUM(COALESCE(disk_read_bytes,0) + COALESCE(disk_write_bytes,0)),
                        SUM(COALESCE(net_in_bytes,0) + COALESCE(net_out_bytes,0))
                 FROM sys_sample WHERE tier = ?2 AND ts >= ?3
                 GROUP BY ts / ?1 ORDER BY 1",
            )?
            .query_map(params![bucket, tier, from], |r| {
                Ok(json!({
                    "ts": r.get::<_, i64>(0)?,
                    "cpu": r.get::<_, Option<f64>>(1)?,
                    "mem": r.get::<_, Option<f64>>(2)?,
                    "gpu": r.get::<_, Option<f64>>(3)?,
                    "disk_bps": r.get::<_, Option<i64>>(4)?.unwrap_or(0) as f64 / bucket as f64,
                    "net_bps": r.get::<_, Option<i64>>(5)?.unwrap_or(0) as f64 / bucket as f64,
                }))
            })?
            .collect::<Result<_, _>>()?;

        // averages are over samples where the app was active (idle apps are not stored)
        let apps: Vec<Value> = self
            .conn
            .prepare(
                "SELECT p.name, p.exe_path, p.signed, AVG(s.cpu_avg), MAX(s.cpu_max),
                        MAX(s.mem_private_max), SUM(COALESCE(s.disk_read_bytes,0) + COALESCE(s.disk_write_bytes,0)),
                        SUM(COALESCE(s.net_in_bytes,0) + COALESCE(s.net_out_bytes,0)), AVG(s.gpu_avg), COUNT(*)
                 FROM proc_sample s JOIN process p ON p.id = s.process_id
                 WHERE s.tier = ?1 AND s.ts >= ?2
                 GROUP BY s.process_id ORDER BY SUM(s.cpu_avg) DESC LIMIT 30",
            )?
            .query_map(params![tier, from], |r| {
                Ok(json!({
                    "name": r.get::<_, String>(0)?,
                    "path": r.get::<_, String>(1)?,
                    "signed": r.get::<_, Option<i64>>(2)?,
                    "cpu_avg": r.get::<_, Option<f64>>(3)?,
                    "cpu_max": r.get::<_, Option<f64>>(4)?,
                    "mem_max": r.get::<_, Option<i64>>(5)?,
                    "disk_bytes": r.get::<_, Option<i64>>(6)?,
                    "net_bytes": r.get::<_, Option<i64>>(7)?,
                    "gpu_avg": r.get::<_, Option<f64>>(8)?,
                    "samples": r.get::<_, i64>(9)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let findings: Vec<Value> = self
            .conn
            .prepare(
                "SELECT f.id, f.ts_start, f.kind, f.severity, p.name, f.detail_json, sn.trigger_value, sn.threshold
                 FROM finding f
                 LEFT JOIN process p ON p.id = f.process_id
                 LEFT JOIN snapshot sn ON sn.id = f.snapshot_id
                 WHERE f.ts_start >= ?1 ORDER BY f.ts_start DESC LIMIT 50",
            )?
            .query_map([from], |r| {
                let detail: String = r.get(5)?;
                let summary = serde_json::from_str::<Value>(&detail)
                    .ok()
                    .and_then(|v| v["summary"].as_str().map(String::from))
                    .unwrap_or(detail);
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "ts": r.get::<_, i64>(1)?,
                    "kind": r.get::<_, String>(2)?,
                    "severity": r.get::<_, i64>(3)?,
                    "app": r.get::<_, Option<String>>(4)?,
                    "summary": summary,
                    "value": r.get::<_, Option<f64>>(6)?,
                    "threshold": r.get::<_, Option<f64>>(7)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let drives: Vec<Value> = self
            .conn
            .prepare(
                "SELECT d.model, d.kind, d.capacity_bytes, h.temp_c, h.percent_used, h.host_writes_bytes,
                        h.available_spare, h.media_errors, h.power_on_hours, h.ts
                 FROM drive d JOIN drive_health h ON h.drive_id = d.id
                 WHERE h.ts = (SELECT MAX(ts) FROM drive_health WHERE drive_id = d.id)",
            )?
            .query_map([], |r| {
                Ok(json!({
                    "model": r.get::<_, String>(0)?,
                    "kind": r.get::<_, String>(1)?,
                    "capacity": r.get::<_, Option<i64>>(2)?,
                    "temp_c": r.get::<_, Option<f64>>(3)?,
                    "percent_used": r.get::<_, Option<f64>>(4)?,
                    "host_writes": r.get::<_, Option<i64>>(5)?,
                    "spare": r.get::<_, Option<f64>>(6)?,
                    "media_errors": r.get::<_, Option<i64>>(7)?,
                    "power_on_hours": r.get::<_, Option<i64>>(8)?,
                    "ts": r.get::<_, i64>(9)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let startup: Vec<Value> = self
            .conn
            .prepare(
                "SELECT name, source, command, enabled, first_seen FROM startup_entry
                 WHERE removed_at IS NULL ORDER BY name COLLATE NOCASE",
            )?
            .query_map([], |r| {
                Ok(json!({
                    "name": r.get::<_, String>(0)?,
                    "source": r.get::<_, String>(1)?,
                    "command": r.get::<_, String>(2)?,
                    "enabled": r.get::<_, i64>(3)? != 0,
                    "first_seen": r.get::<_, i64>(4)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let changes: Vec<Value> = self
            .conn
            .prepare(
                "SELECT c.ts, c.change, e.name, c.old_command, c.new_command
                 FROM startup_change c JOIN startup_entry e ON e.id = c.entry_id
                 ORDER BY c.ts DESC LIMIT 20",
            )?
            .query_map([], |r| {
                Ok(json!({
                    "ts": r.get::<_, i64>(0)?,
                    "change": r.get::<_, String>(1)?,
                    "name": r.get::<_, String>(2)?,
                    "old": r.get::<_, Option<String>>(3)?,
                    "new": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let boots: Vec<Value> = self
            .conn
            .prepare(
                "SELECT boot_ts, duration_ms, main_path_ms, post_boot_ms FROM boot ORDER BY boot_ts DESC LIMIT 5",
            )?
            .query_map([], |r| {
                Ok(json!({
                    "ts": r.get::<_, i64>(0)?,
                    "duration_ms": r.get::<_, Option<i64>>(1)?,
                    "main_path_ms": r.get::<_, Option<i64>>(2)?,
                    "post_boot_ms": r.get::<_, Option<i64>>(3)?,
                }))
            })?
            .collect::<Result<_, _>>()?;

        let setting = |k: &str| -> Option<String> {
            self.conn
                .query_row("SELECT value FROM setting WHERE key = ?1", [k], |r| {
                    r.get(0)
                })
                .ok()
        };
        let last_sample: Option<i64> =
            self.conn
                .query_row("SELECT MAX(ts) FROM sys_sample WHERE tier = 0", [], |r| {
                    r.get(0)
                })?;

        Ok(json!({
            "mem_total": setting("mem_total").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
            "elevated": setting("elevated").as_deref() == Some("1"),
            "last_sample": last_sample,
            "now": now, "range": range, "bucket": bucket,
            "series": series, "apps": apps, "findings": findings,
            "drives": drives, "startup": startup, "startup_changes": changes, "boots": boots,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dashboard_on_empty_db() {
        let s = Store::open(Path::new(":memory:")).unwrap();
        let v = s.dashboard(3600, 10_000).unwrap();
        assert_eq!(v["series"].as_array().unwrap().len(), 0);
        assert!(v["apps"].is_array() && v["findings"].is_array());
    }
}
