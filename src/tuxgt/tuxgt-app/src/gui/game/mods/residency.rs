use std::collections::HashSet;

use gpui_kit::{AppContext as _, Context};
use tuxgt_core::{data_dir, has_apply_record, load_conflicts, stage_status};

use super::super::{
    game_row, load_game_row, load_handle, load_mods_for, GameTab, Nav, Shell, StageRow,
};

impl Shell {
    /// Refresh the selected game's Mods rows after a catalog mutation, but
    /// only when that game's rows are resident (the Mods tab reloads on
    /// entry, so a dropped tab needs no refresh). The row is read
    /// transiently: full rows are not held on the Settings page.
    pub(crate) fn refresh_selected_mods(&mut self) {
        let Some(gid) = self.selected.clone() else {
            return;
        };
        self.mod_counts.insert(
            gid.clone(),
            tuxgt_core::enabled_mod_count(&data_dir(), &gid),
        );
        if !self.mods.contains_key(&gid) {
            return;
        }
        let row = load_game_row(&gid);
        self.mods.insert(
            gid.clone(),
            load_mods_for(&gid, row.as_ref(), &self.strings),
        );
    }

    /// Load the selected game's Mods rows (Mods-tab entry). The tab holds
    /// the selected game only; leaving drops them.
    pub(crate) fn enter_mods_tab(&mut self, game: &str) {
        self.mods.insert(
            game.to_string(),
            load_mods_for(game, game_row(&self.games, game), &self.strings),
        );
    }

    /// Drop held Mods-tab rows on tab/game leave. Counts stay: the sidebar
    /// and Library chips read them on every page. The stage/conflict caches
    /// stay per game: Mods-tab entry repaints the held pills instantly and
    /// rehashes in the background instead of hashing on the UI thread. The
    /// update verdicts stay too (same reason): entry re-checks only what is
    /// missing instead of flickering every card through the pending state.
    pub(crate) fn drop_all_mod_data(&mut self) {
        self.mods.clear();
    }

    /// E62 inline Install picker panel: applicable Mods without an Instance
    /// on this game (registered, enabled, `games` globs; E46 disabled
    /// omitted). Multi-select in check order; Cancel writes nothing.
    pub(crate) fn refresh_mods(&mut self, game: &str, cx: &mut Context<Self>) {
        // Counts always stay fresh (sidebar/Library chips); rows reload only
        // when this game's Mods tab is showing (it reloads on entry).
        let old_count = self.mod_counts.get(game).copied();
        let new_count = tuxgt_core::enabled_mod_count(&data_dir(), game);
        self.mod_counts.insert(game.to_string(), new_count);
        // Launch legality changes with manifests even when the Mods rows are
        // skipped (arming reads the held field, never the rows).
        self.reload_launch_state(cx);
        // Sidebar `mods_only` / `mods` sort derive from these counts; the
        // update poll lands here with unchanged counts, so skip that churn.
        if old_count != Some(new_count) {
            self.rebuild_base();
        }
        // A landed payload changes the game-card Files list. That card is
        // not on screen unless this game's Mods tab is showing, and the
        // Settings catalog Details disclosure shares `preview_open`.
        let on_mods = self.nav == Nav::Game
            && self.selected.as_deref() == Some(game)
            && self.game_tab == GameTab::Mods;
        if on_mods {
            self.file_preview_cache.clear();
            self.preview_errors.clear();
            self.preview_open.clear();
            self.preview_expanded.clear();
        } else {
            self.preview_open.retain(|k| catalog_detail_key(k));
            self.preview_expanded.retain(|k| catalog_detail_key(k));
            let open: Vec<String> = self
                .preview_open
                .iter()
                .filter_map(|k| k.strip_prefix("det:").map(str::to_string))
                .collect();
            self.file_preview_cache.clear();
            self.preview_errors.clear();
            for id in open {
                self.fill_file_preview(&id);
            }
            return;
        }
        self.mods.insert(
            game.to_string(),
            load_mods_for(game, game_row(&self.games, game), &self.strings),
        );
        self.reload_armed_state(game);
        // Update verdicts key on provenance (asset sha vs source), which
        // keep/env/enable/order/slot toggles never touch — so keep them
        // (stale-while-revalidate). Only drop entries whose instance is
        // gone (uninstall GC below); provenance-changing paths (install,
        // update, uninstall, provide/clear) invalidate explicitly, and the
        // catalog poll clears everything. The pending state paints nothing,
        // so a re-check never shifts the card.
        if let Some(rows) = self.mods.get(game) {
            let live: HashSet<String> = rows
                .iter()
                .filter(|r| r.installed)
                .map(|r| r.instance.clone())
                .collect();
            self.mod_updates
                .retain(|(g, i), _| g != game || live.contains(i));
            self.mod_update_pending
                .retain(|(g, i)| g != game || live.contains(i));
        }
        self.refresh_mod_extra(game, cx, true);
    }

