PRAGMA user_version = 1;

CREATE TABLE setting (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE process (
  id INTEGER PRIMARY KEY,
  exe_path TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  group_name TEXT,
  signed INTEGER,
  publisher TEXT,
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  ignored INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sys_sample (
  tier INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  cpu_avg REAL,
  cpu_max REAL,
  cpu_clock_avg REAL,
  mem_used_avg INTEGER,
  mem_used_max INTEGER,
  commit_used_max INTEGER,
  disk_read_bytes INTEGER,
  disk_write_bytes INTEGER,
  disk_busy_max REAL,
  net_in_bytes INTEGER,
  net_out_bytes INTEGER,
  gpu_avg REAL,
  gpu_max REAL,
  vram_used_max INTEGER,
  cpu_temp_max REAL,
  gpu_temp_max REAL,
  ssd_temp_max REAL,
  throttled INTEGER,
  PRIMARY KEY (tier, ts)
) WITHOUT ROWID;

CREATE TABLE proc_sample (
  tier INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  process_id INTEGER NOT NULL REFERENCES process(id),
  cpu_avg REAL,
  cpu_max REAL,
  mem_private_max INTEGER,
  mem_working_max INTEGER,
  handles_max INTEGER,
  disk_read_bytes INTEGER,
  disk_write_bytes INTEGER,
  net_in_bytes INTEGER,
  net_out_bytes INTEGER,
  gpu_avg REAL,
  foreground_sec INTEGER,
  PRIMARY KEY (tier, ts, process_id)
) WITHOUT ROWID;

CREATE INDEX idx_proc_sample_process ON proc_sample (process_id, tier, ts);

CREATE TABLE drive (
  id INTEGER PRIMARY KEY,
  model TEXT NOT NULL,
  serial_hash TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  capacity_bytes INTEGER,
  first_seen INTEGER NOT NULL
);

CREATE TABLE drive_health (
  drive_id INTEGER NOT NULL REFERENCES drive(id),
  ts INTEGER NOT NULL,
  temp_c REAL,
  percent_used REAL,
  host_writes_bytes INTEGER,
  host_reads_bytes INTEGER,
  available_spare REAL,
  media_errors INTEGER,
  power_on_hours INTEGER,
  PRIMARY KEY (drive_id, ts)
) WITHOUT ROWID;

CREATE TABLE snapshot (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  trigger_metric TEXT NOT NULL,
  trigger_value REAL NOT NULL,
  threshold REAL NOT NULL,
  pre_window_sec INTEGER NOT NULL,
  sys_json TEXT NOT NULL
);

CREATE INDEX idx_snapshot_ts ON snapshot (ts);

CREATE TABLE snapshot_proc (
  snapshot_id INTEGER NOT NULL REFERENCES snapshot(id) ON DELETE CASCADE,
  process_id INTEGER NOT NULL REFERENCES process(id),
  pid INTEGER NOT NULL,
  cpu REAL,
  mem_private INTEGER,
  disk_read_rate INTEGER,
  disk_write_rate INTEGER,
  net_rate INTEGER,
  gpu REAL,
  PRIMARY KEY (snapshot_id, pid)
) WITHOUT ROWID;

CREATE TABLE finding (
  id INTEGER PRIMARY KEY,
  ts_start INTEGER NOT NULL,
  ts_end INTEGER,
  kind TEXT NOT NULL,
  severity INTEGER NOT NULL,
  process_id INTEGER REFERENCES process(id),
  drive_id INTEGER REFERENCES drive(id),
  snapshot_id INTEGER REFERENCES snapshot(id),
  detail_json TEXT NOT NULL,
  dismissed INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_finding_ts ON finding (ts_start);

CREATE TABLE startup_entry (
  id INTEGER PRIMARY KEY,
  source TEXT NOT NULL,
  location TEXT NOT NULL,
  name TEXT NOT NULL,
  command TEXT NOT NULL,
  process_id INTEGER REFERENCES process(id),
  enabled INTEGER NOT NULL,
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  removed_at INTEGER,
  UNIQUE (source, location, name)
);

CREATE TABLE startup_change (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  entry_id INTEGER NOT NULL REFERENCES startup_entry(id),
  change TEXT NOT NULL,
  old_command TEXT,
  new_command TEXT
);

CREATE TABLE boot (
  id INTEGER PRIMARY KEY,
  boot_ts INTEGER NOT NULL UNIQUE,
  duration_ms INTEGER,
  main_path_ms INTEGER,
  post_boot_ms INTEGER
);

CREATE TABLE startup_impact (
  boot_id INTEGER NOT NULL REFERENCES boot(id),
  entry_id INTEGER NOT NULL REFERENCES startup_entry(id),
  start_delay_ms INTEGER,
  cpu_ms INTEGER,
  disk_bytes INTEGER,
  window_sec INTEGER NOT NULL,
  PRIMARY KEY (boot_id, entry_id)
) WITHOUT ROWID;

CREATE TABLE event_log (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  channel TEXT NOT NULL,
  provider TEXT NOT NULL,
  event_id INTEGER NOT NULL,
  level INTEGER NOT NULL,
  process_id INTEGER REFERENCES process(id),
  summary TEXT
);

CREATE INDEX idx_event_log_ts ON event_log (ts);