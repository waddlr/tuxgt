use super::*;
use gpui_kit::*;
use tuxgt_core::{
    config_dir, data_dir, install_instance, list_mods, open_db_shared, Error, FetchProgress,
    FluentArgs, InstallOpts,
};

use super::super::tray::InstallGuard;
use super::super::widgets;
use super::super::{
    notice::NoticeKind, rt_block, ConfirmOp, PendingConfirm, ReqDep, Shell, SlotChoiceOp,
};
use super::slot_picks::PickKind;

impl Shell {
    pub(crate) fn open_install_live(
        &mut self,
        game: &str,
        instance: &str,
        label: &str,
        cx: &mut Context<Self>,
    ) -> std::sync::Arc<std::sync::Mutex<Option<FetchProgress>>> {
        let key = (game.to_string(), instance.to_string());
        let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.install_live_seq += 1;
        let generation = self.install_live_seq;
        let id = match self.install_live.get(&key) {
            Some(live) => live.id,
            None => {
                let mut args = FluentArgs::new();
                args.set("label", label.to_string());
                let text = self.strings.get_args("gui-notice-installing", Some(&args));
                self.emit_live(NoticeKind::Info, text, cx)
            }
        };
        self.install_live.insert(
            key.clone(),
            InstallLive {
                id,
                generation,
                progress: cell.clone(),
            },
        );
        self.spawn_live_pump(key, id, generation, cx);
        cell
    }

