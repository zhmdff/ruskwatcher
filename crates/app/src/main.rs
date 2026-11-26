#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The service writes here; RUSKWATCHER_DB overrides for dev runs.
fn db_path() -> PathBuf {
    std::env::var("RUSKWATCHER_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
                .join("RuskWatcher")
                .join("ruskwatcher.db")
        })
}

/// Read-only snapshot of everything the dashboard shows. The app never writes.
#[tauri::command]
fn dashboard(range: u64) -> Result<serde_json::Value, String> {
    let path = db_path();
    if !path.exists() {
        return Err(format!("no_data:{}", path.display()));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    store::Store::open_readonly(&path)
        .and_then(|s| s.dashboard(range, now))
        .map_err(|e| e.to_string())
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hidden(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// service.exe sits next to the app in the install directory (and in target/ for dev runs).
fn service_exe() -> Result<PathBuf, String> {
    let p = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("service.exe");
    p.exists()
        .then_some(p.clone())
        .ok_or(format!("service_exe_missing:{}", p.display()))
}

/// Number after "KEY :" in `sc` output; numeric codes avoid depending on the display language.
fn sc_code(out: &str, key: &str) -> Option<u32> {
    out.lines()
        .find(|l| l.trim_start().starts_with(key))?
        .split(':')
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Whether the background service exists, runs, and starts with Windows. Needs no admin.
#[tauri::command]
fn service_status() -> serde_json::Value {
    let run = |args: &[&str]| {
        hidden("sc.exe")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
    };
    let q = run(&["query", "RuskWatcher"]);
    let c = run(&["qc", "RuskWatcher"]);
    let state = sc_code(&q, "STATE");
    serde_json::json!({
        "installed": state.is_some(),
        "running": state == Some(4),
        "autostart": sc_code(&c, "START_TYPE") == Some(2),
        "db": db_path().display().to_string(),
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// Runs `service.exe <args>` elevated (one UAC prompt) and waits for it.
fn service_elevated(args: &str) -> Result<(), String> {
    let exe = service_exe()?;
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList '{args}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode",
        exe.display()
    );
    let st = hidden("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| e.to_string())?;
    st.success().then_some(()).ok_or("elevation_failed".into())
}

#[tauri::command]
fn set_autostart(on: bool) -> Result<(), String> {
    service_elevated(if on { "autostart on" } else { "autostart off" })
}

#[tauri::command]
fn set_running(on: bool) -> Result<(), String> {
    service_elevated(if on { "start" } else { "stop" })
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            dashboard,
            service_status,
            set_autostart,
            set_running
        ])
        .run(tauri::generate_context!())
        .expect("failed to start RuskWatcher app");
}

#[cfg(test)]
mod tests {
    use super::sc_code;

    #[test]
    fn missing_db_is_a_code() {
        std::env::set_var("RUSKWATCHER_DB", r"Z:\nope\x.db");
        assert_eq!(super::dashboard(60).unwrap_err(), r"no_data:Z:\nope\x.db");
    }

    #[test]
    fn status_has_version() {
        assert_eq!(
            super::service_status()["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn parses_sc_output() {
        let q = "SERVICE_NAME: RuskWatcher\n        TYPE               : 10  WIN32_OWN_PROCESS\n        STATE              : 4  RUNNING\n";
        assert_eq!(sc_code(q, "STATE"), Some(4));
        let c = "        START_TYPE         : 2   AUTO_START\n";
        assert_eq!(sc_code(c, "START_TYPE"), Some(2));
        assert_eq!(sc_code("", "STATE"), None);
    }
}
