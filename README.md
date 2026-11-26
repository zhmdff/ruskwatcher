# RuskWatcher

Records per-app CPU, memory, disk, network and GPU history on Windows, flags spikes, and shows it in a local dashboard.

## How it works

Two programs, one SQLite file:

- **Collector service** (`service.exe`, Windows service `RuskWatcher`, runs as LocalSystem). Samples in the background, writes `C:\ProgramData\RuskWatcher\ruskwatcher.db`, raises findings. It has no window and no network port. LocalSystem is what makes per-app disk/network (ETW) work.
- **Dashboard app** (`RuskWatcher.exe`, Tauri). Opens on demand, reads the database read-only, never writes. Close it and collection continues. Its Settings page shows service state, can start/stop it, and toggles "start collecting when Windows starts" (service start type Auto vs Manual). Those changes show one UAC prompt, because the app itself is not elevated.

## Install

Build both installers (downloads NSIS and WiX into `%LOCALAPPDATA%\RuskWatcherBuildTools` once):

```powershell
.\build              # release binaries + both installers
.\build -SkipBuild   # repackage only, no cargo build
```

Output in `dist\`: `RuskWatcher-Setup-<ver>.exe` (NSIS) and `RuskWatcher-<ver>.msi` (WiX). Both install to `C:\Program Files\RuskWatcher`, install and start the service, add a Start Menu shortcut, and show a components page with the option **Start collecting automatically when Windows starts** (on by default). Unticked = service start type Manual.

Silent MSI: `msiexec /i RuskWatcher-0.1.0.msi /qn` (autostart feature on), or add `ADDLOCAL=FeatMain` to leave autostart off. Upgrades replace the old version. Uninstall keeps `C:\ProgramData\RuskWatcher` (your history).

Needs the Edge WebView2 runtime for the app (preinstalled on Windows 11).

## Run from source

```
cargo run -p service            # console collector, db = .\ruskwatcher.db (or $env:RUSKWATCHER_DB)
$env:RUSKWATCHER_DB="$PWD\ruskwatcher.db"; cargo run -p ruskwatcher-app
```

`service.exe install [manual] | uninstall | autostart on|off | start | stop` manage the service by hand (prompts for elevation). Ctrl+C in console mode flushes and exits.

## Layout

| Crate       | Role                                                                                                                                                                        |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `collector` | Windows sampling: system CPU/RAM, all processes (`NtQuerySystemInformation`), exe path + signature, ETW disk/network, GPU (PDH), NVMe SMART, startup entries, boot timing   |
| `store`     | SQLite (`schema.sql`), batched writes, rollups (tier 0 = 2 s, 1 = 1 min, 2 = 1 h), retention, findings/snapshots, dashboard queries                                         |
| `service`   | Sampling loop (adaptive idle/normal/spike), detection rules, Windows service wrapper and its install/start/autostart commands                                               |
| `app`       | Tauri dashboard (`ui/`, no JS build step; wording in `i18n.js`, plus `app.js`, `icons.js`, `table.js`, `style.css`): Overview, Apps, Findings, Startup, Drives, Settings, About. Commands: `dashboard`, `service_status`, `set_autostart`, `set_running`      |

## Behaviour notes

- Sampling: 5 s idle (CPU < 8%), 2 s normal, 1 s spike (CPU >= 80%, exits below 60%). Writes flushed every 10 s.
- Only active apps are stored per sample (CPU >= 0.5% or I/O >= 256 KB). App identity is the exe path; instances are summed.
- Retention: tier 0 for 1 day, tier 1 for 30 days, tier 2 for 1 year; snapshots 30 days.
- Detection (5 min cooldown per rule and app): system CPU >= 90% over 20 s, app CPU >= 50% over 20 s, RAM >= 90% over 10 s, app disk >= 100 MB/s over 10 s. Each writes a `finding` plus a `snapshot` of the top 10 apps.
- The app opens the database read-only (WAL lets it read while the service writes).

## Known limits

- Not elevated: disk/network show as zero. Boot duration and some protected-process paths also need admin.
- No Intel CPU temps, AMD ADLX, SATA SMART, scheduled-task/service startup entries or startup-impact scoring yet.
- Signature check is run on first sight of an exe, inline with the flush.
- Installers are not code-signed, so SmartScreen will warn.
