use super::*;
use gpui_kit::*;
use tuxgt_core::{
    config_dir, data_dir, install_instance, list_mods, open_db_shared, Error, FetchProgress,
    FluentArgs, InstallOpts,
};

use super::super::widgets;
use super::super::{notice::NoticeKind, rt_block, ConfirmOp, PendingConfirm, Shell};

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
        self.install_mod_ui(&game, &next, None, false, cx);
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

    pub(crate) fn install_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        with_requires: Option<String>,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "install-mod", game, instance, adapter = self.adapter_choice.as_str());
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
        let adapter_bg = self.adapter_choice.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let opts = InstallOpts {
                            adapter: adapter_bg.clone(),
                            redownload: false,
                            with_requires: wr_bg,
                            yes,
                            force: false,
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
                // E102: finish this (game, instance) card before any page-scoped
                // early return — the toast belongs to the install, not the page.
                let outcome = match &result {
                    Ok(_) => "installed",
                    Err(Error::MissingRequires(_)) => "missing-requires",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "install-mod", game = done.as_str(), instance = inst_cb.as_str(), outcome);
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
                    return;
                };
                match result {
                    Ok(m) => {
                        if mine {
                            this.clear_install_current(&done, &inst_cb);
                        }
                        this.refresh_mods(&m.game, cx);
                        if continue_queue {
                            this.continue_install_queue(cx);
                        }
                    }
                    Err(Error::MissingRequires(t)) => {
                        // The catalog rows live on Settings; the Game page
                        // reads the candidates transiently (install-error
                        // path only, like `install_id_info`).
                        let mut candidates: Vec<(String, String)> =
                            list_mods(&config_dir(), &data_dir())
                                .map(|list| {
                                    list.mods
                                        .iter()
                                        .filter(|i| i.enabled && i.mod_type == t)
                                        .map(|i| (i.id.clone(), i.label.clone()))
                                        .collect()
                                })
                                .unwrap_or_default();
                        candidates.sort();
                        candidates.dedup();
                        if candidates.is_empty() {
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
                            let chosen = if candidates.len() == 1 {
                                candidates.first().map(|(id, _)| id.clone())
                            } else {
                                None
                            };
                            this.pending_confirm = Some(PendingConfirm::Requires {
                                game: done.clone(),
                                instance: inst_cb.clone(),
                                req_type: t,
                                candidates: candidates.into_boxed_slice(),
                                chosen,
                            });
                        }
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::Install { with_requires },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
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
            });
        })
        .detach();
    }
}
