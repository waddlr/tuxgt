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
    /// rehashes in the background instead of hashing on the UI thread.
    pub(crate) fn drop_all_mod_data(&mut self) {
        self.mods.clear();
        self.mod_updates.clear();
        self.mod_update_pending.clear();
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
        // Preview bodies and their open state go stale on any rebuild
        // (install, uninstall, resync — a landed payload flips the Files
        // button from the archive name to the walk), even when the rows
        // below are skipped off-tab.
        self.file_preview_cache.clear();
        self.preview_errors.clear();
        self.preview_open.clear();
        self.preview_expanded.clear();
        if !(self.nav == Nav::Game
            && self.selected.as_deref() == Some(game)
            && self.game_tab == GameTab::Mods)
        {
            return;
        }
        self.mods.insert(
            game.to_string(),
            load_mods_for(game, game_row(&self.games, game), &self.strings),
        );
        self.reload_armed_state(game);
        // R32: install/uninstall/reinstall change provenance; drop this game's
        // cached update checks (and in-flight marks) so the follow-up below
        // re-runs `check_update` instead of serving a stale Unknown/Available.
        self.mod_updates.retain(|(g, _), _| g != game);
        self.mod_update_pending.retain(|(g, _)| g != game);
        self.refresh_mod_extra(game, cx, true);
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
        // Both paths spawn the update checks (cheap guard inside spawn_update_check).
        // This ensures cards don't stick on "checking" note after tab leave/re-enter
        // (drop clears pending/updates but we preserve stage cache for warm).
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
