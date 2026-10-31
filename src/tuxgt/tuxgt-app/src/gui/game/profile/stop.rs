use gpui_kit::*;
use tuxgt_core::{
    activate_default_for_write, data_dir, delete_profile, open_db_shared, switch_profile,
    DEFAULT_ID,
};

use super::super::super::{rt_block, Note, Shell};

impl Shell {
    /// ClientStop Confirm for a profile switch: the stop was authorized
    /// by the card, so stop the client, run the switch with that consent,
    /// and restart the client even when the switch fails. Cancel never
    /// reaches here, so it leaves the switch unstarted.
    pub(crate) fn profile_switch_stop_ui(
        &mut self,
        game: String,
        target: String,
        name: String,
        yes_files: bool,
        yes_restore: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let target_bg = target.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game_bg, true)?;
                        let out = switch_profile(
                            &pool,
                            &data_dir(),
                            &game_bg,
                            &target_bg,
                            yes_files,
                            yes_restore,
                            true,
                        )
                        .await
                        .map_err(|e| e.to_string());
                        let restart = match stopped {
                            Some(client) => client.restart_detached().map_err(|e| e.to_string()),
                            None => Ok(String::new()),
                        };
                        Ok((game, name, out, restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, name, Ok(out), restart)) => {
                        this.after_profile_switch(
                            &id,
                            &name,
                            out.restore_deferred,
                            out.rearm_deferred,
                            cx,
                        );
                        this.note_client_restart(restart);
                    }
                    Ok((_, _, Err(e), restart)) => {
                        this.status = e;
                        match restart {
                            Ok(msg) if !msg.is_empty() => {
                                this.status = format!("{} · {msg}", this.status);
                            }
                            Ok(_) => {}
                            Err(re) => {
                                this.status = re.clone();
                                this.pending_note = Some(Note::Err(re));
                            }
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ClientStop Confirm for deleting the active profile: stop, switch
    /// to Default and delete under that consent, then restart. Cancel
    /// never reaches here, so it deletes nothing.
    pub(crate) fn profile_delete_stop_ui(
        &mut self,
        game: String,
        name: String,
        yes_files: bool,
        yes_restore: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let name_bg = name.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game_bg, true)?;
                        let out = delete_profile(
                            &pool,
                            &data_dir(),
                            &game_bg,
                            &name_bg,
                            yes_files,
                            yes_restore,
                            true,
                        )
                        .await
                        .map_err(|e| e.to_string());
                        let restart = match stopped {
                            Some(client) => client.restart_detached().map_err(|e| e.to_string()),
                            None => Ok(String::new()),
                        };
                        Ok((game, out, restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, Ok(out), restart)) => {
                        let active = out
                            .set
                            .by_id(DEFAULT_ID)
                            .map(|p| p.name.clone())
                            .unwrap_or_else(|| "Default".to_string());
                        this.after_profile_switch(
                            &id,
                            &active,
                            out.restore_deferred,
                            out.rearm_deferred,
                            cx,
                        );
                        this.note_client_restart(restart);
                    }
                    Ok((_, Err(e), restart)) => {
                        this.status = e;
                        match restart {
                            Ok(msg) if !msg.is_empty() => {
                                this.status = format!("{} · {msg}", this.status);
                            }
                            Ok(_) => {}
                            Err(re) => {
                                this.status = re.clone();
                                this.pending_note = Some(Note::Err(re));
                            }
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ClientStop Confirm for Add-to-Default: stop, activate Default
    /// under that consent, restart, then run the parked edit. Cancel
    /// clears the parked edit in `cancel_confirm` and never reaches here.
    pub(crate) fn profile_activate_stop_ui(&mut self, game: String, cx: &mut Context<Self>) {
        let Some(retry) = self.profile_retry.take() else {
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
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game_bg, true)?;
                        let out = activate_default_for_write(&pool, &data_dir(), &game_bg, true)
                            .await
                            .map_err(|e| e.to_string());
                        let restart = match stopped {
                            Some(client) => client.restart_detached().map_err(|e| e.to_string()),
                            None => Ok(String::new()),
                        };
                        Ok((game, out, restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((id, Ok(out), restart)) => {
                        // Activating Default replaced the working copy;
                        // drop any open args editor (see
                        // `after_profile_switch`).
                        this.args_editing = None;
                        this.reload_profiles(cx);
                        this.refresh_mods(&id, cx);
                        this.reload_knob_counts(cx);
                        this.reload_game_env(cx);
                        this.reload_armed_state(&id);
                        if out.restore_deferred {
                            this.queue_profile_restore(&id, cx);
                        }
                        if out.rearm_deferred {
                            this.queue_profile_rearm(&id, cx);
                        }
                        (retry.run)(this, cx);
                        this.note_client_restart(restart);
                    }
                    Ok((_, Err(e), restart)) => {
                        this.status = e;
                        match restart {
                            Ok(msg) if !msg.is_empty() => {
                                this.status = format!("{} · {msg}", this.status);
                            }
                            Ok(_) => {}
                            Err(re) => {
                                this.status = re.clone();
                                this.pending_note = Some(Note::Err(re));
                            }
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn note_client_restart(&mut self, restart: Result<String, String>) {
        match restart {
            Ok(msg) if !msg.is_empty() => self.status = msg,
            Ok(_) => {}
            Err(e) => {
                self.status = e.clone();
                self.pending_note = Some(Note::Err(e));
            }
        }
    }
}
