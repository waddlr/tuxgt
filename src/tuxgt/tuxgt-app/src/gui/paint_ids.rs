//! T24 precomputed paint id keys.
//!
//! Element ids must be stable strings, but rebuilding them with `format!` on
//! every paint wastes hundreds of transient allocs per frame. Row-scoped ids
//! are built once where the row data is constructed and stored alongside it
//! (`InstanceIds`, `ModIds`, `KnobIds`); ids over small static domains
//! (detect fields, unpack tools, mod tabs, host rows) are interned helpers.
//! Every constructor below yields byte-identical strings to the `format!` it
//! replaces (see tests).

use std::collections::HashMap;
use std::sync::OnceLock;

use gpui_kit::SharedString;
use tuxgt_core::EnvKnob;

use super::game::files_accordion_key;
use super::SettingsModsTab;

/// Settings Mods row ids (`settings/mods.rs`). Built in `load_instances`.
#[derive(Clone)]
pub struct InstanceIds {
    pub row: SharedString,
    pub enable: SharedString,
    pub export: SharedString,
    pub rescan: SharedString,
    pub remove: SharedString,
    pub cat_update: SharedString,
}

impl InstanceIds {
    pub fn for_id(id: &str) -> Self {
        Self {
            row: SharedString::from(format!("in-{id}")),
            enable: SharedString::from(format!("ie-{id}")),
            export: SharedString::from(format!("ex-{id}")),
            rescan: SharedString::from(format!("rs-{id}")),
            remove: SharedString::from(format!("rm-{id}")),
            cat_update: SharedString::from(format!("cat-update-{id}")),
        }
    }
}

/// Game mod-card ids (`game/card.rs`). Built in `load_mods_for`, which knows
/// the game id the files-accordion key needs.
#[derive(Clone)]
pub struct ModIds {
    pub card: SharedString,
    pub switch: SharedString,
    pub name: SharedString,
    pub uninstall: SharedString,
    pub slot_row: SharedString,
    pub slot: SharedString,
    pub move_up: SharedString,
    pub move_down: SharedString,
    pub files_key: SharedString,
    pub files_id: SharedString,
}

impl ModIds {
    pub fn for_instance(instance: &str, game_id: &str) -> Self {
        let files_key = files_accordion_key(game_id, instance);
        Self {
            card: SharedString::from(format!("mod-{instance}")),
            switch: SharedString::from(format!("sw-{instance}")),
            name: SharedString::from(format!("mod-name-{instance}")),
            uninstall: SharedString::from(format!("un-{instance}")),
            slot_row: SharedString::from(format!("slot-row-{instance}")),
            slot: SharedString::from(format!("slot-{instance}")),
            move_up: SharedString::from(format!("move-up-{instance}")),
            move_down: SharedString::from(format!("move-down-{instance}")),
            files_id: SharedString::from(format!("mod-files-{files_key}")),
            files_key: SharedString::from(files_key),
        }
    }
}

/// Env knob-row ids (`game/knob.rs`). The knob set changes only when plugins
/// toggle, so `Shell` keeps one map; paint falls back to `for_id` if a key is
/// ever missing (same string, one paint's alloc).
#[derive(Clone)]
pub struct KnobIds {
    pub enable: SharedString,
    pub value: SharedString,
    pub menu: SharedString,
    pub label: SharedString,
    pub tip: SharedString,
}

impl KnobIds {
    pub fn for_id(id: &str) -> Self {
        Self {
            enable: SharedString::from(format!("ke-{id}")),
            value: SharedString::from(format!("k-{id}")),
            menu: SharedString::from(format!("kv-{id}")),
            label: SharedString::from(format!("knob-l-{id}")),
            tip: SharedString::from(format!("kt-{id}")),
        }
    }
}

