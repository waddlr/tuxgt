use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tuxgt_core::config_dir;

use super::gpu::GpuPref;
use super::theme::{FontScale, ThemeId};

fn is_false(b: &bool) -> bool {
    !b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn default_true() -> bool {
    true
}

fn default_collapsed() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub font_scale: String,
    #[serde(default)]
    type_ladder: u8,
    #[serde(default)]
    pub last_game: Option<String>,
    #[serde(default)]
    pub view: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub settings_tab: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub settings_mods_tab: String,
    /// Rail-first: a missing key reads collapsed; the titlebar toggle
    /// persists the choice.
    #[serde(default = "default_collapsed", skip_serializing_if = "is_true")]
    pub sidebar_collapsed: bool,
    /// Stored GPU choice for the first window: empty = system default, `cpu`
    /// = software rendering, else a PCI device id (`13c0`). Applies at the
    /// next start.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gpu: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub debug_log: bool,
    /// R12: opt-in. Close hides to the tray instead of quitting. Default
    /// off is the shipped behavior: a close quits.
    #[serde(default, skip_serializing_if = "is_false")]
    pub close_to_tray: bool,
    /// Hide to Tray: minimize hides to the tray instead of minimizing to
    /// the taskbar. Default on, independent of `close_to_tray`.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub minimize_to_tray: bool,
    /// Hide to Tray: hide the window after a successful Play, leaving it
    /// hidden until the user re-shows it. Default on.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub hide_on_launch: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: ThemeId::TuxDark.id().into(),
            font_scale: FontScale::Default.id().into(),
            type_ladder: 1,
            last_game: None,
            view: "library".into(),
            settings_tab: String::new(),
            settings_mods_tab: String::new(),
            sidebar_collapsed: true,
            gpu: String::new(),
            debug_log: false,
            close_to_tray: false,
            minimize_to_tray: true,
            hide_on_launch: true,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        config_dir().join("ui.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut p: Prefs = toml::from_str(&text).unwrap_or_default();
        let ladder = p.migrate_type_ladder();
        let stitch = p.migrate_stitch();
        if ladder || stitch {
            p.save();
        }
        p
    }

    /// Old shipped default was `large` at today's Default numbers. Rewrite once.
    fn migrate_type_ladder(&mut self) -> bool {
        if self.type_ladder >= 1 {
            return false;
        }
        if self.font_scale == "large" || self.font_scale.is_empty() {
            self.font_scale = FontScale::Default.id().into();
        }
        self.type_ladder = 1;
        true
    }

    /// E105: the retired `stitch` id reads as Dark Alt (Blue) and is rewritten.
    fn migrate_stitch(&mut self) -> bool {
        if self.theme == "stitch" {
            self.theme = ThemeId::TuxDarkBlue.id().into();
            true
        } else {
            false
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = fs::write(&path, text) {
                    tracing::warn!(error = %e, "write ui.toml");
                }
            }
            Err(e) => tracing::warn!(error = %e, "serialize ui.toml"),
        }
    }

    pub fn theme_id(&self) -> ThemeId {
        ThemeId::parse(&self.theme)
    }

    pub fn scale(&self) -> FontScale {
        FontScale::parse(&self.font_scale)
    }

    pub fn is_list(&self) -> bool {
        self.view == "library-list"
    }

    /// Stored GPU choice, read as the first-window preference.
    pub fn gpu_pref(&self) -> GpuPref {
        GpuPref::parse(&self.gpu)
    }
}

#[cfg(test)]
mod tests {
    use super::super::theme::ThemeId;
    use super::GpuPref;
    use super::Prefs;

    #[test]
    fn default_theme_is_tux_dark() {
        assert_eq!(Prefs::default().theme_id(), ThemeId::TuxDark);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(text.contains("tuxgt-dark"), "explicit id: {text}");
    }

    #[test]
    fn stitch_reads_and_migrates_to_dark_blue() {
        let mut p: Prefs = toml::from_str("theme = \"stitch\"").expect("old id");
        assert_eq!(p.theme_id(), ThemeId::TuxDarkBlue);
        assert!(p.migrate_stitch());
        assert_eq!(p.theme, "tuxgt-dark-blue");
        assert!(!p.migrate_stitch());
    }

