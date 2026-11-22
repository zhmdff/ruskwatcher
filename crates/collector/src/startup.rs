//! Startup entries (Run keys + Startup folders), elevation check and boot timing.
use std::process::Command;
use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[derive(Debug, Clone)]
pub struct StartupEntry {
    pub source: String,
    pub location: String,
    pub name: String,
    pub command: String,
    pub enabled: bool,
}

pub struct BootTimes {
    pub boot_ts: u64,
    pub duration_ms: Option<i64>,
    pub main_path_ms: Option<i64>,
    pub post_boot_ms: Option<i64>,
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut e = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut e as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && e.TokenIsElevated != 0
    }
}

/// All values of a registry key as (name, type, raw bytes).
fn reg_values(root: HKEY, path: &str) -> Vec<(String, u32, Vec<u8>)> {
    let mut out = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut key) != ERROR_SUCCESS {
            return out;
        }
        for i in 0.. {
            let mut name = [0u16; 512];
            let mut nlen = name.len() as u32;
            let mut data = vec![0u8; 4096];
            let mut dlen = data.len() as u32;
            let mut ty = 0u32;
            let r = RegEnumValueW(
                key,
                i,
                Some(PWSTR(name.as_mut_ptr())),
                &mut nlen,
                None,
                Some(&mut ty),
                Some(data.as_mut_ptr()),
                Some(&mut dlen),
            );
            if r == ERROR_SUCCESS {
                data.truncate(dlen as usize);
                out.push((String::from_utf16_lossy(&name[..nlen as usize]), ty, data));
            } else if r.0 == 234 {
                continue; // ERROR_MORE_DATA: value too large, skip it
            } else {
                break;
            }
        }
        let _ = RegCloseKey(key);
    }
    out
}

#[allow(clippy::chunks_exact_to_as_chunks)]
fn wide_str(b: &[u8]) -> String {
    let u: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u)
        .trim_end_matches('\0')
        .to_string()
}

pub fn startup_entries() -> Vec<StartupEntry> {
    const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const APPROVED: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
    let mut v = Vec::new();
    for (root, label) in [(HKEY_CURRENT_USER, "HKCU"), (HKEY_LOCAL_MACHINE, "HKLM")] {
        let approved = reg_values(root, APPROVED);
        for (name, ty, data) in reg_values(root, RUN) {
            if ty != REG_SZ.0 && ty != REG_EXPAND_SZ.0 {
                continue;
            }
            // StartupApproved: first byte even = enabled, odd = disabled by the user
            let enabled = approved
                .iter()
                .find(|(n, _, _)| *n == name)
                .and_then(|(_, _, d)| d.first())
                .is_none_or(|b| b % 2 == 0);
            v.push(StartupEntry {
                source: "run".into(),
                location: format!("{label}\\{RUN}"),
                name,
                command: wide_str(&data),
                enabled,
            });
        }
    }
    let sub = "\\Microsoft\\Windows\\Start Menu\\Programs\\Startup";
    let folders = [
        ("user", std::env::var("APPDATA").ok()),
        ("common", std::env::var("ProgramData").ok()),
    ];
    for (label, base) in folders {
        let Some(base) = base else { continue };
        let dir = format!("{base}{sub}");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            v.push(StartupEntry {
                source: "folder".into(),
                location: format!("{label}:{dir}"),
                name,
                command: f.path().to_string_lossy().to_string(),
                enabled: true,
            });
        }
    }
    v
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Days since 1970-01-01 for a civil date (Hinnant algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// "2026-11-22T10:00:00.123Z" -> epoch seconds
fn parse_iso(s: &str) -> Option<u64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let secs = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?) * 86_400
        + n(11..13)? * 3600
        + n(14..16)? * 60
        + n(17..19)?;
    u64::try_from(secs).ok()
}

fn xml_data(xml: &str, name: &str) -> Option<String> {
    let tag = format!("<Data Name=\"{name}\">");
    let alt = format!("<Data Name='{name}'>");
    let start = xml
        .find(&tag)
        .map(|i| i + tag.len())
        .or_else(|| xml.find(&alt).map(|i| i + alt.len()))?;
    Some(xml[start..start + xml[start..].find('<')?].to_string())
}

/// Boot timestamp from uptime, plus durations from the Diagnostics-Performance event 100
/// (best effort: the log may need admin or not exist yet for the current boot).
pub fn boot_info() -> BootTimes {
    let now = now_secs();
    let boot_ts = now.saturating_sub(unsafe { GetTickCount64() } / 1000);
    let mut t = BootTimes {
        boot_ts,
        duration_ms: None,
        main_path_ms: None,
        post_boot_ms: None,
    };
    let out = Command::new("wevtutil")
        .args([
            "qe",
            "Microsoft-Windows-Diagnostics-Performance/Operational",
            "/q:*[System[(EventID=100)]]",
            "/c:1",
            "/rd:true",
            "/f:xml",
        ])
        .output();
    let Ok(out) = out else { return t };
    let xml = String::from_utf8_lossy(&out.stdout);
    let logged = xml
        .find("SystemTime=")
        .and_then(|i| parse_iso(&xml[i + 12..]));
    // only trust an event written after this boot began
    if logged.is_some_and(|l| l >= boot_ts) {
        let get = |n| xml_data(&xml, n).and_then(|v| v.parse().ok());
        t.duration_ms = get("BootTime");
        t.main_path_ms = get("MainPathBootTime");
        t.post_boot_ms = get("BootPostBootTime");
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_parse() {
        assert_eq!(parse_iso("1970-01-02T00:00:01.5Z"), Some(86_401));
        assert_eq!(parse_iso("2000-03-01T00:00:00Z"), Some(951_868_800));
    }

    #[test]
    fn xml_field() {
        assert_eq!(
            xml_data("<Data Name=\"BootTime\">1234</Data>", "BootTime").as_deref(),
            Some("1234")
        );
    }
}