pub(crate) fn knob_ids_for(knobs: &[&'static EnvKnob]) -> HashMap<&'static str, KnobIds> {
    knobs
        .iter()
        .map(|k| (k.id, KnobIds::for_id(k.id)))
        .collect()
}

/// Detect-row ids (`game/detect.rs`). Snapshot keys are 10 fixed literals, so
/// both ids are `new_static` (no alloc); unknown keys fall back to `format!`.
#[derive(Clone)]
pub struct DetectIds {
    pub row: SharedString,
    pub eff: SharedString,
}

pub(crate) fn detect_ids(key: &str) -> DetectIds {
    match key {
        "exe" => DetectIds {
            row: SharedString::new_static("det-exe"),
            eff: SharedString::new_static("deteff-exe"),
        },
        "platform" => DetectIds {
            row: SharedString::new_static("det-platform"),
            eff: SharedString::new_static("deteff-platform"),
        },
        "bitness" => DetectIds {
            row: SharedString::new_static("det-bitness"),
            eff: SharedString::new_static("deteff-bitness"),
        },
        "api" => DetectIds {
            row: SharedString::new_static("det-api"),
            eff: SharedString::new_static("deteff-api"),
        },
        "extra_apis" => DetectIds {
            row: SharedString::new_static("det-extra_apis"),
            eff: SharedString::new_static("deteff-extra_apis"),
        },
        "engine" => DetectIds {
            row: SharedString::new_static("det-engine"),
            eff: SharedString::new_static("deteff-engine"),
        },
        "prefix" => DetectIds {
            row: SharedString::new_static("det-prefix"),
            eff: SharedString::new_static("deteff-prefix"),
        },
        "proton" => DetectIds {
            row: SharedString::new_static("det-proton"),
            eff: SharedString::new_static("deteff-proton"),
        },
        "build" => DetectIds {
            row: SharedString::new_static("det-build"),
            eff: SharedString::new_static("deteff-build"),
        },
        "exe_version" => DetectIds {
            row: SharedString::new_static("det-exe_version"),
            eff: SharedString::new_static("deteff-exe_version"),
        },
        _ => DetectIds {
            row: SharedString::from(format!("det-{key}")),
            eff: SharedString::from(format!("deteff-{key}")),
        },
    }
}

/// Host tools row id (`settings/host.rs`). Tool names are 4 fixed literals.
pub(crate) fn tool_id(name: &str) -> SharedString {
    match name {
        "unzip" => SharedString::new_static("tool-unzip"),
        "unrar" => SharedString::new_static("tool-unrar"),
        "7z" => SharedString::new_static("tool-7z"),
        "tar" => SharedString::new_static("tool-tar"),
        _ => SharedString::from(format!("tool-{name}")),
    }
}

/// Mods-tab section id (`game/mods.rs`). Three static values.
pub(crate) fn mods_section_id(tab: SettingsModsTab) -> SharedString {
    match tab {
        SettingsModsTab::Optiscaler => SharedString::new_static("mods-section-optiscaler"),
        SettingsModsTab::Reshade => SharedString::new_static("mods-section-reshade"),
        SettingsModsTab::Custom => SharedString::new_static("mods-section-custom"),
    }
}

/// Load-conflicts block id (`game/mods.rs`). Three static values.
pub(crate) fn load_conflicts_id(tab: SettingsModsTab) -> SharedString {
    match tab {
        SettingsModsTab::Optiscaler => SharedString::new_static("load-conflicts-optiscaler"),
        SettingsModsTab::Reshade => SharedString::new_static("load-conflicts-reshade"),
        SettingsModsTab::Custom => SharedString::new_static("load-conflicts-custom"),
    }
}

/// Host-install failure row id (`settings/host.rs`, `host-{i}` over the
/// paint-time failure filter). Indices are small; the table covers 0..64 and
/// anything beyond falls back to `format!` (same string either way).
pub(crate) fn host_row_id(i: usize) -> SharedString {
    static IDS: OnceLock<Box<[SharedString]>> = OnceLock::new();
    let ids = IDS.get_or_init(|| {
        (0..64)
            .map(|n| SharedString::from(format!("host-{n}")))
            .collect()
    });
    ids.get(i)
        .cloned()
        .unwrap_or_else(|| SharedString::from(format!("host-{i}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_ids_match_legacy_strings() {
        let ids = InstanceIds::for_id("my-mod");
        assert_eq!(ids.row.as_str(), "in-my-mod");
        assert_eq!(ids.enable.as_str(), "ie-my-mod");
        assert_eq!(ids.export.as_str(), "ex-my-mod");
        assert_eq!(ids.rescan.as_str(), "rs-my-mod");
        assert_eq!(ids.remove.as_str(), "rm-my-mod");
        assert_eq!(ids.cat_update.as_str(), "cat-update-my-mod");
    }

    #[test]
    fn mod_ids_match_legacy_strings() {
        let ids = ModIds::for_instance("shaders", "skyrim");
        assert_eq!(ids.card.as_str(), "mod-shaders");
        assert_eq!(ids.switch.as_str(), "sw-shaders");
        assert_eq!(ids.name.as_str(), "mod-name-shaders");
        assert_eq!(ids.uninstall.as_str(), "un-shaders");
        assert_eq!(ids.slot_row.as_str(), "slot-row-shaders");
        assert_eq!(ids.slot.as_str(), "slot-shaders");
        assert_eq!(ids.move_up.as_str(), "move-up-shaders");
        assert_eq!(ids.move_down.as_str(), "move-down-shaders");
        assert_eq!(ids.files_key.as_str(), "skyrim/shaders");
        assert_eq!(ids.files_id.as_str(), "mod-files-skyrim/shaders");
    }

    #[test]
    fn knob_ids_match_legacy_strings() {
        let ids = KnobIds::for_id("proton-log");
        assert_eq!(ids.enable.as_str(), "ke-proton-log");
        assert_eq!(ids.value.as_str(), "k-proton-log");
        assert_eq!(ids.menu.as_str(), "kv-proton-log");
        assert_eq!(ids.label.as_str(), "knob-l-proton-log");
    }

    #[test]
    fn detect_ids_cover_all_snapshot_keys() {
        for key in [
            "exe",
            "platform",
            "bitness",
            "api",
            "extra_apis",
            "engine",
            "prefix",
            "proton",
            "build",
            "exe_version",
        ] {
            let ids = detect_ids(key);
            assert_eq!(ids.row.as_str(), format!("det-{key}"));
            assert_eq!(ids.eff.as_str(), format!("deteff-{key}"));
        }
        let ids = detect_ids("future-key");
        assert_eq!(ids.row.as_str(), "det-future-key");
        assert_eq!(ids.eff.as_str(), "deteff-future-key");
    }

    #[test]
    fn tool_and_host_ids_match_legacy_strings() {
        for name in ["unzip", "unrar", "7z", "tar"] {
            assert_eq!(tool_id(name).as_str(), format!("tool-{name}"));
        }
        assert_eq!(tool_id("future-tool").as_str(), "tool-future-tool");
        assert_eq!(host_row_id(0).as_str(), "host-0");
        assert_eq!(host_row_id(63).as_str(), "host-63");
        assert_eq!(host_row_id(64).as_str(), "host-64");
    }

    #[test]
    fn tab_ids_match_legacy_strings() {
        for (tab, pref) in [
            (SettingsModsTab::Optiscaler, "optiscaler"),
            (SettingsModsTab::Reshade, "reshade"),
            (SettingsModsTab::Custom, "custom"),
        ] {
            assert_eq!(
                mods_section_id(tab).as_str(),
                format!("mods-section-{pref}")
            );
            assert_eq!(
                load_conflicts_id(tab).as_str(),
                format!("load-conflicts-{pref}")
            );
        }
    }
}
