//! Stored GPU choice for the first window (`ui.toml gpu`).
//!
//! Adapter selection is not the app's: the window platform picks the adapter
//! that can configure and present the surface, and its sort treats
//! `ZED_DEVICE_ID` as the *top* preference while still walking the rest of the
//! list when the named adapter fails that test. So a stored card is applied as
//! a device filter — a preference that steers selection, never a hard
//! exclusion — and the system default sets no filter at all. `Cpu` instead
//! pins software rendering for the first window; its environment is restored
//! once that window owns its renderer, so children never inherit it.
use std::env;

use tuxgt_core::{data_dir, GpuCard};

/// System default, one host card by PCI device id, or software rendering.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuPref {
    /// No filter. The platform's own priority (compositor GPU, device type,
    /// backend) decides. The shipped default, and what an empty or unreadable
    /// stored value falls back to.
    #[default]
    SystemDefault,
    /// One card. `ui.toml` stores the PCI device id (`13c0`), which is the
    /// form `ZED_DEVICE_ID` parses.
    Card(u32),
    /// Software rendering (`ui.toml` stores `cpu`). No real Vulkan ICDs and
    /// software GL, so the platform lands on llvmpipe whatever hardware is
    /// present. Chosen, never automatic: nothing here downgrades a window
    /// that could use hardware.
    Cpu,
}

impl GpuPref {
    /// Read a stored `ui.toml gpu` value. `""`, `system`, and anything
    /// unparseable read as the system default; `cpu` reads as software
    /// rendering; `13c0` and `0x13c0` read as that card.
    pub fn parse(stored: &str) -> Self {
        let id = stored.trim();
        if id.is_empty() || id == "system" {
            return Self::SystemDefault;
        }
        if id == "cpu" {
            return Self::Cpu;
        }
        let id = id
            .strip_prefix("0x")
            .or_else(|| id.strip_prefix("0X"))
            .unwrap_or(id);
        match u32::from_str_radix(id, 16) {
            Ok(id) if id <= 0xffff => Self::Card(id),
            _ => Self::SystemDefault,
        }
    }

    /// Stored form: empty for the system default (the key is then omitted
    /// from `ui.toml`), `cpu` for software rendering, the 4-digit device id
    /// for a card.
    pub fn pref_id(self) -> String {
        match self {
            Self::SystemDefault => String::new(),
            Self::Card(id) => format!("{id:04x}"),
            Self::Cpu => "cpu".into(),
        }
    }

    /// Whether a failed first-window open under this stored choice reverts
    /// the choice: a named card, or software rendering, that cannot present
    /// must not brick the next start, while the system default has nothing
    /// to revert.
    pub fn revert_on_failure(self) -> bool {
        matches!(self, Self::Card(_) | Self::Cpu)
    }

    /// Publish the device filter the platform reads when it selects the first
    /// window's adapter. `ZED_DEVICE_ID` is the environment the locked
    /// platform crate already reads, so the app owns the value it stores and
    /// sets it before the first window instead of the user. An inherited
    /// value is left alone when no card is stored: a card set here, or in the
    /// environment, wins as the top preference over the rest of the adapter
    /// list, so an explicit user filter is never silently discarded.
    ///
    /// Returns the saved environment for the software-rendering choice: drop
    /// it once the first window owns its renderer so no child inherits the
    /// forced vars. Empty for the other choices.
    pub fn apply(self) -> CpuEnvRestore {
        if self == Self::Cpu {
            return Self::apply_cpu();
        }
        const KEY: &str = "ZED_DEVICE_ID";
        let value = self.pref_id();
        if value.is_empty() && env::var(KEY).is_ok_and(|v| !v.trim().is_empty()) {
            return CpuEnvRestore::empty();
        }
        if value.is_empty() {
            env::remove_var(KEY);
        } else {
            env::set_var(KEY, &value);
        }
        tracing::info!(gpu = %value, "GPU preference applied for first window");
        CpuEnvRestore::empty()
    }

