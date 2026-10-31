use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::{data_dir, ensure_profiles, open_db_shared, UNMODDED_ID};

use super::super::super::widgets;
use super::super::super::{rt_block, ClientStopOp, Note, ProfileRetry, Shell};

impl Shell {
    pub(crate) fn profile_restore_ui(&mut self, game: String, cx: &mut Context<Self>) {
        self.profile_restore_game = None;
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game, true)?;
                        // Capture the write result so the restart below
                        // runs even when the write fails.
                        let line = match tuxgt_core::restore_launch(&data_dir(), &game) {
                            Ok(line) => tuxgt_core::sync_session(&pool, &data_dir(), &host, &game)
                                .await
                                .map(|_| line)
                                .map_err(|e| e.to_string()),
                            Err(e) => Err(e.to_string()),
                        };
                        let restart = match stopped {
                            Some(client) => client.restart_detached().map_err(|e| e.to_string()),
                            None => Ok(String::new()),
                        };
                        Ok((game, line, restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, line, restart)) => {
                        this.status = match (line, restart) {
                            (Ok(line), Ok(msg)) if !msg.is_empty() => {
                                format!("{line} · {msg}")
                            }
                            (Ok(line), Ok(_)) => line,
                            (Ok(_), Err(e)) | (Err(e), _) => {
                                this.pending_note = Some(Note::Err(e.clone()));
                                e
                            }
                        };
                        this.applied
                            .insert(id.clone(), tuxgt_core::has_apply_record(&data_dir(), &id));
                        this.reload_armed_state(&id);
                        this.refresh_launch_state(cx);
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn profile_consent_card(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let confirm = self.strings.get("gui-action-confirm");
        let cancel = self.strings.get("gui-action-cancel");
        let ask = self.strings.get("gui-profile-add-default");
        v_flex()
            .id("profile-consent")
            .gap_1()
            .p_2()
            .rounded(px(4.))
            .border_1()
            .border_color(cx.theme().primary)
            .bg(cx.theme().tab_bar_segmented)
            .child(widgets::muted(ask, cx))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("profile-consent-yes", cx)
                            .primary()
                            .child(widgets::blabel(confirm, cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.accept_profile_consent(cx));
                                }
                            }),
                    )
                    .child(
                        widgets::btn("profile-consent-no", cx)
                            .ghost()
                            .child(widgets::blabel(cancel, cx))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.cancel_profile_consent(cx));
                            }),
                    ),
            )
    }

    pub(crate) fn accept_profile_consent(&mut self, cx: &mut Context<Self>) {
        let Some(retry) = self.profile_retry.take() else {
            return;
        };
        let Some(game) = self.selected.clone() else {
            return;
        };
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        tuxgt_core::activate_default_for_write(&pool, &data_dir(), &game_bg, false)
                            .await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(out) => {
                        // Activating Default replaced the working copy;
                        // drop any open args editor (see
                        // `after_profile_switch`).
                        this.args_editing = None;
                        this.reload_profiles(cx);
                        this.refresh_mods(&game, cx);
                        this.reload_knob_counts(cx);
                        this.reload_game_env(cx);
                        this.reload_armed_state(&game);
                        if out.restore_deferred {
                            this.queue_profile_restore(&game, cx);
                        }
                        if out.rearm_deferred {
                            this.queue_profile_rearm(&game, cx);
                        }
                        (retry.run)(this, cx);
                    }
                    Err(tuxgt_core::Error::NeedConfirm(msg))
                        if msg.starts_with("profile-client-stop:") =>
                    {
                        this.profile_retry = Some(retry);
                        if !this.park_client_stop(
                            &game,
                            ClientStopOp::ProfileActivateDefault,
                            true,
                            cx,
                        ) {
                            this.profile_activate_stop_ui(game.clone(), cx);
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn queue_profile_restore(&mut self, game: &str, cx: &mut Context<Self>) {
        self.profile_restore_game = Some(game.to_string());
        self.flush_profile_restore(cx);
    }

    /// Show the client-stop card when the queue is free. A card that is
    /// already up stays; this restore waits until that card is cleared.
    pub(crate) fn flush_profile_restore(&mut self, cx: &mut Context<Self>) {
        let Some(game) = self.profile_restore_game.clone() else {
            return;
        };
        if self.pending_confirm.is_some() {
            return;
        }
        if self.park_client_stop(&game, ClientStopOp::ProfileRestore, true, cx) {
            return;
        }
        self.profile_restore_ui(game, cx);
    }

    pub(crate) fn cancel_profile_consent(&mut self, cx: &mut Context<Self>) {
        self.profile_retry = None;
        self.install_current = None;
        self.install_queue.clear();
        self.install_hold = None;
        self.uninstall_current = None;
        self.uninstall_queue.clear();
        self.uninstall_done = 0;
        cx.notify();
    }

    pub(crate) fn active_unmodded(&self) -> bool {
        self.profiles
            .as_ref()
            .is_some_and(|set| set.active == UNMODDED_ID)
    }

    pub(crate) fn note_saved_default(&mut self, was_unmodded: bool, cx: &mut Context<Self>) {
        if !was_unmodded {
            return;
        }
        self.pending_note = Some(Note::Ok(self.strings.get("gui-profile-saved-default")));
        self.reload_profiles(cx);
    }

    pub(crate) fn hold_profile_retry(
        &mut self,
        err: &tuxgt_core::Error,
        retry: impl FnOnce(&mut Shell, &mut Context<Shell>) + 'static,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(err, tuxgt_core::Error::NeedProfile(_)) {
            return false;
        }
        self.profile_retry = Some(ProfileRetry {
            run: Box::new(retry),
        });
        cx.notify();
        true
    }

    pub(crate) fn reload_profiles(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.profiles = None;
            return;
        };
        let id_bg = id.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    rt_block(async {
                        let dir = data_dir();
                        let pool = open_db_shared(&dir).await?;
                        ensure_profiles(&pool, &dir, &id_bg).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                match loaded {
                    Ok(set) => this.profiles = Some(set),
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