    /// Drop one instance's cached update verdict (and in-flight mark) so the
    /// follow-up `refresh_mods` re-runs `check_update`. Provenance-changing
    /// paths only (install, update, uninstall); keep/env/enable/order/slot
    /// keep theirs.
    pub(crate) fn invalidate_update(&mut self, game: &str, instance: &str) {
        self.mod_updates
            .remove(&(game.to_string(), instance.to_string()));
        self.mod_update_pending
            .remove(&(game.to_string(), instance.to_string()));
    }

    /// Drop all cached update verdicts (and in-flight marks). Catalog poll
    /// and provide/clear flows: affected installs span games (or are unknown
    /// without a manifest walk), so clear everything — entry re-checks only
    /// missing keys, silently. In-flight checks still land normally; their
    /// completion inserts the fresh verdict.
    pub(crate) fn invalidate_all_updates(&mut self) {
        self.mod_updates.clear();
        self.mod_update_pending.clear();
    }

    /// E94: re-read armed Launch Mode from core. Core restores a channel the
    /// last mod/knob/wrapper just stopped needing (`sync_session`), so a
    /// mutation repaints the radio from truth instead of its own edits. A
    /// trampoline that vanished under the user carries the same client-restart
    /// notice a manual Not-hooked click gives.
    pub(crate) fn reload_armed_state(&mut self, game: &str) {
        let was_applied = self.applied.get(game).copied().unwrap_or(false);
        let applied = has_apply_record(&data_dir(), game);
        self.applied.insert(game.to_string(), applied);
        self.handle.insert(game.to_string(), load_handle(game));
        if was_applied && !applied {
            self.note_heroic_restart(game);
        }
    }
    /// R32/R33 follow-up for the Mods tab: reload the per-file staging cache
    /// synchronously (when `force=true`, for mutations), or for entry
    /// (`force=false`): if warm cache present for this game, return immediately
    /// without hashing and spawn a background rehash that inserts updated
    /// stage/conflicts + cx.notify() on completion. First visit (no prior
    /// cache) keeps today's synchronous compute; no placeholder UI states.
    /// Mutations (install/uninstall/resync/save/apply paths) always force
    /// sync recompute. Stale cache on external file edits is accepted (bg
    /// refresh heals one frame late); documented here.
    pub(crate) fn refresh_mod_extra(&mut self, game: &str, cx: &mut Context<Self>, force: bool) {
        let game_id = game.to_string();
        if !force
            && self.mod_stage.contains_key(&game_id)
            && self.mod_conflicts.contains_key(&game_id)
        {
            // warm: entry paints from cache; rehash off UI thread
            self.spawn_mod_extra_refresh(game, cx);
        } else {
            // sync path (first visit or force)
            let rows: Vec<StageRow> = stage_status(&data_dir(), &game_id)
                .unwrap_or_default()
                .into_iter()
                .map(|l| StageRow {
                    instance: l.instance,
                    file: l.file,
                    state: l.state,
                })
                .collect();
            self.mod_stage.insert(game_id.clone(), rows);
            let conflicts = load_conflicts(&data_dir(), &game_id).unwrap_or_default();
            self.mod_conflicts
                .insert(game_id.clone(), conflicts.into_boxed_slice());
            self.bump_mod_extra_epoch(&game_id);
        }
        // Both paths spawn the update checks (cheap guard inside spawn_update_check:
        // cached or in-flight keys skip, so toggles never re-check).
        // cx.notify() kept on this path for identical behavior to pre-fix.
        let installed: Vec<(String, String)> = self
            .mods
            .get(&game_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.installed)
            .map(|r| (game_id.clone(), r.instance))
            .collect();
        for (game, instance) in installed {
            self.spawn_update_check(&game, &instance, cx);
        }
        cx.notify();
    }