    /// Pin software rendering for the first window: no real Vulkan ICDs, so
    /// the platform's Vulkan enumeration is empty, and software GL, so it
    /// lands on llvmpipe. A stored device filter is removed with it: under
    /// software rendering there is no named adapter to prefer. Prior values
    /// are saved for the restore guard.
    fn apply_cpu() -> CpuEnvRestore {
        // Never created: the loader treats it as no drivers installed, which
        // is the outcome (an empty `VK_ICD_FILENAMES` would instead mean
        // "use the default drivers").
        let absent = data_dir().join("absent-vulkan-icd.json");
        let absent = absent.to_string_lossy().into_owned();
        let mut saved = Vec::new();
        for (key, value) in [
            ("LIBGL_ALWAYS_SOFTWARE", "1"),
            ("GALLIUM_DRIVER", "llvmpipe"),
            (
                "__EGL_VENDOR_LIBRARY_FILENAMES",
                "/usr/share/glvnd/egl_vendor.d/50_mesa.json",
            ),
            ("VK_ICD_FILENAMES", absent.as_str()),
            ("VK_DRIVER_FILES", absent.as_str()),
        ] {
            saved.push((key, env::var_os(key)));
            env::set_var(key, value);
        }
        saved.push(("ZED_DEVICE_ID", env::var_os("ZED_DEVICE_ID")));
        env::remove_var("ZED_DEVICE_ID");
        tracing::info!(gpu = "cpu", "GPU preference applied for first window");
        CpuEnvRestore { saved }
    }
}

