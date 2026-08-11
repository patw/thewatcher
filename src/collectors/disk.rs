//! Disk/filesystem metric collector.

use crate::model::*;

use super::{ok_status, CollectorContext};

/// Pseudo-filesystem prefixes to suppress by default
const PSEUDO_FS_PREFIXES: &[&str] = &[
    "/sys", "/proc", "/dev", "/run", "/snap",
];

pub fn collect(_ctx: &mut CollectorContext, statuses: &mut Vec<CollectorStatus>) -> Vec<DiskSnapshot> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut disks = Vec::new();

    // sysinfo 0.34: use Disks
    let sys_disks = sysinfo::Disks::new_with_refreshed_list();

    for disk in sys_disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();

        // Skip pseudo-filesystems
        if PSEUDO_FS_PREFIXES.iter().any(|p| mount.starts_with(p)) {
            continue;
        }

        let total_bytes = disk.total_space();
        let available_bytes = disk.available_space();
        let used_bytes = total_bytes.saturating_sub(available_bytes);
        let used_percent = if total_bytes > 0 {
            (used_bytes as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };

        let filesystem = Some(disk.name().to_string_lossy().to_string());

        disks.push(DiskSnapshot {
            mount,
            filesystem,
            total_bytes,
            used_bytes,
            available_bytes,
            used_percent,
        });
    }

    // If we have no disks, try fallback: on Linux, read /proc/mounts
    #[cfg(target_os = "linux")]
    if disks.is_empty() {
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 2 {
                    continue;
                }
                let device = parts[0];
                let mount = parts[1];
                let fs_type = parts.get(2).copied().unwrap_or("unknown");

                // Skip pseudo
                if PSEUDO_FS_PREFIXES.iter().any(|p| mount.starts_with(p)) {
                    continue;
                }
                if fs_type == "tmpfs" || fs_type == "devtmpfs" || fs_type == "squashfs" {
                    continue;
                }
                // Only real block devices or well-known fs types
                if !device.starts_with("/dev/") && fs_type != "zfs" && fs_type != "btrfs"
                    && fs_type != "xfs" && fs_type != "ext4" && fs_type != "ext3"
                    && fs_type != "ext2" && fs_type != "ntfs" && fs_type != "vfat"
                    && fs_type != "exfat" && fs_type != "fuseblk"
                {
                    continue;
                }

                // Get stats via statvfs
                if let Some(stat) = nix_statvfs(mount) {
                    disks.push(DiskSnapshot {
                        mount: mount.to_string(),
                        filesystem: Some(device.to_string()),
                        total_bytes: stat.total_bytes,
                        used_bytes: stat.total_bytes.saturating_sub(stat.available_bytes),
                        available_bytes: stat.available_bytes,
                        used_percent: if stat.total_bytes > 0 {
                            ((stat.total_bytes - stat.available_bytes) as f64
                                / stat.total_bytes as f64)
                                * 100.0
                        } else {
                            0.0
                        },
                    });
                }
            }
        }
    }

    statuses.push(ok_status("disk", now_ms));

    disks
}

#[cfg(target_os = "linux")]
struct StatvfsResult {
    total_bytes: u64,
    available_bytes: u64,
}

#[cfg(target_os = "linux")]
fn nix_statvfs(mount: &str) -> Option<StatvfsResult> {
    use std::ffi::CString;
    use std::mem;

    let c_path = CString::new(mount).ok()?;
    let mut buf: libc::statvfs = unsafe { mem::zeroed() };

    let ret = unsafe { libc::statvfs(c_path.as_ptr(), &mut buf) };
    if ret != 0 {
        return None;
    }

    let block_size = buf.f_frsize as u64;
    let total_bytes = buf.f_blocks * block_size;
    let available_bytes = buf.f_bavail * block_size;

    Some(StatvfsResult {
        total_bytes,
        available_bytes,
    })
}
