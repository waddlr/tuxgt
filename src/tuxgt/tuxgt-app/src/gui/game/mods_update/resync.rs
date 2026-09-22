use gpui_kit::*;
use tuxgt_core::{
    apply_slot_picks, data_dir, open_db_shared, resync_game, resync_instance,
    resync_repick_instances, FluentArgs,
};

use super::super::super::{rt_block, ConfirmOp, PendingConfirm, SlotChoiceOp, SlotPick};
use super::super::slot_picks::PickKind;
use super::super::{confirm_dests, GameTab, Nav, Shell, StageRow};

enum ResyncStep {
    Rows(Vec<StageRow>),
    Repick(Vec<String>),
}

impl Shell {
    /// R33 per-instance force re-sync: re-copy depot sources over
    /// user-touched staging, then refresh the stage cache + status line.
    pub(crate) fn resync_instance_ui(
        &mut self,
        game: &str,
        instance: &str,
        cx: &mut Context<Self>,
    ) {
        self.resync_instance_go(game, instance, true, cx);
    }

    pub(crate) fn resync_instance_go(
        &mut self,
        game: &str,
        instance: &str,
        ask: bool,
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
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    let data = data_dir();
                    if ask {
                        let ids = resync_repick_instances(&data, &tuxgt_core::config_dir(), &game)?;
                        if !ids.is_empty() {
                            return Ok(ResyncStep::Repick(ids));
                        }
                    }
                    resync_instance(&data, &game, &instance, true).map(|lines| {
                        ResyncStep::Rows(
                            lines
                                .into_iter()
                                .map(|l| StageRow {
                                    instance: l.instance,
                                    file: l.file,
                                    state: l.state,
                                })
                                .collect(),
                        )
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
                tracing::debug!(
                    action = "resync-instance",
                    game = done.as_str(),
                    instance = inst_cb.as_str(),
                    outcome
                );
                match result {
                    Ok(ResyncStep::Repick(ids)) => {
                        this.pending_confirm = Some(PendingConfirm::SlotChoice {
                            game: done.clone(),
                            op: SlotChoiceOp::Resync {
                                instance: Some(inst_cb.clone()),
                            },
                            picks: this.picks_for(&done, ids, PickKind::Resync),
                        });
                        this.status = this.strings.get("gui-note-slot-choice");
                    }
                    Ok(ResyncStep::Rows(rows)) => {
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
        self.resync_game_go(game, true, cx);
    }

    pub(crate) fn resync_game_go(&mut self, game: &str, ask: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "resync-game", game);
        let game = game.to_string();
        let done = game.clone();
        let mut args = FluentArgs::new();
        args.set("game", game.clone());
        self.status = self
            .strings
            .get_args("gui-mod-status-resyncing-game", Some(&args));
        cx.notify();
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    let data = data_dir();
                    if ask {
                        let ids = resync_repick_instances(&data, &tuxgt_core::config_dir(), &game)?;
                        if !ids.is_empty() {
                            return Ok(ResyncStep::Repick(ids));
                        }
                    }
                    resync_game(&data, &game, true).map(|lines| {
                        ResyncStep::Rows(
                            lines
                                .into_iter()
                                .map(|l| StageRow {
                                    instance: l.instance,
                                    file: l.file,
                                    state: l.state,
                                })
                                .collect(),
                        )
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
                    Ok(ResyncStep::Repick(ids)) => {
                        this.pending_confirm = Some(PendingConfirm::SlotChoice {
                            game: done.clone(),
                            op: SlotChoiceOp::Resync { instance: None },
                            picks: this.picks_for(&done, ids, PickKind::Resync),
                        });
                        this.status = this.strings.get("gui-note-slot-choice");
                    }
                    Ok(ResyncStep::Rows(rows)) => {
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

    /// Apply a re-sync re-pick, then resync without asking again.
    /// `yes` is the overwrite consent. `NeedConfirm` parks that card and
    /// does not resync; Cancel drops the card and makes no second core call.
    /// Any other error restores the previous dests and skips the resync.
    pub(crate) fn repick_then_resync(
        &mut self,
        game: String,
        instance: Option<String>,
        picks: Box<[SlotPick]>,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        let game_bg = game.clone();
        let picks_park = picks.clone();
        let instance_park = instance.clone();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let pairs: Vec<(&str, &str)> = picks
                            .iter()
                            .map(|p| (p.instance.as_str(), p.slot.as_str()))
                            .collect();
                        apply_slot_picks(&pool, &data, &game_bg, &pairs, yes).await?;
                        Ok::<(), tuxgt_core::Error>(())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(game.as_str()) {
                    return;
                }
                match result {
                    Ok(()) => {
                        this.refresh_mods(&game, cx);
                        match &instance {
                            Some(inst) => this.resync_instance_go(&game, inst, false, cx),
                            None => this.resync_game_go(&game, false, cx),
                        }
                    }
                    Err(tuxgt_core::Error::NeedConfirm(msg)) if !yes => {
                        this.refresh_mods(&game, cx);
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: game.clone(),
                            instance: instance_park.clone().unwrap_or_default(),
                            op: ConfirmOp::Resync {
                                instance: instance_park,
                                picks: picks_park,
                            },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                        this.status = this.strings.get("gui-note-overwrite-confirm");
                        cx.notify();
                    }
                    Err(e) => {
                        this.refresh_mods(&game, cx);
                        this.status = format!("{e}");
                        cx.notify();
                    }
                }
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
