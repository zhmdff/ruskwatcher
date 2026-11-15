use collector::Sample;
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
}