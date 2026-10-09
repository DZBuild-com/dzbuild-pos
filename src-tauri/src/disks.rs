//! Drives and free space (sysinfo) for the backup screens, the fatal screen and the diagnostic.

use serde::Serialize;
use specta::Type;
use std::path::Path;
use sysinfo::Disks;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemovableDrive {
    /// `E:\`
    pub mount: String,
    /// The volume name Windows shows ("KINGSTON"), empty when it has none.
    pub label: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

pub fn removable_drives() -> Vec<RemovableDrive> {
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|d| d.is_removable())
        .map(|d| RemovableDrive {
            mount: d.mount_point().to_string_lossy().into_owned(),
            label: d.name().to_string_lossy().into_owned(),
            free_bytes: d.available_space(),
            total_bytes: d.total_space(),
        })
        .collect()
}

/// Free bytes on the drive that holds `path` (the longest mount point it starts with).
pub fn free_space(path: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}
