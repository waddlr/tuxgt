use gpui_kit::*;
use tuxgt_core::{
    data_dir, ensure_update_baseline, open_db_shared, stage_status, Error, UpdateStatus,
};

use super::super::slot_picks::PickKind;
use super::super::{
    confirm_dests, rt_block, ConfirmOp, GameTab, Nav, PendingConfirm, Shell, SlotChoiceOp, StageRow,
};

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
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write (the baseline path can run a repair install).
        // Held inside the future: a local would drop on return, before the
        // op starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
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
                if !(this.nav == Nav::Game
                    && this.selected.as_deref() == Some(key.0.as_str())
                    && this.game_tab == GameTab::Mods)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(report) => {
                        if report.installed {
                            // The repair install rewrote staging: reload
                            // this instance's rows so the card paints fresh.
                            let rows: Vec<StageRow> = stage_status(&data_dir(), &key.0)
                                .unwrap_or_default()
                                .into_iter()
                                .map(|l| StageRow {
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
                        // R37: only an installed row knows its adapter.
                        let adapter = this
                            .mods
                            .get(&key.0)
                            .and_then(|rows| rows.iter().find(|r| r.instance == key.1))
                            .filter(|r| r.installed)
                            .map(|r| r.adapter.clone());
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: key.0.clone(),
                            instance: key.1.clone(),
                            op: ConfirmOp::Update {
                                adapter,
                                slot: None,
                            },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                        this.mod_updates.insert(
                            key,
                            UpdateStatus::Unknown {
                                reason: "no install provenance recorded".into(),
                            },
                        );
                    }
                    Err(Error::NeedSlotChoice(msg)) => {
                        // Repair install met an unnamed proxy: the slot card
                        // retries through the redownload path, which records
                        // provenance and clears the next check.
                        let adapter = this
                            .mods
                            .get(&key.0)
                            .and_then(|rows| rows.iter().find(|r| r.instance == key.1))
                            .filter(|r| r.installed)
                            .map(|r| r.adapter.clone());
                        this.pending_confirm = Some(PendingConfirm::SlotChoice {
                            game: key.0.clone(),
                            op: SlotChoiceOp::Update { adapter },
                            picks: this.picks_for(&key.0, confirm_dests(&msg), PickKind::Recipe),
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
}
