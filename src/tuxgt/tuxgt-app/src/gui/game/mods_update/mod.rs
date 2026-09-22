mod resync;

use super::*;
use gpui_kit::*;
use tuxgt_core::{
    data_dir, ensure_update_baseline, install_instance, open_db_shared, stage_status, Error,
    FetchProgress, FluentArgs, InstallOpts, UpdateStatus,
};

use super::super::{notice::NoticeKind, rt_block, ConfirmOp, PendingConfirm, Shell};

impl Shell {
    /// Queue one background update repair, unless a result is already cached
    /// or a check is in flight for this (game, instance). E76: the task runs
    /// `ensure_update_baseline` (provenance/cache self-heal, then a
    /// redownload repair install when nothing is on disk) instead of a bare
    /// read-only `check_update`.
    pub(crate) fn spawn_update_check(
        &mut self,
        game: &str,
        instance: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "spawn-update-check", game, instance);
        let key = (game.to_string(), instance.to_string());
        if self.mod_updates.contains_key(&key) || self.mod_update_pending.contains(&key) {
            return;
        }
        self.mod_update_pending.insert(key.clone());
        let (game_bg, inst_bg) = key.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let cfg = tuxgt_core::config_dir();
                        ensure_update_baseline(&pool, &data, &cfg, &game_bg, &inst_bg).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.mod_update_pending.remove(&key);
                // Dropped tab or game: the check is stale and the tab
                // re-checks on entry, so late results are discarded.
                if !(this.nav == super::Nav::Game
                    && this.selected.as_deref() == Some(key.0.as_str())
                    && this.game_tab == super::GameTab::Mods)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(report) => {
                        if report.installed {
                            // The repair install rewrote staging: reload
                            // this instance's rows so the card paints fresh.
                            let rows: Vec<super::StageRow> = stage_status(&data_dir(), &key.0)
                                .unwrap_or_default()
                                .into_iter()
                                .map(|l| super::StageRow {
                                    instance: l.instance,
                                    file: l.file,
                                    state: l.state,
                                })
                                .filter(|r| r.instance == key.1)
                                .collect();
                            this.merge_stage_rows(&key.0, rows);
                        }
                        this.mod_updates.insert(key, report.status);
                    }
                    Err(Error::NeedConfirm(msg)) => {
                        // Repair install hit foreign dests: E34 confirm card;
                        // the retry keeps redownload via ConfirmOp::Update.
                        // The card keeps a short note meanwhile.
                        let adapter = this
                            .mods
                            .get(&key.0)
                            .and_then(|rows| rows.iter().find(|r| r.instance == key.1))
                            .map(|r| r.adapter.clone())
                            .unwrap_or_else(|| this.adapter_choice.clone());
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: key.0.clone(),
                            instance: key.1.clone(),
                            op: ConfirmOp::Update { adapter },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                        this.mod_updates.insert(
                            key,
                            UpdateStatus::Unknown {
                                reason: "no install provenance recorded".into(),
                            },
                        );
                    }
                    // A failed repair is one short muted note (rendered via
                    // `short_update_reason`), never a game-id dump or CLI
                    // text. Update retries the redownload path.
                    Err(e) => {
                        this.mod_updates.insert(
                            key,
                            UpdateStatus::Unknown {
                                reason: e.to_string(),
                            },
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// R32 update: reinstall the instance with `--redownload` semantics so the
    /// new bytes land and provenance refreshes. Keep bits of surviving dests
    /// are preserved by core; foreign game-dir overwrites confirm first.
    /// `adapter` overrides the stored intent (confirm retry); `None` reuses it.
    /// `force_staging` (from a `ConfirmOp::UpdateForce` confirm only) lets the
    /// reinstall overwrite staged config edits the redownload wipes.
    pub(crate) fn update_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        adapter: Option<String>,
        yes: bool,
        force_staging: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "update-mod", game, instance);
        let game = game.to_string();
        let instance = instance.to_string();
        let inst_cb = instance.clone();
        let done = game.clone();
        let adapter = adapter
            .or_else(|| {
                self.mods
                    .get(&game)
                    .and_then(|rows| rows.iter().find(|r| r.instance == instance))
                    .map(|r| r.adapter.clone())
            })
            .unwrap_or_else(|| self.adapter_choice.clone());
        let adapter_cb = adapter.clone();
        let label = self.catalog_label(&instance);
        // E102: redownload got a Live card too (same one-per-(game, instance)
        // shape as Install).
        let cell = self.open_install_live(&game, &instance, &label, cx);
        let cell_bg = cell.clone();
        let label_done = label.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        // `gui.mod-config-edit`: the redownload wipes payload
                        // edits and overwrites staged touches, so a first run
                        // parks a confirm naming them instead of installing.
                        // Skipped once confirmed (`force_staging` rides the
                        // UpdateForce retry); `yes` stays false there so the
                        // foreign game-dir confirm below still fires.
                        if !yes && !force_staging {
                            let drifted = tuxgt_core::payload_drift(&data, &game, &instance)
                                .unwrap_or_default();
                            let touched: Vec<String> = tuxgt_core::stage_status(&data, &game)
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|l| {
                                    l.instance == instance
                                        && l.state == tuxgt_core::StageState::UserModified
                                })
                                .map(|l| l.file)
                                .collect();
                            if !drifted.is_empty() || !touched.is_empty() {
                                let mut dests = drifted;
                                dests.extend(touched);
                                dests.sort();
                                dests.dedup();
                                return Err(tuxgt_core::Error::NeedConfirm(format!(
                                    "config-overwrite: {}",
                                    dests.join(", ")
                                )));
                            }
                        }
                        let pool = open_db_shared(&data).await?;
                        let opts = InstallOpts {
                            adapter,
                            redownload: true,
                            with_requires: None,
                            yes,
                            force: force_staging,
                        };
                        let sink = |p: FetchProgress| {
                            if let Ok(mut slot) = cell_bg.lock() {
                                *slot = Some(p);
                            }
                        };
                        install_instance(
                            &pool,
                            &data,
                            &tuxgt_core::config_dir(),
                            &game,
                            &instance,
                            &opts,
                            Some(&sink),
                        )
                        .await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // E102: the card belongs to (game, instance), so it finishes
                // before the page-scoped early return below.
                let outcome = match &result {
                    Ok(_) => "updated",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "update-mod", game = done.as_str(), instance = inst_cb.as_str(), outcome);
                match &result {
                    Ok(_) => {
                        let mut args = FluentArgs::new();
                        args.set("label", label_done.clone());
                        let text = this.strings.get_args("gui-notice-installed", Some(&args));
                        this.finish_install_live(&done, &inst_cb, NoticeKind::Ok, text, cx);
                    }
                    Err(Error::NeedConfirm(_)) if !yes => {
                        // E34 park: the overwrite card is the follow-up.
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(e) => {
                        this.finish_install_live(
                            &done,
                            &inst_cb,
                            NoticeKind::Err,
                            format!("{e}"),
                            cx,
                        );
                    }
                }
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                match result {
                    Ok(m) => {
                        // Fresh bytes mean a fresh check next paint.
                        this.mod_updates.remove(&(done.clone(), inst_cb.clone()));
                        this.refresh_mods(&m.game, cx);
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        // `gui.mod-config-edit`: the marker parks the config
                        // leg (`foreign_done: false`); a foreign NeedConfirm
                        // on a forced run parks the foreign leg (same force,
                        // foreign note + dests). Unforced foreign confirms
                        // keep the existing Update path.
                        let (op, dests) = match msg.strip_prefix("config-overwrite:") {
                            Some(rest) => (
                                ConfirmOp::UpdateForce {
                                    adapter: adapter_cb.clone(),
                                    foreign_done: false,
                                },
                                rest.split(", ")
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string)
                                    .collect::<Vec<_>>(),
                            ),
                            None if force_staging => (
                                ConfirmOp::UpdateForce {
                                    adapter: adapter_cb.clone(),
                                    foreign_done: true,
                                },
                                confirm_dests(&msg),
                            ),
                            None => (
                                ConfirmOp::Update {
                                    adapter: adapter_cb.clone(),
                                },
                                confirm_dests(&msg),
                            ),
                        };
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op,
                            dests: dests.into_boxed_slice(),
                        });
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