    #[test]
    fn settings_tab_missing_is_empty() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(p.settings_tab.is_empty());
        let p: Prefs = toml::from_str("settings_tab = \"core-plugins\"").expect("core-plugins");
        assert_eq!(p.settings_tab, "core-plugins");
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("settings_tab"),
            "unset key must stay omitted: {text}"
        );
    }

    #[test]
    fn settings_tab_round_trips() {
        use super::super::SettingsTab;
        for (pref, tab) in [
            ("general", SettingsTab::General),
            ("core-plugins", SettingsTab::CorePlugins),
            ("game-env", SettingsTab::GameEnv),
            ("mods", SettingsTab::Mods),
        ] {
            assert!(SettingsTab::from_pref(pref) == tab, "{pref}");
            assert!(tab.pref_id() == pref);
        }
        assert!(SettingsTab::from_pref("plugins") == SettingsTab::General);
        assert!(SettingsTab::from_pref("env") == SettingsTab::General);
        assert!(SettingsTab::from_pref("") == SettingsTab::General);
    }
    #[test]
    fn settings_mods_tab_missing_is_empty() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(p.settings_mods_tab.is_empty());
        let p: Prefs = toml::from_str("settings_mods_tab = \"reshade\"").expect("reshade");
        assert_eq!(p.settings_mods_tab, "reshade");
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("settings_mods_tab"),
            "unset key must stay omitted: {text}"
        );
    }

    #[test]
    fn settings_mods_tab_round_trips() {
        use super::super::SettingsModsTab;
        for (pref, tab) in [
            ("optiscaler", SettingsModsTab::Optiscaler),
            ("reshade", SettingsModsTab::Reshade),
            ("custom", SettingsModsTab::Custom),
        ] {
            assert!(SettingsModsTab::from_pref(pref) == tab, "{pref}");
            assert!(tab.pref_id() == pref);
        }
        assert!(SettingsModsTab::from_pref("quake") == SettingsModsTab::Optiscaler);
        assert!(SettingsModsTab::from_pref("") == SettingsModsTab::Optiscaler);
    }

    #[test]
    fn type_ladder_migrates_old_large() {
        let mut p: Prefs = toml::from_str("font_scale = \"large\"").expect("old large");
        assert_eq!(p.type_ladder, 0);
        assert!(p.migrate_type_ladder());
        assert_eq!(p.font_scale, "default");
        assert_eq!(p.type_ladder, 1);
        assert!(!p.migrate_type_ladder());
    }

    #[test]
    fn type_ladder_keeps_compact() {
        let mut p: Prefs = toml::from_str("font_scale = \"compact\"").expect("compact");
        assert!(p.migrate_type_ladder());
        assert_eq!(p.font_scale, "compact");
        assert_eq!(p.type_ladder, 1);
    }

    #[test]
    fn sidebar_collapsed_missing_is_true_and_round_trips() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(p.sidebar_collapsed);
        let p: Prefs = toml::from_str("sidebar_collapsed = false").expect("expanded");
        assert!(!p.sidebar_collapsed);
        let p: Prefs = toml::from_str("sidebar_collapsed = true").expect("collapsed");
        assert!(p.sidebar_collapsed);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("sidebar_collapsed"),
            "default key must stay omitted: {text}"
        );
    }

    #[test]
    fn hide_on_launch_missing_is_true_and_round_trips() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(p.hide_on_launch);
        let p: Prefs = toml::from_str("hide_on_launch = false").expect("launch off");
        assert!(!p.hide_on_launch);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("hide_on_launch"),
            "default key must stay omitted: {text}"
        );
    }

    #[test]
    fn minimize_to_tray_missing_is_true_and_round_trips() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(p.minimize_to_tray);
        let p: Prefs = toml::from_str("minimize_to_tray = false").expect("min off");
        assert!(!p.minimize_to_tray);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("minimize_to_tray"),
            "default key must stay omitted: {text}"
        );
    }

    #[test]
    fn debug_log_missing_is_false_and_round_trips() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert!(!p.debug_log);
        let p: Prefs = toml::from_str("debug_log = true").expect("debug on");
        assert!(p.debug_log);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(
            !text.contains("debug_log"),
            "unset key must stay omitted: {text}"
        );
    }

    #[test]
    fn gpu_missing_is_system_default_and_round_trips() {
        let p: Prefs = toml::from_str("").expect("empty prefs");
        assert_eq!(p.gpu_pref(), GpuPref::SystemDefault);
        let text = toml::to_string(&Prefs::default()).expect("ser");
        assert!(!text.contains("gpu"), "unset key must stay omitted: {text}");
        let mut p: Prefs = toml::from_str("gpu = \"13c0\"").expect("gpu");
        assert_eq!(p.gpu_pref(), GpuPref::Card(0x13c0));
        let cpu: Prefs = toml::from_str("gpu = \"cpu\"").expect("cpu");
        assert_eq!(cpu.gpu_pref(), GpuPref::Cpu);
        p.gpu = GpuPref::SystemDefault.pref_id();
        let text = toml::to_string(&p).expect("ser");
        assert!(
            !text.contains("gpu"),
            "system default omits the key: {text}"
        );
    }

    #[test]
    fn type_ladder_keeps_post_rebase_large() {
        let mut p: Prefs =
            toml::from_str("font_scale = \"large\"\ntype_ladder = 1").expect("new large");
        assert!(!p.migrate_type_ladder());
        assert_eq!(p.font_scale, "large");
    }

    /// R12: a Hide/Show cycle rebuilds the window from `Prefs::load()`, not
    /// from the snapshot taken at process start, so a Settings change made
    /// mid-session must be what the next load reports. A captured
    /// launch-time clone would keep answering with the old values here.
    #[test]
    fn load_reflects_a_session_change_over_the_launch_snapshot() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("tuxgt-prefs-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp config dir");
        let prev = std::env::var_os("TUXGT_CONFIG");
        std::env::set_var("TUXGT_CONFIG", &dir);

        // What the process would have captured at launch.
        let launch = Prefs::load();
        assert_eq!(launch.theme_id(), ThemeId::TuxDark);
        assert!(!launch.close_to_tray);

        // The session then changes theme/scale and enables both tray
        // switches, each persisting through the normal save path.
        let mut session = launch.clone();
        session.theme = ThemeId::TuxLight.id().into();
        session.font_scale = super::super::theme::FontScale::Compact.id().into();
        session.close_to_tray = true;
        session.minimize_to_tray = true;
        session.save();

        // The rebuild path: the next window reads the change, not the
        // launch-time values.
        let reopened = Prefs::load();
        assert_eq!(reopened.theme_id(), ThemeId::TuxLight);
        assert_eq!(reopened.scale(), super::super::theme::FontScale::Compact);
        assert!(reopened.close_to_tray);
        assert!(reopened.minimize_to_tray);
        // The snapshot is untouched: nothing about the launch value moved.
        assert_eq!(launch.theme_id(), ThemeId::TuxDark);
        assert!(!launch.close_to_tray);

        match prev {
            Some(v) => std::env::set_var("TUXGT_CONFIG", v),
            None => std::env::remove_var("TUXGT_CONFIG"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
