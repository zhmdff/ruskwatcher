use collector::{AppSample, Sample};
use rusqlite::Connection;
use std::path::Path;

const SCHEMA: &str = include_str!("../schema.sql");

pub struct Store {
    conn: Connection,
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

    pub fn insert_samples(&mut self, samples: &[Sample]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO sys_sample
                 (tier, ts, cpu_avg, cpu_max, mem_used_avg, mem_used_max)
                 VALUES (0, ?1, ?2, ?2, ?3, ?3)",
            )?;
            for s in samples {
                stmt.execute((s.timestamp as i64, s.cpu_percent, s.mem_used_bytes as i64))?;
            }
        }
        tx.commit()
    }

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
}
