use gpui_kit::*;

use tuxgt_core::{data_dir, has_apply_record, FluentArgs, GameRow};

use super::super::{
    library, load_disabled, load_game_row, load_handle, load_metadata_for, load_mod_counts,
    load_payload, load_proton, load_tiers, scan_library, Nav, Shell,
};

impl Shell {
    pub(crate) fn rescan(&mut self, cx: &mut Context<Self>) {
        tracing::debug!(action = "rescan");
        self.status = self.strings.get("gui-status-scanning");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async { scan_library() }).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(rows) => {
                        tracing::debug!(action = "rescan", count = rows.len(), outcome = "done");
                        this.apply_scan_completion(rows, cx, true);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "rescan failed");
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-scan", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Boot library scan: quiet completion (no status line), errors to the
    /// log. Runs on `App` context: the root view does not exist yet.
    pub(crate) fn spawn_boot_scan(view: Entity<Self>, cx: &mut App) {
        cx.spawn({
            let view = view.clone();
            async move |cx| {
                let result = cx.background_spawn(async { scan_library() }).await;
                match result {
                    Ok(rows) => {
                        let _ = cx.update(|cx| {
                            view.update(cx, |this, cx| {
                                this.apply_scan_completion(rows, cx, false);
                                cx.notify();
                            });
                        });
                    }
                    Err(e) => tracing::error!(error = %e, "background scan"),
                }
            }
        })
        .detach();
    }

    /// Shared scan completion (boot scan + manual rescan). Post-revalidate
    /// order: per-selected refreshes target the live selection, and a pruned
    /// selection revalidates to another game whose row the Game page reloads
    /// (`apply_scan_rows` kept the old one, if any). `announce` writes the
    /// titles-count status line (manual rescan only; the boot scan stays
    /// quiet). Callers own the repaint.
    pub(crate) fn apply_scan_completion(
        &mut self,
        rows: Vec<GameRow>,
        cx: &mut Context<Self>,
        announce: bool,
    ) {
        self.apply_scan_rows(rows);
        self.spawn_art_render_all(cx);
        self.refresh_selected_mods();
        if let Some(id) = self.selected.clone() {
            self.applied
                .insert(id.clone(), has_apply_record(&data_dir(), &id));
            self.handle.insert(id.clone(), load_handle(&id));
            self.session_payload.insert(id.clone(), load_payload(&id));
        }
        if announce {
            let mut args = FluentArgs::new();
            args.set("count", self.index.len().to_string());
            self.status = self.strings.get_args("gui-status-titles", Some(&args));
        }
        self.revalidate_selection();
        if self.nav == Nav::Game {
            self.reload_selected_row();
        }
        self.fetch_for_tab(cx);
    }

    /// Fold transient scan rows into the always-held index, rebuild the
    /// stored base filter+sort from them (a rescan can add or drop games),
    /// then keep full rows + metadata per current page (Library: all, Game:
    /// selected, Settings: neither).
    pub(crate) fn apply_scan_rows(&mut self, rows: Vec<GameRow>) {
        self.set_index_from_rows(&rows);
        self.mod_counts = load_mod_counts(&self.index);
        self.disabled_managers = load_disabled();
        let tiers = load_tiers();
        let awacy = library::load_awacy(&rows);
        self.base_filtered =
            library::compute_base(&self.filters, &tiers, &awacy, &self.mod_counts, &rows);
        match self.nav {
            Nav::Library => {
                self.games = rows.into_boxed_slice();
                self.tiers = tiers;
                self.proton = load_proton();
                self.awacy = awacy;
            }
            Nav::Game => {
                let id = self.selected.clone().unwrap_or_default();
                if let Some(g) = rows.iter().find(|g| g.id == id).cloned() {
                    self.set_game_metadata(&id, load_metadata_for(&id, &g));
                    self.games = Box::new([g]);
                } else {
                    self.games = Default::default();
                    self.clear_metadata();
                }
            }
            Nav::Settings => {
                self.games = Default::default();
                self.clear_metadata();
            }
        }
    }

    /// Reload the selected game's full row + metadata (Game page holding).
    /// Empty selection or unknown id clears both.
    pub(crate) fn reload_selected_row(&mut self) {
        let Some(id) = self.selected.clone() else {
            self.games = Default::default();
            self.clear_metadata();
            return;
        };
        match load_game_row(&id) {
            Some(g) => {
                self.set_game_metadata(&id, load_metadata_for(&id, &g));
                self.games = Box::new([g]);
            }
            None => {
                self.games = Default::default();
                self.clear_metadata();
            }
        }
    }
}