/// Saved process environment behind a CPU software-rendering choice.
///
/// The forced vars steer first-window adapter selection only; the same vars
/// would cripple any child that does GL/Vulkan (games above all), so the
/// guard restores the prior values once the first window owns its renderer.
/// Restores on drop.
pub struct CpuEnvRestore {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl CpuEnvRestore {
    fn empty() -> Self {
        Self { saved: Vec::new() }
    }
}

impl Drop for CpuEnvRestore {
    fn drop(&mut self) {
        for (key, prev) in self.saved.drain(..) {
            match prev {
                Some(v) => env::set_var(key, v),
                None => env::remove_var(key),
            }
        }
    }
}

/// Painted label for a stored card. A card that is not in the current
/// detection list keeps its id on screen: falling back to a label that reads
/// like the system default would hide a choice that is still in effect.
pub fn card_label(id: u32, cards: &[GpuCard]) -> String {
    match cards.iter().find(|c| c.device == id) {
        Some(card) => format!(
            "{} · {}",
            tuxgt_core::vendor_name(card.vendor),
            card.pref_id()
        ),
        None => format!("Not detected · {id:04x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serializes the tests that mutate process render env; disjoint from the
    // `TUXGT_CONFIG` lock in `prefs.rs`, which touches no render var.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn cards() -> Vec<GpuCard> {
        vec![
            GpuCard {
                vendor: 0x10de,
                device: 0x13c0,
            },
            GpuCard {
                vendor: 0x1002,
                device: 0x73df,
            },
        ]
    }

    #[test]
    fn system_default_is_the_empty_value() {
        assert_eq!(GpuPref::parse(""), GpuPref::SystemDefault);
        assert_eq!(GpuPref::default(), GpuPref::SystemDefault);
        assert_eq!(GpuPref::SystemDefault.pref_id(), "");
    }

    #[test]
    fn card_reads_forms_and_round_trips() {
        for stored in ["13c0", "0x13c0", "0X13c0", "13C0", " 13c0 "] {
            assert_eq!(GpuPref::parse(stored), GpuPref::Card(0x13c0), "{stored}");
        }
        assert_eq!(GpuPref::Card(0x13c0).pref_id(), "13c0");
        let stored = GpuPref::Card(0x73df).pref_id();
        assert_eq!(GpuPref::parse(&stored), GpuPref::Card(0x73df));
    }

    #[test]
    fn unparseable_values_fall_back_to_system_default() {
        for stored in ["system", "zz", "13c0z", "123456", "0x", "-1"] {
            assert_eq!(GpuPref::parse(stored), GpuPref::SystemDefault, "{stored}");
        }
    }

    #[test]
    fn cpu_reads_exact_lowercase_and_round_trips() {
        assert_eq!(GpuPref::parse("cpu"), GpuPref::Cpu);
        assert_eq!(GpuPref::parse("  cpu "), GpuPref::Cpu);
        assert_eq!(GpuPref::Cpu.pref_id(), "cpu");
        assert_eq!(GpuPref::parse(&GpuPref::Cpu.pref_id()), GpuPref::Cpu);
        for stored in ["CPU", "Cpu", "cpus", "0xcpu"] {
            assert_eq!(GpuPref::parse(stored), GpuPref::SystemDefault, "{stored}");
        }
    }

    #[test]
    fn only_a_named_choice_reverts_after_open_failure() {
        assert!(GpuPref::Card(0x13c0).revert_on_failure());
        assert!(GpuPref::Cpu.revert_on_failure());
        assert!(!GpuPref::SystemDefault.revert_on_failure());
        assert!(!GpuPref::parse("zz").revert_on_failure());
    }

    #[test]
    fn cpu_apply_pins_software_rendering_and_restores_on_drop() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        const KEYS: [&str; 6] = [
            "LIBGL_ALWAYS_SOFTWARE",
            "GALLIUM_DRIVER",
            "__EGL_VENDOR_LIBRARY_FILENAMES",
            "VK_ICD_FILENAMES",
            "VK_DRIVER_FILES",
            "ZED_DEVICE_ID",
        ];
        let prev: Vec<Option<std::ffi::OsString>> =
            KEYS.iter().map(|k| std::env::var_os(k)).collect();
        // An inherited device filter plus a pinned software var: both must
        // come back after the guard drops.
        std::env::set_var("ZED_DEVICE_ID", "2684");
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "0");

        let guard = GpuPref::Cpu.apply();
        assert_eq!(std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref(), Ok("1"));
        assert_eq!(std::env::var("GALLIUM_DRIVER").as_deref(), Ok("llvmpipe"));
        assert!(std::env::var_os("ZED_DEVICE_ID").is_none());
        let icd = std::env::var("VK_ICD_FILENAMES").expect("ICD override");
        assert_eq!(
            std::env::var("VK_DRIVER_FILES").as_deref(),
            Ok(icd.as_str())
        );
        assert!(
            !std::path::Path::new(&icd).exists(),
            "ICD path stays absent"
        );
        drop(guard);

        assert_eq!(std::env::var("ZED_DEVICE_ID").as_deref(), Ok("2684"));
        assert_eq!(std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref(), Ok("0"));
        for (key, was) in KEYS.iter().zip(prev) {
            match was {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn card_filter_is_not_restored() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev: Vec<Option<std::ffi::OsString>> = ["ZED_DEVICE_ID"]
            .iter()
            .map(|k| std::env::var_os(k))
            .collect();
        std::env::remove_var("ZED_DEVICE_ID");
        drop(GpuPref::SystemDefault.apply());
        assert!(std::env::var_os("ZED_DEVICE_ID").is_none());
        drop(GpuPref::Card(0x13c0).apply());
        assert_eq!(std::env::var("ZED_DEVICE_ID").as_deref(), Ok("13c0"));
        match &prev[0] {
            Some(v) => std::env::set_var("ZED_DEVICE_ID", v),
            None => std::env::remove_var("ZED_DEVICE_ID"),
        }
    }

    #[test]
    fn label_names_the_stored_vendor_and_keeps_unknown_cards_visible() {
        assert_eq!(card_label(0x13c0, &cards()), "NVIDIA · 10de:13c0");
        assert_eq!(card_label(0x73df, &cards()), "AMD · 1002:73df");
        assert_eq!(card_label(0x1111, &cards()), "Not detected · 1111");
        assert_eq!(card_label(0x13c0, &[]), "Not detected · 13c0");
    }
}