    pub(crate) fn bump_mod_extra_epoch(&mut self, game: &str) {
        let e = self.mod_extra_epoch.entry(game.to_string()).or_insert(0);
        *e += 1;
    }

    /// Background rehash of stage/conflicts for warm cache hit on entry.
    /// Matches the spawn pattern from spawn_update_check / spawn_launch_state:
    /// background_spawn the blocking work, then update+notify under guard
    /// that the game+Mods tab is still selected *and* the epoch at spawn time
    /// still matches (prevents stale overwrite from slow bg after a mutation
    /// or resync bumped the epoch).
    fn spawn_mod_extra_refresh(&mut self, game: &str, cx: &mut Context<Self>) {
        // Newest-wins: bump first so an older in-flight snapshot sees a moved
        // epoch and drops; capture after the bump for this snapshot.
        self.bump_mod_extra_epoch(game);
        let game_id = game.to_string();
        let captured = self.mod_extra_epoch.get(&game_id).copied().unwrap_or(0);
        let bg_game = game_id.clone();
        cx.spawn(async move |this, cx| {
            let (rows, conflicts) = cx
                .background_spawn(async move {
                    let rows: Vec<StageRow> = stage_status(&data_dir(), &bg_game)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|l| StageRow {
                            instance: l.instance,
                            file: l.file,
                            state: l.state,
                        })
                        .collect();
                    let conflicts = load_conflicts(&data_dir(), &bg_game)
                        .unwrap_or_default()
                        .into_boxed_slice();
                    (rows, conflicts)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !(this.nav == Nav::Game
                    && this.selected.as_deref() == Some(game_id.as_str())
                    && this.game_tab == GameTab::Mods)
                {
                    return;
                }
                let current = this.mod_extra_epoch.get(&game_id).copied().unwrap_or(0);
                if current != captured {
                    return;
                }
                this.bump_mod_extra_epoch(&game_id);
                this.mod_stage.insert(game_id.clone(), rows);
                this.mod_conflicts.insert(game_id.clone(), conflicts);
                cx.notify();
            });
        })
        .detach();
    }
}

/// Settings Mods Details keys. Game-card Files (`mod:`) is not one of them.
/// `fx:` is shared with the game card; page leave already clears it, so
/// keeping it here only preserves an Effects list opened inside Details.
fn catalog_detail_key(key: &str) -> bool {
    matches!(
        key.split_once(':').map(|(kind, _)| kind),
        Some("det" | "dfiles" | "dapply" | "dinc" | "dremap" | "drule" | "denv" | "dinst" | "fx")
    )
}

#[cfg(test)]
mod tests {
    use super::catalog_detail_key;

    #[test]
    fn catalog_detail_keys_survive_an_offscreen_game_rebuild() {
        assert!(catalog_detail_key("det:reshade"));
        assert!(catalog_detail_key("dfiles:reshade"));
        assert!(catalog_detail_key("fx:reshade"));
        assert!(catalog_detail_key("dinst:reshade"));
        assert!(!catalog_detail_key("mod:reshade"));
        assert!(!catalog_detail_key("reshade"));
    }
}
