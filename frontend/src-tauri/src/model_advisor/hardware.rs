//! What this machine has, read at run time.

use std::path::Path;

use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    /// Installed memory rounded to whole GB (8, 16, 24…).
    pub ram_gb: u32,
    pub total_memory_bytes: u64,
    /// Name of the processor as the system reports it ("Apple M2"…). Empty when unknown.
    pub chip: String,
    pub apple_silicon: bool,
    pub cpu_cores: u32,
    /// Free space on the disk that holds the models. `None` when unknown.
    pub free_disk_bytes: Option<u64>,
}

/// 8 GB machines report a little under 8 GiB: round to the nearest GB.
pub fn nominal_gb(total_memory_bytes: u64) -> u32 {
    ((total_memory_bytes as f64) / (1024.0 * 1024.0 * 1024.0)).round() as u32
}

/// Free space on the disk whose mount point is the longest prefix of `path`.
fn free_space_at(path: &Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space())
}

/// `models_dir` is where the app stores its models (from the Tauri path API).
pub fn detect(models_dir: Option<&Path>) -> Hardware {
    let system = System::new_with_specifics(
        RefreshKind::new()
            .with_memory(MemoryRefreshKind::new().with_ram())
            .with_cpu(CpuRefreshKind::new()),
    );
    let total_memory_bytes = system.total_memory();
    let chip = system.cpus().first().map(|cpu| cpu.brand().trim().to_string()).unwrap_or_default();
    Hardware {
        ram_gb: nominal_gb(total_memory_bytes),
        total_memory_bytes,
        chip,
        apple_silicon: cfg!(all(target_os = "macos", target_arch = "aarch64")),
        cpu_cores: system.cpus().len() as u32,
        free_disk_bytes: models_dir.and_then(free_space_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_rounded_to_the_size_on_the_box() {
        assert_eq!(nominal_gb(8 * 1024 * 1024 * 1024), 8);
        assert_eq!(nominal_gb(8_589_934_592 - 300_000_000), 8);
        assert_eq!(nominal_gb(17_179_869_184), 16);
        assert_eq!(nominal_gb(25_769_803_776), 24);
        assert_eq!(nominal_gb(0), 0);
    }

    #[test]
    fn this_machine_reports_memory_and_processor() {
        let hardware = detect(Some(&std::env::temp_dir()));
        assert!(hardware.ram_gb >= 1);
        assert!(hardware.cpu_cores >= 1);
        assert!(hardware.free_disk_bytes.is_some());
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(hardware.apple_silicon && hardware.chip.contains("Apple"));
    }
}
