# RuskWatcher: Status

## Where you are

| Step | What works                                                                    | State                                                                                                                      |
| ---- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1-4b | Workspace, system sampler, SQLite store, per-process sampling and aggregation | done, verified                                                                                                             |
| 5    | Exe path, Authenticode signed flag, grouping by path                          | done, verified                                                                                                             |
| 6    | Adaptive idle/normal/spike sampling, 30 s ring buffer                         | done, unit tested                                                                                                          |
| 7    | 1-min / 1-h rollups, retention                                                | done, unit tested                                                                                                          |
| 8    | ETW per-app disk and network                                                  | verified from the installed service (non-zero per-app disk/net); accuracy vs Resource Monitor not compared                 |
| 9    | GPU (PDH) and NVMe SMART                                                      | done, verified; Intel temps and AMD ADLX skipped                                                                           |
| 10   | Detection rules, snapshots, findings                                          | done, a real finding was produced in a live run                                                                            |
| 11   | Startup entries and changes, boot time                                        | entries verified; boot durations need admin                                                                                |
| 12   | Windows service, install/uninstall/autostart commands                         | service install and ETW verified on a live install (per-app disk/network recorded); autostart/start/stop commands untested |
| 13   | UI                                                                            | Tauri app (`crates/app`) reading the DB read-only, with Settings tab; builds and launches, not clicked through             |
| 14a  | Installers                                                                    | NSIS `.exe` and WiX `.msi` built by `installer\build.ps1`, with an autostart option; **not run yet** (need UAC)            |

## Test next

1. Install `dist\RuskWatcher-Setup-0.1.0.exe` (or the `.msi`). Untick the autostart option once and confirm `sc qc RuskWatcher` shows DEMAND_START; tick it and confirm AUTO_START.
2. Open the app: Settings tab should show running state and toggle autostart / start / stop (one UAC prompt each).
3. Burn CPU for 30 s and confirm a finding appears under Findings.
4. Measure `service.exe` cost (targets: < 0.1% CPU, < 15 MB RAM) and the app's idle cost.
5. Upgrade test: install again over an existing install.

## Left on the roadmap

- Verify ETW and service install (above).
- Optional: Intel CPU temps, AMD ADLX, SATA SMART, scheduled-task/service startup entries, startup-impact scoring.
- Step 14: Event Log correlation, SSD lifespan projection.
- Per-finding detail view in the UI (snapshots are stored, not yet shown).

## Habits to keep

- Run `cargo fmt` and `cargo clippy` after every step.
- Commit after every working step: `git add . && git commit -m "step N"`.
- Re-measure the service's own CPU and RAM after every step.
