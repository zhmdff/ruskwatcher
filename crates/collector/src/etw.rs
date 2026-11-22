//! Per-process disk and network bytes from the NT Kernel Logger (needs admin / LocalSystem).
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use windows::core::{GUID, PWSTR};
use windows::Win32::System::Diagnostics::Etw::*;

#[derive(Default, Clone, Copy)]
pub struct IoCounters {
    pub disk_read: u64,
    pub disk_write: u64,
    pub net_in: u64,
    pub net_out: u64,
}

#[derive(Default)]
struct State {
    per_pid: HashMap<u32, IoCounters>,
    /// thread id -> owning pid, so disk I/O can be charged to the issuing process
    threads: HashMap<u32, u32>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

const GUID_DISKIO: GUID = GUID::from_u128(0x3d6fa8d4_fe05_11d0_9dda_00c04fd7ba7c);
const GUID_THREAD: GUID = GUID::from_u128(0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c);
const GUID_TCPIP: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
const GUID_UDPIP: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);

const ERROR_ALREADY_EXISTS: u32 = 183;
const NAME_BYTES: usize = 2 * 64;

fn u32_at(d: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(off..off + 4)?.try_into().ok()?))
}

unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
    let rec = &*rec;
    let guid = rec.EventHeader.ProviderId;
    let op = rec.EventHeader.EventDescriptor.Opcode;
    let data: &[u8] = if rec.UserData.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(rec.UserData as *const u8, rec.UserDataLength as usize)
    };
    let Ok(mut st) = state().lock() else { return };

    if guid == GUID_THREAD {
        // Start(1) / DCStart(3): ProcessId, ThreadId; End(2) / DCEnd(4)
        if let (Some(pid), Some(tid)) = (u32_at(data, 0), u32_at(data, 4)) {
            match op {
                1 | 3 => {
                    st.threads.insert(tid, pid);
                }
                2 | 4 => {
                    st.threads.remove(&tid);
                }
                _ => {}
            }
        }
    } else if guid == GUID_DISKIO && (op == 10 || op == 11) {
        // DiskNumber, IrpFlags, TransferSize ... IssuingThreadId at 40
        let (Some(size), Some(tid)) = (u32_at(data, 8), u32_at(data, 40)) else {
            return;
        };
        let pid = st
            .threads
            .get(&tid)
            .copied()
            .unwrap_or(rec.EventHeader.ProcessId);
        let c = st.per_pid.entry(pid).or_default();
        if op == 10 {
            c.disk_read += size as u64;
        } else {
            c.disk_write += size as u64;
        }
    } else if guid == GUID_TCPIP || guid == GUID_UDPIP {
        // send = 10 (v4) / 26 (v6), recv = 11 / 27; payload starts with PID, size
        let (Some(pid), Some(size)) = (u32_at(data, 0), u32_at(data, 4)) else {
            return;
        };
        let c = st.per_pid.entry(pid).or_default();
        match op {
            10 | 26 => c.net_out += size as u64,
            11 | 27 => c.net_in += size as u64,
            _ => {}
        }
    }
}

fn props_buf() -> Vec<u8> {
    let size = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + NAME_BYTES;
    let mut buf = vec![0u8; size];
    let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
    unsafe {
        (*p).Wnode.BufferSize = size as u32;
        (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        (*p).Wnode.ClientContext = 1;
        (*p).Wnode.Guid = SystemTraceControlGuid;
        (*p).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        (*p).EnableFlags = EVENT_TRACE_FLAG(
            EVENT_TRACE_FLAG_DISK_IO.0
                | EVENT_TRACE_FLAG_NETWORK_TCPIP.0
                | EVENT_TRACE_FLAG_THREAD.0,
        );
        (*p).LoggerNameOffset = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    }
    buf
}

pub struct Etw {
    session: CONTROLTRACE_HANDLE,
}

impl Etw {
    /// Fails (typically access denied) when not elevated; callers should carry on without it.
    pub fn start() -> Result<Self, windows::core::Error> {
        let mut session = CONTROLTRACE_HANDLE::default();
        let mut buf = props_buf();
        let mut err = unsafe {
            StartTraceW(
                &mut session,
                KERNEL_LOGGER_NAMEW,
                buf.as_mut_ptr() as *mut _,
            )
        };
        if err.0 == ERROR_ALREADY_EXISTS {
            // leftover session from a crashed run
            let mut stop = props_buf();
            unsafe {
                let _ = ControlTraceW(
                    CONTROLTRACE_HANDLE::default(),
                    KERNEL_LOGGER_NAMEW,
                    stop.as_mut_ptr() as *mut _,
                    EVENT_TRACE_CONTROL_STOP,
                );
            }
            buf = props_buf();
            err = unsafe {
                StartTraceW(
                    &mut session,
                    KERNEL_LOGGER_NAMEW,
                    buf.as_mut_ptr() as *mut _,
                )
            };
        }
        err.ok()?;

        #[allow(clippy::field_reassign_with_default)]
        std::thread::spawn(|| unsafe {
            let mut name = KERNEL_LOGGER_NAMEW.as_wide().to_vec();
            name.push(0);
            let mut log = EVENT_TRACE_LOGFILEW::default();
            log.LoggerName = PWSTR(name.as_mut_ptr());
            log.Anonymous1.ProcessTraceMode =
                PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
            log.Anonymous2.EventRecordCallback = Some(on_event);
            let h = OpenTraceW(&mut log);
            if h.Value == u64::MAX {
                eprintln!("ETW: OpenTrace failed");
                return;
            }
            let _ = ProcessTrace(&[h], None, None); // blocks until the session stops
            let _ = CloseTrace(h);
        });
        Ok(Self { session })
    }

    /// Bytes per pid since the previous call.
    pub fn drain(&self) -> HashMap<u32, IoCounters> {
        state()
            .lock()
            .map(|mut s| std::mem::take(&mut s.per_pid))
            .unwrap_or_default()
    }
}

impl Drop for Etw {
    fn drop(&mut self) {
        let mut buf = props_buf();
        unsafe {
            let _ = ControlTraceW(
                self.session,
                KERNEL_LOGGER_NAMEW,
                buf.as_mut_ptr() as *mut _,
                EVENT_TRACE_CONTROL_STOP,
            );
        }
    }
}
