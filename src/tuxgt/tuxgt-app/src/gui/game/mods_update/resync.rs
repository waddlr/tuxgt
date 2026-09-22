use gpui_kit::*;
use tuxgt_core::{data_dir, resync_game, resync_instance, FluentArgs};

use super::super::{GameTab, Nav, Shell, StageRow};

impl Shell {
    /// R33 per-instance force re-sync: re-copy depot sources over
    /// user-touched staging, then refresh the stage cache + status line.
    pub(crate) fn resync_instance_ui(
        &mut self,
        game: &str,
        instance: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "resync-instance", game, instance);
        let game = game.to_string();
        let instance = instance.to_string();
        let done = game.clone();
        let inst_cb = instance.clone();
        let mut args = FluentArgs::new();
        args.set("instance", instance.clone());
        self.status = self
            .strings
            .get_args("gui-mod-status-resyncing", Some(&args));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let data = data_dir();
                    resync_instance(&data, &game, &instance, true).map(|lines| {
                        lines
                            .into_iter()
                            .map(|l| StageRow {
                                instance: l.instance,
                                file: l.file,
                                state: l.state,
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str())
                    || this.nav != Nav::Game
                    || this.game_tab != GameTab::Mods
                {
                    return;
                }
                let outcome = if result.is_ok() { "resynced" } else { "error" };
                tracing::debug!(action = "resync-instance", game = done.as_str(), instance = inst_cb.as_str(), outcome);
                match result {
                    Ok(rows) => {
                        let count = rows.len();
                        this.merge_stage_rows(&done, rows);
                        // Resync rewrites manifests; arming reads the held field.
                        this.reload_launch_state(cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", inst_cb.clone());
                        args.set("count", count.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-mod-status-resynced", Some(&args));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// R33 whole-game force re-sync: every installed instance, then the same
    /// stage-cache + status refresh as the per-instance path.
    pub(crate) fn resync_game_ui(&mut self, game: &str, cx: &mut Context<Self>) {
        tracing::debug!(action = "resync-game", game);
        let game = game.to_string();
        let done = game.clone();
        let mut args = FluentArgs::new();
        args.set("game", game.clone());
        self.status = self
            .strings
            .get_args("gui-mod-status-resyncing-game", Some(&args));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let data = data_dir();
                    resync_game(&data, &game, true).map(|lines| {
                        lines
                            .into_iter()
                            .map(|l| StageRow {
                                instance: l.instance,
                                file: l.file,
                                state: l.state,
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str())
                    || this.nav != Nav::Game
                    || this.game_tab != GameTab::Mods
                {
                    return;
                }
                let outcome = if result.is_ok() { "resynced" } else { "error" };
                tracing::debug!(action = "resync-game", game = done.as_str(), outcome);
                match result {
                    Ok(rows) => {
                        let count = rows.len();
                        this.bump_mod_extra_epoch(&done);
                        this.mod_stage.insert(done.clone(), rows);
                        // Resync rewrites manifests; arming reads the held field.
                        this.reload_launch_state(cx);
                        let mut args = FluentArgs::new();
                        args.set("game", done.clone());
                        args.set("count", count.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-mod-status-resynced-game", Some(&args));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Replace cached stage rows for one instance, keeping sibling instances.
    pub(crate) fn merge_stage_rows(&mut self, game: &str, rows: Vec<StageRow>) {
        self.bump_mod_extra_epoch(game);
        let entry = self.mod_stage.entry(game.to_string()).or_default();
        if let Some(first) = rows.first() {
            let inst = first.instance.clone();
            entry.retain(|r| r.instance != inst);
            entry.extend(rows);
        }
        entry.sort_by(|a, b| (&a.instance, &a.file).cmp(&(&b.instance, &b.file)));
    }
}
