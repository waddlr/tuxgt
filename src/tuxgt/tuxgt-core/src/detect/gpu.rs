use std::fs;
use std::path::Path;

/// PCI vendors seen on `/sys/class/drm/card*/device/vendor`.
/// Hybrid is every bit that is set. All-false is unknown (sysfs missing).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostGpu {
    pub nvidia: bool,
    pub amd: bool,
    pub intel: bool,
}

impl HostGpu {
    pub fn unknown(self) -> bool {
        !self.nvidia && !self.amd && !self.intel
    }
}

/// Host GPU vendors via DRM sysfs (`10de` nvidia, `1002` amd, `8086` intel).
pub fn host_gpu() -> HostGpu {
    host_gpu_from(Path::new("/sys/class/drm"))
}

pub fn host_gpu_from(drm: &Path) -> HostGpu {
    let mut gpu = HostGpu::default();
    let Ok(rd) = fs::read_dir(drm) else {
        return gpu;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let n = name.to_string_lossy();
        if !n.starts_with("card") {
            continue;
        }
        if !n[4..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(ent.path().join("device/vendor")) else {
            continue;
        };
        let id = text.trim().trim_start_matches("0x").to_ascii_lowercase();
        match id.as_str() {
            "10de" => gpu.nvidia = true,
            "1002" => gpu.amd = true,
            "8086" => gpu.intel = true,
            _ => {}
        }
    }
    gpu
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, b: &str) {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(p, b).unwrap();
    }

    #[test]
    fn vendors_from_card_sysfs() {
        let root = std::env::temp_dir().join(format!(
            "tuxgt-gpu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        write(&root.join("card0/device/vendor"), "0x10de\n");
        write(&root.join("card1/device/vendor"), "0x1002\n");
        write(&root.join("card0-HDMI-A-1/device/vendor"), "0x8086\n");
        write(&root.join("renderD128/device/vendor"), "0x8086\n");
        let gpu = host_gpu_from(&root);
        assert!(gpu.nvidia);
        assert!(gpu.amd);
        assert!(!gpu.intel);
        assert!(!gpu.unknown());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_sysfs_is_unknown() {
        let root = std::env::temp_dir().join(format!(
            "tuxgt-gpu-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(host_gpu_from(&root).unknown());
    }
}
