mod preview;

use gpui_kit::*;

use tuxgt_core::{data_dir, has_apply_record, GameRow};

use super::*;

impl Shell {
    pub fn selected_game(&self) -> Option<&GameRow> {
        let id = self.selected.as_deref()?;
        self.games.iter().find(|g| g.id == id)
    }

    pub(crate) fn revalidate_selection(&mut self) {
        if let Some(id) = &self.selected {
            if self.index.iter().any(|g| g.id == *id) {
                return;
            }
        }
        self.selected = self
            .prefs
            .last_game
            .clone()
            .filter(|id| self.index.iter().any(|g| g.id == *id))
            .or_else(|| self.index.first().map(|g| g.id.clone()));
        self.clear_game_data();
        self.prefs.last_game = self.selected.clone();
        self.prefs.save();
    }

    pub(crate) fn select_game(&mut self, id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "select-game", game = id.as_str());
        let place = Place::Game {
            id: id.clone(),
            tab: self.game_tab,
        };
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        let _ = self.push_if_new(place);
        self.show_game(id, self.game_tab, cx);
    }

    /// Page-scoped metadata for one game: single-entry maps, nothing
    /// library-wide. Missing facts stay absent (honest `none` at paint).
    pub(crate) fn set_game_metadata(&mut self, id: &str, meta: GameMeta) {
        let (tiers, proton, awacy) = game_metadata_maps(id, meta);
        self.tiers = tiers;
        self.proton = proton;
        self.awacy = awacy;
    }

    /// Drop all cached metadata facts. Pages reload what they paint.
    pub(crate) fn clear_metadata(&mut self) {
        self.tiers = Default::default();
        self.proton = Default::default();
        self.awacy = Default::default();
    }

    pub(crate) fn show_game(&mut self, id: String, tab: GameTab, cx: &mut Context<Self>) {
        // Direct callers (attention follow, history) funnel through here;
        // the guard runs again and passes once the editor is clear.
        if !self.try_leave_config(
            ConfigNavPending::Place(Place::Game {
                id: id.clone(),
                tab,
            }),
            cx,
        ) {
            return;
        }
        // File-list disclosure is per page: a real move collapses every open
        // arrow and full list; same-place reloads keep them. The walk cache
        // stays (paint requires `preview_open`).
        if self.current_place()
            != (Place::Game {
                id: id.clone(),
                tab,
            })
        {
            self.drop_preview_disclosure();
        }
        tracing::debug!(action = "show-game", game = id.as_str(), tab = ?tab);
        self.selected = Some(id.clone());
        self.nav = Nav::Game;
        self.game_tab = tab;
        self.prefs.last_game = Some(id.clone());
        self.persist_nav();
        // The Game page holds the selected game only: drop Settings data,
        // the previous game's Mods-tab data, and the armed-state flags
        // (reloaded below for the new game). Mods-tab rows load in
        // `fetch_for_tab` when that tab shows.
        self.drop_settings_data();
        self.drop_all_mod_data();
        // At most one hero decode stays resident: the previous game's wash
        // (the largest entry) drops on every switch.
        self.evict_art(&[tuxgt_core::ArtKind::Hero], cx);
        self.applied.clear();
        self.handle.clear();
        self.session_payload.clear();
        self.reload_selected_row();
        self.applied
            .insert(id.clone(), has_apply_record(&data_dir(), &id));
        self.handle.insert(id.clone(), load_handle(&id));
        self.session_payload.insert(id.clone(), load_payload(&id));
        if self.applied.get(&id).copied().unwrap_or(false) {
            self.note_heroic_restart(&id);
        }
        self.clear_game_data();
        self.fetch_for_tab(cx);
        self.pending_confirm = None;
        self.mods_picker_open = false;
        self.picker_checked.clear();
        self.install_queue.clear();
        self.install_current = None;
        self.uninstall_queue.clear();
        self.uninstall_current = None;
        self.custom_env_adding = false;
        self.scroll_page_top();
        cx.notify();
    }

    pub(crate) fn mod_matches_needle(label: &str, id: &str, needle: &str) -> bool {
        needle.is_empty()
            || label.to_lowercase().contains(needle)
            || id.to_lowercase().contains(needle)
    }

    /// Env maps are held on the Env tab; late async completions refill them
    /// only while showing.
    pub(crate) fn env_maps_showing(&self) -> bool {
        self.nav == Nav::Game && self.game_tab == GameTab::Env
    }

    /// Catalog rows are held on Settings CorePlugins + Mods; late async
    /// completions refill them only while showing (tabs reload on entry).
    pub(crate) fn instances_showing(&self) -> bool {
        self.nav == Nav::Settings
            && matches!(
                self.settings_tab,
                SettingsTab::CorePlugins | SettingsTab::Mods
            )
    }

    /// Drop held Env-tab maps (tab leave). Counts stay for the hero.
    pub(crate) fn clear_env_maps(&mut self) {
        self.knob_values.clear();
        self.knob_enabled.clear();
        self.custom_env = Default::default();
        self.global_knobs.clear();
    }

    pub(crate) fn clear_game_data(&mut self) {
        self.clear_env_maps();
        self.knob_count = 0;
        self.custom_count = 0;
        self.wrappers = Default::default();
        self.launch_needs = Default::default();
        self.launch_cfg = None;
        self.about_env_scroll = ScrollHandle::new();
        self.appid_stored = None;
        self.appid_searching = false;
        self.appid_searched = false;
        self.appid_hits = Default::default();
        self.detect = Default::default();
        self.extras = Default::default();
        self.extra_exe_for = None;
        self.detect_for = None;
        self.override_edit = None;
        self.redetect_confirm = false;
    }
}
