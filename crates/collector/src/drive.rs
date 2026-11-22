//! Physical disks: NVMe health log, SATA/SAS kind (SSD vs HDD) and ATA SMART.
use std::hash::{Hash, Hasher};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;

const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;
// FILE_ANY_ACCESS, so it works on the query-only handle (LENGTH_INFO needs read access)
const IOCTL_DISK_GET_DRIVE_GEOMETRY_EX: u32 = 0x0007_00A0;
const SMART_RCV_DRIVE_DATA: u32 = 0x0007_C088;
const BUS_ATA: u32 = 3;
const BUS_SAS: u32 = 10;
const BUS_SATA: u32 = 11;
const BUS_NVME: u32 = 17;

pub struct DriveHealth {
    pub model: String,
    pub serial_hash: String,
    /// "nvme", "ssd" or "hdd"
    pub kind: String,
    pub capacity_bytes: u64,
    pub temp_c: Option<f64>,
    pub percent_used: Option<f64>,
    pub host_writes_bytes: Option<u64>,
    pub host_reads_bytes: Option<u64>,
    pub available_spare: Option<f64>,
    pub media_errors: Option<u64>,
    pub power_on_hours: Option<u64>,
}

fn ioctl(h: HANDLE, code: u32, input: &[u8], out: &mut [u8]) -> bool {
    let mut ret = 0u32;
    unsafe {
        DeviceIoControl(
            h,
            code,
            Some(input.as_ptr() as *const _),
            input.len() as u32,
            Some(out.as_mut_ptr() as *mut _),
            out.len() as u32,
            Some(&mut ret),
            None,
        )
        .is_ok()
    }
}

fn u128_at(d: &[u8], off: usize) -> u128 {
    u128::from_le_bytes(d[off..off + 16].try_into().unwrap())
}

fn u32_at(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(d[off..off + 4].try_into().unwrap())
}

fn cstr_at(d: &[u8], off: usize) -> String {
    if off == 0 || off >= d.len() {
        return String::new();
    }
    let end = d[off..]
        .iter()
        .position(|&b| b == 0)
        .map_or(d.len(), |p| off + p);
    String::from_utf8_lossy(&d[off..end]).trim().to_string()
}

fn open(index: u32, access: u32) -> Option<HANDLE> {
    let path = HSTRING::from(format!(r"\\.\PhysicalDrive{index}"));
    unsafe {
        CreateFileW(
            &path,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()
}

fn nvme_log(h: HANDLE) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 8 + 40 + 512];
    buf[0..4].copy_from_slice(&50u32.to_le_bytes()); // StorageDeviceProtocolSpecificProperty
    buf[8..12].copy_from_slice(&3u32.to_le_bytes()); // ProtocolTypeNvme
    buf[12..16].copy_from_slice(&2u32.to_le_bytes()); // NVMeDataTypeLogPage
    buf[16..20].copy_from_slice(&2u32.to_le_bytes()); // log page 2 = SMART/health
    buf[24..28].copy_from_slice(&40u32.to_le_bytes()); // data offset
    buf[28..32].copy_from_slice(&512u32.to_le_bytes()); // data length
    let input = buf.clone();
    ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, &input, &mut buf).then(|| buf[48..].to_vec())
}

/// ATA SMART attribute sector (needs a read/write handle, so admin or LocalSystem).
fn ata_smart(index: u32) -> Option<Vec<u8>> {
    let h = open(index, 0xC000_0000)?; // GENERIC_READ | GENERIC_WRITE
                                       // SENDCMDINPARAMS: cBufferSize(4) + IDEREGS(8) + bDriveNumber(1) + reserved(3) + dwReserved(16) + 1 data byte
    let mut input = [0u8; 33];
    input[0..4].copy_from_slice(&512u32.to_le_bytes());
    input[4] = 0xD0; // features: READ ATTRIBUTES
    input[5] = 1; // sector count
    input[6] = 1; // sector number
    input[7] = 0x4F; // cyl low (SMART signature)
    input[8] = 0xC2; // cyl high
    input[9] = 0xA0 | ((index as u8 & 1) << 4);
    input[10] = 0xB0; // SMART command
    input[12] = index as u8;
    let mut out = vec![0u8; 16 + 512];
    let ok = ioctl(h, SMART_RCV_DRIVE_DATA, &input, &mut out);
    unsafe { CloseHandle(h).ok() };
    // SENDCMDOUTPARAMS: cBufferSize(4) + DRIVERSTATUS(12), then the 512-byte sector
    (ok && out[4] == 0).then(|| out[16..].to_vec())
}

