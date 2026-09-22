use gpui_kit::*;

use super::super::Shell;
use super::{ConfigNavPending, GameTab, Nav, Place, SettingsTab};

impl Shell {
    /// E143: tab switches are history entries — back/forward cross tabs,
    /// not just top-level pages. Same-tab clicks are no-ops.
    /// Shared core: push history, run the per-kind refresh, scroll, notify.
    fn switch_tab(
        &mut self,
        place: Place,
        refresh: impl FnOnce(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        self.push_if_new(place);
        refresh(self, cx);
        self.scroll_page_top();
        cx.notify();
    }

    pub(crate) fn switch_settings_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        tracing::debug!(action = "switch-settings-tab", tab = ?tab);
        if tab == self.settings_tab {
            return;
        }
        let place = Place::Settings { tab };
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        self.switch_tab(
            place,
            |this, cx| {
                this.settings_tab = tab;
                this.persist_settings_tab();
                this.add_form = None;
                this.load_settings_tab(cx);
                this.drop_preview_disclosure();
            },
            cx,
        );
    }

    pub(crate) fn switch_game_tab(&mut self, tab: GameTab, cx: &mut Context<Self>) {
        tracing::debug!(action = "switch-game-tab", game = self.selected.as_deref().unwrap_or("-"), tab = ?tab);
        if tab == self.game_tab {
            return;
        }
        let place = match self.selected.clone() {
            Some(id) => Place::Game { id, tab },
            None => return,
        };
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        self.switch_tab(
            place,
            |this, cx| {
                // Tab-scoped data drops on leave; the new tab reloads in
                // `fetch_for_tab`.
                if this.game_tab == GameTab::Mods && tab != GameTab::Mods {
                    this.drop_all_mod_data();
                }
                if this.game_tab == GameTab::Env && tab != GameTab::Env {
                    this.clear_env_maps();
                }
                this.game_tab = tab;
                this.drop_preview_disclosure();
                this.fetch_for_tab(cx);
            },
            cx,
        );
    }

    pub(crate) fn jump_library(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "jump-library");
        let place = Place::Library {
            list: self.library_list,
        };
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        if !self.push_if_new(place) {
            return;
        }
        self.nav = Nav::Library;
        self.persist_nav();
        // The Library holds no game or Settings tab data.
        self.clear_game_data();
        self.drop_preview_disclosure();
        self.drop_all_mod_data();
        self.drop_settings_data();
        self.enter_library_data();
        self.scroll_page_top();
        cx.notify();
    }

    pub(crate) fn enter_settings(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "enter-settings");
        if self.nav == Nav::Settings {
            self.go_back(cx);
            return;
        }
        let place = Place::Settings {
            tab: self.settings_tab,
        };
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        if !self.push_if_new(place) {
            return;
        }
        self.nav = Nav::Settings;
        self.persist_settings_tab();
        self.drop_preview_disclosure();
        self.persist_nav();
        self.clear_game_data();
        self.drop_all_mod_data();
        self.drop_page_data();
        self.reload_secrets(cx);
        self.reload_tools(cx);
        self.load_settings_tab(cx);
        self.scroll_page_top();
        cx.notify();
    }

    pub(crate) fn go_back(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "go-back");
        // Empty history is a no-op: never touch the editor for a nav that
        // goes nowhere.
        if self.hist_back.is_empty() {
            return;
        }
        if !self.try_leave_config(ConfigNavPending::Back, cx) {
            return;
        }
        let Some(prev) = self.hist_back.pop() else {
            return;
        };
        self.hist_fwd.push(self.current_place());
        self.apply_place(prev, cx);
    }

    pub(crate) fn go_forward(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "go-forward");
        // Empty history is a no-op: never touch the editor for a nav that
        // goes nowhere.
        if self.hist_fwd.is_empty() {
            return;
        }
        if !self.try_leave_config(ConfigNavPending::Forward, cx) {
            return;
        }
        let Some(next) = self.hist_fwd.pop() else {
            return;
        };
        self.hist_back.push(self.current_place());
        self.apply_place(next, cx);
    }

    pub(crate) fn apply_place(&mut self, place: Place, cx: &mut Context<Self>) {
        // Confirm funnel included: Discard replays intents here with the
        // editor already cleared, so this passes through.
        if !self.try_leave_config(ConfigNavPending::Place(place.clone()), cx) {
            return;
        }
        match place {
            Place::Library { list } => {
                self.nav = Nav::Library;
                self.library_list = list;
                self.drop_preview_disclosure();
                self.persist_nav();
                self.clear_game_data();
                self.drop_all_mod_data();
                self.drop_settings_data();
                self.enter_library_data();
                self.scroll_page_top();
                cx.notify();
            }
            Place::Game { id, tab } => self.show_game(id, tab, cx),
            Place::Settings { tab } => {
                self.nav = Nav::Settings;
                self.settings_tab = tab;
                self.add_form = None;
                self.persist_settings_tab();
                self.drop_preview_disclosure();
                self.persist_nav();
                self.clear_game_data();
                self.drop_all_mod_data();
                self.drop_page_data();
                self.reload_secrets(cx);
                self.reload_tools(cx);
                self.load_settings_tab(cx);
                self.scroll_page_top();
                cx.notify();
            }
        }
    }
}
