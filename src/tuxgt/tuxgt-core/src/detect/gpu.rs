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
        let Some(id) = pci_id_file(&ent.path().join("device/vendor")) else {
            continue;
        };
        match id {
            0x10de => gpu.nvidia = true,
            0x1002 => gpu.amd = true,
            0x8086 => gpu.intel = true,
            _ => {}
        }
    }
    gpu
}

/// One DRM card with its PCI identity (`device/vendor` + `device/device`
/// under `/sys/class/drm/card*/`). Unlike `HostGpu` (vendor flags for Env
/// Advance), this names a concrete adapter so the Settings GPU chooser can
/// persist a choice (`ui.toml gpu`) that steers first-window adapter
/// selection via `ZED_DEVICE_ID`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuCard {
    pub vendor: u32,
    pub device: u32,
}

impl GpuCard {
    /// Full `vvvv:dddd` lowercase hex identity for display. This is not the
    /// stored `ui.toml gpu` value, which is the device id alone (`dddd`).
    pub fn pref_id(self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.device)
    }
}

/// Display name for a PCI vendor id. Proper nouns, never translated.
pub fn vendor_name(vendor: u32) -> &'static str {
    match vendor {
        0x10de => "NVIDIA",
        0x1002 => "AMD",
        0x8086 => "Intel",
        _ => "Unknown",
    }
}

/// Concrete host GPUs, deduplicated and in stable `(vendor, device)` order.
/// Cards missing either id file are skipped; missing sysfs is an empty list
/// (unknown host, not an error).
pub fn host_gpus() -> Vec<GpuCard> {
    host_gpus_from(Path::new("/sys/class/drm"))
}

pub fn host_gpus_from(drm: &Path) -> Vec<GpuCard> {
    let mut cards = Vec::new();
    let Ok(rd) = fs::read_dir(drm) else {
        return cards;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let n = name.to_string_lossy();
        if !n.starts_with("card") {
            continue;
        }
        if n.len() <= 4 || !n[4..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let dev = ent.path().join("device");
        let (Some(vendor), Some(device)) = (
            pci_id_file(&dev.join("vendor")),
            pci_id_file(&dev.join("device")),
        ) else {
            continue;
        };
        let card = GpuCard { vendor, device };
        if !cards.contains(&card) {
            cards.push(card);
        }
    }
    cards.sort_by_key(|c| (c.vendor, c.device));
    cards
}

/// Parse a sysfs PCI id file (`0x10de`, bare hex, any case; 16-bit).
fn pci_id_file(path: &Path) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    let id = text.trim();
    let id = id
        .strip_prefix("0x")
        .or_else(|| id.strip_prefix("0X"))
        .unwrap_or(id);
    if id.is_empty() || id.len() > 4 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(id, 16).ok()
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

    #[test]
    fn cards_list_vendor_device_pairs() {
        let root = std::env::temp_dir().join(format!(
            "tuxgt-gpucards-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        write(&root.join("card0/device/vendor"), "0x10de\n");
        write(&root.join("card0/device/device"), "0x13c0\n");
        write(&root.join("card1/device/vendor"), "0x1002\n");
        write(&root.join("card1/device/device"), "0x73df\n");
        // Missing device id: skipped here, still counts for vendor flags.
        write(&root.join("card2/device/vendor"), "0x8086\n");
        write(&root.join("card0-DP-1/device/vendor"), "0x8086\n");
        write(&root.join("card0-DP-1/device/device"), "0x1234\n");
        write(&root.join("renderD128/device/vendor"), "0x8086\n");
        write(&root.join("renderD128/device/device"), "0x1234\n");
        let cards = host_gpus_from(&root);
        assert_eq!(
            cards,
            vec![
                GpuCard {
                    vendor: 0x1002,
                    device: 0x73df
                },
                GpuCard {
                    vendor: 0x10de,
                    device: 0x13c0
                },
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cards_dedupe_identical_pairs() {
        let root = std::env::temp_dir().join(format!(
            "tuxgt-gpudedupe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        write(&root.join("card0/device/vendor"), "0x10de\n");
        write(&root.join("card0/device/device"), "0x13c0\n");
        write(&root.join("card1/device/vendor"), "0x10de\n");
        write(&root.join("card1/device/device"), "0x13c0\n");
        let cards = host_gpus_from(&root);
        assert_eq!(
            cards,
            vec![GpuCard {
                vendor: 0x10de,
                device: 0x13c0
            }]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cards_missing_sysfs_is_empty() {
        let root = std::env::temp_dir().join(format!(
            "tuxgt-gpumissing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(host_gpus_from(&root).is_empty());
    }

    #[test]
    fn vendor_names_and_pref_id() {
        assert_eq!(vendor_name(0x10de), "NVIDIA");
        assert_eq!(vendor_name(0x1002), "AMD");
        assert_eq!(vendor_name(0x8086), "Intel");
        assert_eq!(vendor_name(0x1234), "Unknown");
        assert_eq!(
            GpuCard {
                vendor: 0x10de,
                device: 0x13c0
            }
            .pref_id(),
            "10de:13c0"
        );
    }
}
