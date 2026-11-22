mod adaptive;
mod detect;
mod run;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

const NAME: &str = "RuskWatcher";

define_windows_service!(ffi_service_main, service_main);

/// Service mode keeps data in ProgramData; console mode uses RUSKWATCHER_DB or ./ruskwatcher.db.
fn db_path(service: bool) -> PathBuf {
    if service {
        let dir = PathBuf::from(
            std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()),
        )
        .join("RuskWatcher");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("ruskwatcher.db")
    } else {
        std::env::var("RUSKWATCHER_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("ruskwatcher.db"))
    }
}

fn service_main(_args: Vec<OsString>) {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handler = move |ev| match ev {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            flag.store(true, Ordering::Relaxed);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(status) = service_control_handler::register(NAME, handler) else {
        return;
    };
    let set = |state, accept, code| {
        let _ = status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accept,
            exit_code: code,
            checkpoint: 0,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        });
    };
    set(
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        ServiceExitCode::Win32(0),
    );
    let result = run::run(stop, &db_path(true));
    if let Err(e) = &result {
        eprintln!("{e}");
    }
    set(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        ServiceExitCode::Win32(if result.is_ok() { 0 } else { 1 }),
    );
}

/// Re-launch this exe elevated (UAC prompt) with the same arguments.
fn relaunch_elevated() {
    use windows::core::{w, HSTRING};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe().unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            &HSTRING::from(exe.to_string_lossy().as_ref()),
            &HSTRING::from(args.join(" ")),
            None,
            SW_SHOWNORMAL,
        );
    }
}

fn install() -> Result<(), String> {
    let e = |e: windows_service::Error| e.to_string();
    let mgr = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(e)?;
    let info = ServiceInfo {
        name: NAME.into(),
        display_name: "RuskWatcher".into(),
        service_type: ServiceType::OWN_PROCESS,
        // `install manual` keeps it off until the user enables startup in Settings
        start_type: if std::env::args().nth(2).as_deref() == Some("manual") {
            ServiceStartType::OnDemand
        } else {
            ServiceStartType::AutoStart
        },
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe().map_err(|e| e.to_string())?,
        launch_arguments: vec!["--service".into()],
        dependencies: vec![],
        account_name: None, // LocalSystem: elevated, so ETW works
        account_password: None,
    };
    let svc = mgr
        .create_service(&info, ServiceAccess::START | ServiceAccess::CHANGE_CONFIG)
        .map_err(e)?;
    let _ = svc.set_description("Records per-app CPU, memory, disk, network and GPU history.");
    svc.start::<&str>(&[]).map_err(e)?;
    println!("Installed and started. Data: {}", db_path(true).display());
    Ok(())
}

/// Boot-time collection on/off = service start type Auto/Manual.
fn set_autostart(on: bool) -> Result<(), String> {
    let e = |e: windows_service::Error| e.to_string();
    let mgr =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(e)?;
    let svc = mgr
        .open_service(NAME, ServiceAccess::CHANGE_CONFIG)
        .map_err(e)?;
    let info = ServiceInfo {
        name: NAME.into(),
        display_name: "RuskWatcher".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: if on {
            ServiceStartType::AutoStart
        } else {
            ServiceStartType::OnDemand
        },
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe().map_err(|e| e.to_string())?,
        launch_arguments: vec!["--service".into()],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    svc.change_config(&info).map_err(e)
}

fn control(start: bool) -> Result<(), String> {
    let e = |e: windows_service::Error| e.to_string();
    let mgr =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(e)?;
    let svc = mgr
        .open_service(NAME, ServiceAccess::START | ServiceAccess::STOP)
        .map_err(e)?;
    if start {
        svc.start::<&str>(&[]).map_err(e)
    } else {
        svc.stop().map(|_| ()).map_err(e)
    }
}

fn uninstall() -> Result<(), String> {
    let e = |e: windows_service::Error| e.to_string();
    let mgr =
        ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(e)?;
    let svc = mgr
        .open_service(
            NAME,
            ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
        )
        .map_err(e)?;
    let _ = svc.stop();
    svc.delete().map_err(e)?;
    println!("Service removed (data kept).");
    Ok(())
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let result = match arg.as_str() {
        "install" | "uninstall" | "autostart" | "start" | "stop" => {
            if !collector::is_elevated() {
                println!("Admin rights needed; asking for elevation...");
                relaunch_elevated();
                return;
            }
            match arg.as_str() {
                "install" => install(),
                "uninstall" => uninstall(),
                "autostart" => set_autostart(std::env::args().nth(2).as_deref() == Some("on")),
                "start" => control(true),
                _ => control(false),
            }
        }
        "--service" => service_dispatcher::start(NAME, ffi_service_main).map_err(|e| e.to_string()),
        _ => {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let _ = ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed));
            if !collector::is_elevated() {
                println!("Not elevated: per-app disk/network (ETW) will be off.");
            }
            run::run(stop, &db_path(false))
        }
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