/// Raw 6 bytes of SMART attribute `id`; entries are 12 bytes each starting at offset 2.
fn smart_attr(t: &[u8], id: u8) -> Option<[u8; 6]> {
    (0..30)
        .map(|i| &t[2 + i * 12..2 + (i + 1) * 12])
        .find(|e| e[0] == id)
        .map(|e| e[5..11].try_into().unwrap())
}

fn raw_u64(raw: [u8; 6]) -> u64 {
    raw.iter().rev().fold(0u64, |acc, &b| (acc << 8) | b as u64)
}

fn query(index: u32) -> Option<DriveHealth> {
    let h = open(index, 0)?;
    let out = (|| {
        // device descriptor: StorageDeviceProperty, PropertyStandardQuery
        let q = [0u8; 12];
        let mut desc = [0u8; 1024];
        if !ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, &q, &mut desc) {
            return None;
        }
        let bus = u32_at(&desc, 28);
        if ![BUS_NVME, BUS_ATA, BUS_SAS, BUS_SATA].contains(&bus) {
            return None; // USB sticks, virtual disks, ...
        }
        let model = cstr_at(&desc, u32_at(&desc, 16) as usize);
        let serial = cstr_at(&desc, u32_at(&desc, 24) as usize);
        // shortcut: DefaultHasher is stable only per toolchain; swap for sha256 if ids must survive upgrades
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (model.as_str(), serial.as_str()).hash(&mut hasher);

        // DISK_GEOMETRY_EX: 24-byte geometry, then DiskSize
        let mut geo = [0u8; 128];
        let capacity = if ioctl(h, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], &mut geo) {
            i64::from_le_bytes(geo[24..32].try_into().unwrap()) as u64
        } else {
            0
        };
        let mut d = DriveHealth {
            model,
            serial_hash: format!("{:016x}", hasher.finish()),
            kind: String::new(),
            capacity_bytes: capacity,
            temp_c: None,
            percent_used: None,
            host_writes_bytes: None,
            host_reads_bytes: None,
            available_spare: None,
            media_errors: None,
            power_on_hours: None,
        };

        if bus == BUS_NVME {
            d.kind = "nvme".into();
            let log = nvme_log(h)?;
            d.temp_c = Some(u16::from_le_bytes([log[1], log[2]]) as f64 - 273.15);
            d.available_spare = Some(log[3] as f64);
            d.percent_used = Some(log[5] as f64);
            d.host_reads_bytes = Some((u128_at(&log, 32) * 512_000).min(u64::MAX as u128) as u64);
            d.host_writes_bytes = Some((u128_at(&log, 48) * 512_000).min(u64::MAX as u128) as u64);
            d.power_on_hours = Some(u128_at(&log, 128) as u64);
            d.media_errors = Some(u128_at(&log, 160) as u64);
        } else {
            // StorageDeviceSeekPenaltyProperty (7): rotating disks incur a seek penalty
            let mut sq = [0u8; 12];
            sq[0..4].copy_from_slice(&7u32.to_le_bytes());
            let mut sp = [0u8; 16];
            let solid = ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, &sq, &mut sp) && sp[8] == 0;
            d.kind = if solid { "ssd" } else { "hdd" }.into();
            if let Some(t) = ata_smart(index) {
                // 194 temperature (low byte), 9 power-on hours, 5 reallocated + 197 pending sectors
                d.temp_c = smart_attr(&t, 194).map(|r| r[0] as f64);
                d.power_on_hours = smart_attr(&t, 9).map(|r| raw_u64(r) & 0xFFFF_FFFF);
                d.media_errors = match (smart_attr(&t, 5), smart_attr(&t, 197)) {
                    (None, None) => None,
                    (a, b) => Some(a.map_or(0, raw_u64) + b.map_or(0, raw_u64)),
                };
            }
        }
        Some(d)
    })();
    unsafe { CloseHandle(h).ok() };
    out
}

/// Every NVMe/SATA/SAS physical disk. Health fields stay None when SMART is unreadable.
pub fn drives() -> Vec<DriveHealth> {
    (0..8).filter_map(query).collect()
}
