mod data;
mod flow;

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Library,
    Game,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameTab {
    General,
    Mods,
    Env,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    General,
    CorePlugins,
    GameEnv,
    Mods,
}

impl SettingsTab {
    pub(crate) fn from_pref(s: &str) -> Self {
        match s {
            "core-plugins" => Self::CorePlugins,
            "game-env" => Self::GameEnv,
            "mods" => Self::Mods,
            _ => Self::General,
        }
    }

    pub(crate) fn pref_id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::CorePlugins => "core-plugins",
            Self::GameEnv => "game-env",
            Self::Mods => "mods",
        }
    }
}

/// Inner tab of the Settings Mods page (E89): OptiScaler | ReShade | Custom
/// Mods. Persisted as `settings_mods_tab`; unknown/missing prefers OptiScaler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsModsTab {
    Optiscaler,
    Reshade,
    Custom,
}

impl SettingsModsTab {
    /// Section order of the game page's Mods-tab sections (the Settings Mods
    /// inner tabs carry the same order in `settings.rs`).
    pub(crate) const ALL: [Self; 3] = [Self::Optiscaler, Self::Reshade, Self::Custom];

    /// Mods-tab section header label.
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Optiscaler => "gui-tab-mods-optiscaler",
            Self::Reshade => "gui-tab-mods-reshade",
            Self::Custom => "gui-tab-mods-custom",
        }
    }

    pub(crate) fn from_pref(s: &str) -> Self {
        match s {
            "reshade" => Self::Reshade,
            "custom" => Self::Custom,
            _ => Self::Optiscaler,
        }
    }

    pub(crate) fn pref_id(self) -> &'static str {
        match self {
            Self::Optiscaler => "optiscaler",
            Self::Reshade => "reshade",
            Self::Custom => "custom",
        }
    }

    /// Which inner tab owns a Provides id. Pack kinds (`reshade_addon`,
    /// `effect`, `texture`) ride with ReShade as one unified list.
    pub(crate) fn for_type(t: &str) -> Self {
        match t {
            "reshade" | "reshade_addon" | "effect" | "texture" => Self::Reshade,
            "custom" => Self::Custom,
            _ => Self::Optiscaler,
        }
    }

    /// Provides-types rendered on this tab. Pack kinds ride with ReShade as
    /// one unified list; every known ModType belongs to exactly one tab.
    pub(crate) fn types(self) -> &'static [&'static str] {
        match self {
            Self::Optiscaler => &["optiscaler"],
            Self::Reshade => &["reshade", "reshade_addon", "effect", "texture"],
            Self::Custom => &["custom"],
        }
    }

    /// Pack Provides-kinds shown unified on the ReShade tab.
    pub(crate) fn pack_types() -> &'static [&'static str] {
        &["reshade_addon", "effect", "texture"]
    }
}

/// In-session back/forward place. Not `ui.toml`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Place {
    Library { list: bool },
    Game { id: String, tab: GameTab },
    Settings { tab: SettingsTab },
}

/// Parked navigate-away intent while the config editor holds unsaved edits
/// (`gui.mod-config-edit` discard modal). Confirm replays the original op
/// so history bookkeeping matches an unblocked nav.
#[derive(Clone)]
pub(crate) enum ConfigNavPending {
    Place(Place),
    Back,
    Forward,
    /// Settings Mods inner-tab switch. Not a `Place` (which ends at
    /// `SettingsTab`), and never a same-place move: call sites return
    /// early on same-tab clicks before guarding.
    ModsTab(SettingsModsTab),
}

pub(crate) const HIST_CAP: usize = 20;

impl Shell {
    pub(crate) fn persist_nav(&mut self) {
        self.prefs.view = match (self.nav, self.library_list) {
            (Nav::Library, true) => "library-list".into(),
            (Nav::Library, false) => "library".into(),
            (Nav::Game, _) => "game".into(),
            (Nav::Settings, _) => "settings".into(),
        };
        self.prefs.save();
    }

    pub(crate) fn persist_settings_tab(&mut self) {
        self.prefs.settings_tab = self.settings_tab.pref_id().into();
        self.prefs.save();
    }

    pub(crate) fn persist_mods_tab(&mut self) {
        self.prefs.settings_mods_tab = self.mods_tab.pref_id().into();
        self.prefs.save();
    }

    pub(crate) fn current_place(&self) -> Place {
        match self.nav {
            Nav::Library => Place::Library {
                list: self.library_list,
            },
            Nav::Game => match self.selected.as_deref() {
                Some(id) => Place::Game {
                    id: id.to_string(),
                    tab: self.game_tab,
                },
                None => Place::Library {
                    list: self.library_list,
                },
            },
            Nav::Settings => Place::Settings {
                tab: self.settings_tab,
            },
        }
    }

    pub(crate) fn push_if_new(&mut self, new: Place) -> bool {
        if new == self.current_place() {
            return false;
        }
        self.hist_back.push(self.current_place());
        if self.hist_back.len() > HIST_CAP {
            self.hist_back.remove(0);
        }
        self.hist_fwd.clear();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsModsTab;

    #[test]
    fn mods_tab_owns_pack_types() {
        assert_eq!(
            SettingsModsTab::for_type("optiscaler"),
            SettingsModsTab::Optiscaler
        );
        assert_eq!(
            SettingsModsTab::for_type("reshade"),
            SettingsModsTab::Reshade
        );
        for t in ["reshade_addon", "effect", "texture"] {
            assert_eq!(
                SettingsModsTab::for_type(t),
                SettingsModsTab::Reshade,
                "{t}"
            );
        }
        assert_eq!(SettingsModsTab::for_type("custom"), SettingsModsTab::Custom);
    }

    #[test]
    fn mods_tabs_partition_every_mod_type() {
        let tabs = [
            SettingsModsTab::Optiscaler,
            SettingsModsTab::Reshade,
            SettingsModsTab::Custom,
        ];
        let mut seen: Vec<&str> = Vec::new();
        for tab in tabs {
            for t in tab.types() {
                assert!(!seen.contains(t), "duplicate owner: {t}");
                seen.push(t);
            }
            assert_eq!(SettingsModsTab::for_type(tab.pref_id()), tab);
        }
        let mut known: Vec<&str> = tuxgt_core::MOD_TYPES.iter().map(|t| t.mod_type()).collect();
        seen.sort_unstable();
        known.sort_unstable();
        assert_eq!(seen, known);
        for t in SettingsModsTab::pack_types() {
            assert!(
                SettingsModsTab::Reshade.types().contains(t),
                "pack outside ReShade tab: {t}"
            );
        }
        assert_eq!(
            SettingsModsTab::from_pref("nope"),
            SettingsModsTab::Optiscaler
        );
    }
}
