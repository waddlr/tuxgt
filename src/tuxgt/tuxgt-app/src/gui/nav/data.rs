use gpui_kit::*;

use tuxgt_core::{data_dir, list_templates, GameIndexRow};

use super::super::{
    library, load_games, load_instances, load_plugins, load_proton, load_tiers, Nav, Shell,
};
use super::SettingsTab;

impl Shell {
    /// Derive the always-held index from full rows. Base positions index
    /// these rows, so every rebuild derives both from the same read (an
    /// external DB change between two independent queries cannot misalign
    /// them).
    pub(crate) fn set_index_from_rows(&mut self, rows: &[tuxgt_core::GameRow]) {
        self.index = rows
            .iter()
            .map(GameIndexRow::from_row)
            .collect::<Vec<_>>()
            .into_boxed_slice();
    }

    /// Library page data: full rows (aligned with `index`), full metadata,
    /// and a fresh base filter+sort. Synchronous local reads.
    pub(crate) fn enter_library_data(&mut self) {
        match load_games() {
            Ok(games) => {
                self.set_index_from_rows(&games);
                self.games = games.into_boxed_slice();
                self.revalidate_selection();
            }
            Err(e) => {
                self.games = Default::default();
                self.status = format!("{e}");
            }
        }
        self.tiers = load_tiers();
        self.proton = load_proton();
        self.awacy = library::load_awacy(&self.games);
        self.recompute_base();
    }

    /// Rebuild the stored sidebar/Library base after mod counts, tiers,
    /// AWACY, or row fields change. Library recomputes from held rows;
    /// other pages rebuild from a transient full read (the sidebar
    /// filters/sorts off this list on every page) and refresh the index
    /// from the same rows so positions stay aligned.
    pub(crate) fn rebuild_base(&mut self) {
        if self.nav == Nav::Library {
            self.recompute_base();
            return;
        }
        let Ok(rows) = load_games() else {
            return;
        };
        let tiers = load_tiers();
        let awacy = library::load_awacy(&rows);
        self.base_filtered =
            library::compute_base(&self.filters, &tiers, &awacy, &self.mod_counts, &rows);
        self.set_index_from_rows(&rows);
        self.revalidate_selection();
    }

    /// Drop page-scoped full rows + metadata. Settings holds neither; the
    /// Game page reloads its selected row in `show_game`.
    pub(crate) fn drop_page_data(&mut self) {
        self.games = Default::default();
        self.clear_metadata();
        self.drop_preview_disclosure();
    }

    /// Collapse open/expanded mod file lists. The walk cache
    /// (`file_preview_cache`) stays: paint requires `preview_open`, so kept
    /// bodies never paint without their arrow.
    pub(crate) fn drop_preview_disclosure(&mut self) {
        self.preview_open.clear();
        self.preview_expanded.clear();
    }

    /// Per-tab Settings data, loaded on page entry and on tab switch. The
    /// Mods tab re-reads the catalog rows: their preview facts (payload
    /// presence, EffectFiles) change with every install. Tab-scoped: the
    /// previous tab's rows drop first.
    pub(crate) fn load_settings_tab(&mut self, cx: &mut Context<Self>) {
        self.plugins = Default::default();
        self.instances = Default::default();
        self.family_templates = Default::default();
        self.global_knobs = Default::default();
        match self.settings_tab {
            SettingsTab::CorePlugins => {
                self.plugins = load_plugins(&self.strings).into_boxed_slice();
                self.instances = load_instances().into_boxed_slice();
            }
            SettingsTab::GameEnv => self.reload_global_env(cx),
            SettingsTab::Mods => {
                self.instances = load_instances().into_boxed_slice();
                self.family_templates = list_templates(&data_dir())
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|t| t.family.is_some())
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
            }
            SettingsTab::General => {}
        }
    }

    /// Drop Settings-held data (leaving Settings). The next entry reloads
    /// per tab in `load_settings_tab`.
    pub(crate) fn drop_settings_data(&mut self) {
        self.plugins = Default::default();
        self.instances = Default::default();
        self.family_templates = Default::default();
        self.tools = Default::default();
        self.add_form = None;
        self.pending_archive_password = None;
        self.family_mint = None;
        self.extras_mint = None;
    }
}