    /// E102: read one install's cell every `LIVE_TICK_MS` and repaint the card
    /// when its whole percent moved. Exits once the card is finished or replaced.
    pub(crate) fn spawn_live_pump(
        &mut self,
        key: (String, String),
        id: u64,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(LIVE_TICK_MS))
                .await;
            let alive = this.update(cx, |this, cx| {
                // The cell clone ends the map borrow before the repaint.
                let Some(cell) = this
                    .install_live
                    .get(&key)
                    .filter(|l| l.generation == generation)
                    .map(|l| l.progress.clone())
                else {
                    return false;
                };
                let pct = cell
                    .lock()
                    .ok()
                    .and_then(|slot| *slot)
                    .and_then(|p| p.percent());
                this.set_live_progress(id, pct, cx);
                true
            });
            if !matches!(alive, Ok(true)) {
                break;
            }
        })
        .detach();
    }

    /// E102: finish one install's Live card into an Activity toast.
    pub(crate) fn finish_install_live(
        &mut self,
        game: &str,
        instance: &str,
        kind: NoticeKind,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let key = (game.to_string(), instance.to_string());
        let Some(live) = self.install_live.remove(&key) else {
            return;
        };
        self.finish_live(live.id, kind, text, cx);
    }

    /// E102: drop one install's Live card with no toast — the E34 park has its
    /// own follow-up (confirm card, or the status line) and must not toast too.
    pub(crate) fn drop_install_live(&mut self, game: &str, instance: &str, cx: &mut Context<Self>) {
        let key = (game.to_string(), instance.to_string());
        let Some(live) = self.install_live.remove(&key) else {
            return;
        };
        self.notices.drop_live(live.id);
        cx.notify();
    }

    /// Install the next queued instance. A `NeedConfirm` / `MissingRequires`
    /// leaves the rest of the batch parked behind the confirm card; an error
    /// drops it (the batch cannot continue on its own).
    pub(crate) fn continue_install_queue(&mut self, cx: &mut Context<Self>) {
        if self.install_current.is_some() {
            return;
        }
        let Some(game) = self.selected.clone() else {
            self.install_queue.clear();
            return;
        };
        if self.install_queue.is_empty() {
            return;
        }
        let next = self.install_queue.remove(0);
        self.install_current = Some((game.clone(), next.clone()));
        self.install_mod_ui(&game, &next, None, false, None, None, cx);
    }

    pub(crate) fn clear_install_current(&mut self, game: &str, instance: &str) {
        if self
            .install_current
            .as_ref()
            .is_some_and(|(g, i)| g == game && i == instance)
        {
            self.install_current = None;
        }
    }

    /// R12: a finished transfer stops owning the Hide veto, but the work it
    /// left behind in this window does not: a parked E34 confirm and the
    /// instances queued behind it would be dropped by a Hide. Hand the
    /// guard to the Shell in that case, so the veto survives exactly as
    /// long as the work does; with nothing parked the guard drops here and
    /// Close is free again.
    ///
    /// The hand-off always clears first: a batch that ran on past a park
    /// carries this transfer's guard, so keeping an earlier one would arm
    /// a veto no future drop ever releases.
    pub(crate) fn park_transfer_hold(&mut self, transfer: InstallGuard) {
        self.install_hold = None;
        let password_parked = matches!(
            self.pending_archive_password,
            Some(PendingArchivePassword::Install { .. })
        );
        if password_parked
            || install_work_parked(&self.install_queue, self.pending_confirm.as_ref())
        {
            self.install_hold = Some(transfer);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        with_requires: Option<String>,
        yes: bool,
        password: Option<String>,
        slot: Option<String>,
        cx: &mut Context<Self>,
    ) {
        // No adapter in the log: core resolves the game's persisted choice
        // and logs it on the install entry, so a stale UI value can never
        // be read as the one in flight.
        tracing::debug!(action = "install-mod", game, instance);
        let game = game.to_string();
        let instance = instance.to_string();
        let inst_cb = instance.clone();
        let done = game.clone();
        let label = self.catalog_label(&instance);
        // E102: the Live card replaces the old `Installing…` status line — one
        // card per (game, instance), finished into a success/error toast.
        let cell = self.open_install_live(&game, &instance, &label, cx);
        let cell_bg = cell.clone();
        let label_done = label.clone();
        let wr_bg = with_requires.clone();
        let tried_password = password.is_some();
        let password_bg = password;
        let slot_cb = slot.clone();
        let slot_bg = slot;
        // R37: no GUI override. The game's persisted choice is the single
        // source of truth, read by core at install time.
        // R12: this transfer outlives the window it was started in — the
        // bytes keep moving after a Hide, and the completion is reported
        // into whichever Shell exists then. The guard vetoes a Hide for
        // as long as work that lives only in this window is unfinished:
        // the download itself, then whatever it parks (an E34 confirm, the
        // rest of the batch) — see `park_transfer_hold`. It must be held
        // inside the spawned future: a local here would drop when this
        // function returns, before the transfer even starts.
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let opts = InstallOpts {
                            adapter: None,
                            redownload: false,
                            with_requires: wr_bg,
                            yes,
                            force: false,
                            password: password_bg,
                            slot: slot_bg,
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
            let _ = this.update(cx, move |this, cx| {
                // E102: finish this (game, instance) card before any page-scoped
                // early return — the toast belongs to the install, not the page.
                let outcome = match &result {
                    Ok(_) => "installed",
                    Err(Error::MissingRequires(_)) => "missing-requires",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(Error::NeedSlotChoice(_)) => "need-slot-choice",
                    Err(Error::ArchivePasswordRequired) => "need-password",
                    Err(_) => "error",
                };
                tracing::debug!(
                    action = "install-mod",
                    game = done.as_str(),
                    instance = inst_cb.as_str(),
                    outcome
                );
                match &result {
                    Ok(_) => {
                        let mut args = FluentArgs::new();
                        args.set("label", label_done.clone());
                        let text = this.strings.get_args("gui-notice-installed", Some(&args));
                        this.finish_install_live(&done, &inst_cb, NoticeKind::Ok, text, cx);
                    }
                    Err(Error::MissingRequires(_)) => {
                        // E34 park: the requires card (or the status line) is the
                        // follow-up; no error toast, and no card left hanging.
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(Error::NeedConfirm(_)) if !yes => {
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(Error::NeedSlotChoice(_)) => {
                        this.drop_install_live(&done, &inst_cb, cx);
                    }
                    Err(Error::ArchivePasswordRequired) => {
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
                let mine = this
                    .install_current
                    .as_ref()
                    .is_some_and(|(g, i)| g == &done && i == &inst_cb);
                let here = this.selected.as_deref() == Some(done.as_str());
                let other_inflight = this.install_current.is_some() && !mine;
                let Some(continue_queue) = install_spawn_apply(mine, here, other_inflight) else {
                    if mine {
                        this.install_current = None;
                        this.install_queue.clear();
                    }
                    this.park_transfer_hold(transfer);
                    return;
                };
                match result {
                    Ok(m) => {
                        if mine {
                            this.clear_install_current(&done, &inst_cb);
                        }
                        // Fresh provenance: drop this instance's verdict so
                        // the follow-up re-checks instead of serving stale.
                        this.invalidate_update(&m.game, &m.instance);
                        this.refresh_mods(&m.game, cx);
                        if continue_queue {
                            this.continue_install_queue(cx);
                        }
                    }
                    Err(Error::MissingRequires(t)) => {
                        // The catalog rows live on Settings; the Game page
                        // reads them transiently (install-error path only,
                        // like `install_id_info`). One card lists every
                        // missing dep; Install required queues them first.
                        let data = data_dir();
                        let list = list_mods(&config_dir(), &data).ok();
                        let manifests =
                            tuxgt_core::game_manifests(&data, &done).unwrap_or_default();
                        let mut deps: Vec<ReqDep> = list
                            .as_ref()
                            .map(|l| {
                                let mods: Vec<ClosureMod> = l
                                    .mods
                                    .iter()
                                    .map(|m| ClosureMod {
                                        id: &m.id,
                                        label: &m.label,
                                        mod_type: &m.mod_type,
                                        requires: &m.requires,
                                        enabled: m.enabled,
                                    })
                                    .collect();
                                let installed: Vec<(&str, &str)> = manifests
                                    .iter()
                                    .map(|m| (m.instance.as_str(), m.mod_type.as_str()))
                                    .collect();
                                missing_closure(&mods, &installed, &inst_cb)
                            })
                            .unwrap_or_default();
                        if deps.is_empty() || deps.iter().any(|d| d.candidates.is_empty()) {
                            // Unresolvable line (or nothing the GUI can
                            // see): fall back to the single miss that
                            // provoked the card.
                            let single: Vec<(String, String)> = list
                                .as_ref()
                                .map(|l| {
                                    requires_candidates(
                                        l.mods.iter().map(|i| {
                                            (
                                                i.id.as_str(),
                                                i.label.as_str(),
                                                i.mod_type.as_str(),
                                                i.enabled,
                                            )
                                        }),
                                        &t,
                                    )
                                })
                                .unwrap_or_default();
                            deps = if single.is_empty() {
                                Vec::new()
                            } else {
                                vec![ReqDep {
                                    req: t.clone(),
                                    chosen: single.first().map(|(id, _)| id.clone()),
                                    candidates: single.into_boxed_slice(),
                                }]
                            };
                        }
                        if deps.is_empty() {
                            let mut args = FluentArgs::new();
                            args.set(
                                "type",
                                widgets::id_label(widgets::ValKind::ModType, &t, &this.strings),
                            );
                            this.status = this
                                .strings
                                .get_args("gui-status-missing-requires", Some(&args));
                            if mine {
                                this.install_current = None;
                                this.install_queue.clear();
                            }
                        } else {
                            this.pending_confirm = Some(PendingConfirm::Requires {
                                game: done.clone(),
                                instance: inst_cb.clone(),
                                deps: deps.into_boxed_slice(),
                            });
                        }
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::Install {
                                with_requires,
                                slot: slot_cb,
                            },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                    }
                    Err(Error::NeedSlotChoice(msg)) => {
                        this.pending_confirm = Some(PendingConfirm::SlotChoice {
                            game: done.clone(),
                            op: SlotChoiceOp::Install {
                                target: inst_cb.clone(),
                                with_requires,
                            },
                            picks: this.picks_for(&done, confirm_dests(&msg), PickKind::Recipe),
                        });
                    }
                    Err(Error::ArchivePasswordRequired) => {
                        if tried_password {
                            this.pending_note = Some(Note::Warn(
                                this.strings.get("gui-archive-password-rejected"),
                            ));
                        }
                        this.pending_archive_password = Some(PendingArchivePassword::Install {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            adapter: None,
                            with_requires,
                            yes,
                            redownload: false,
                            force: false,
                            slot: slot_cb,
                        });
                        this.scroll_page_top();
                    }
                    Err(_) => {
                        // E102: the Live card already toasted the message; the
                        // queue stop below is unchanged (this batch cannot
                        // continue on its own).
                        if mine {
                            this.install_current = None;
                            this.install_queue.clear();
                        }
                    }
                }
                cx.notify();
                this.park_transfer_hold(transfer);
            });
        })
        .detach();
    }
}
