use gpui_kit::*;

use super::super::{rt_block, ClientStopOp, Shell};

#[derive(Clone)]
enum ArgOp {
    Set { id: String, value: Option<String> },
    Disable { ids: Vec<String> },
    SetCustom { value: String },
    ClearCustom,
}

impl Shell {
    pub(crate) fn set_game_arg(
        &mut self,
        id: String,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.write_args(ArgOp::Set { id, value }, cx);
    }

    pub(crate) fn disable_game_args(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.write_args(ArgOp::Disable { ids }, cx);
    }

    pub(crate) fn set_custom_args(&mut self, value: String, cx: &mut Context<Self>) {
        self.write_args(ArgOp::SetCustom { value }, cx);
    }

    pub(crate) fn clear_custom_args(&mut self, cx: &mut Context<Self>) {
        self.write_args(ArgOp::ClearCustom, cx);
    }

    fn write_args(&mut self, write: ArgOp, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let retry = write.clone();
        let transfer = self.hide_state.installing();
        let was_unmodded = self.active_unmodded();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = tuxgt_core::open_db_shared(&tuxgt_core::data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let dir = tuxgt_core::data_dir();
                        tuxgt_core::ensure_profile_write(&pool, &dir, &game).await?;
                        match write {
                            ArgOp::Set { id, value } => {
                                tuxgt_core::set_arg(
                                    &pool,
                                    &dir,
                                    &host,
                                    &game,
                                    &id,
                                    value.as_deref(),
                                )
                                .await?;
                            }
                            ArgOp::Disable { ids } => {
                                for id in ids {
                                    tuxgt_core::disable_arg(&pool, &dir, &host, &game, &id).await?;
                                }
                            }
                            ArgOp::SetCustom { value } => {
                                tuxgt_core::set_custom_args(&pool, &dir, &host, &game, &value)
                                    .await?;
                            }
                            ArgOp::ClearCustom => {
                                tuxgt_core::clear_custom(&pool, &dir, &host, &game).await?;
                            }
                        }
                        Ok(())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.note_saved_default(was_unmodded, cx);
                        this.reload_game_env(cx);
                        this.reload_launch_state(cx);
                    }
                    Err(e)
                        if this.hold_profile_retry(
                            &e,
                            {
                                let retry = retry.clone();
                                move |this, cx| this.write_args(retry, cx)
                            },
                            cx,
                        ) => {}
                    Err(e) => this.status = e.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn save_args_ui(&mut self, game: &str, cx: &mut Context<Self>) {
        if self.park_client_stop(game, ClientStopOp::ArgsSave, true, cx) {
            return;
        }
        self.save_args_op_ui(game.to_string(), false, cx);
    }

    pub(crate) fn save_args_op_ui(
        &mut self,
        game: String,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = tuxgt_core::open_db_shared(&tuxgt_core::data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game, confirmed)?;
                        let report =
                            tuxgt_core::save_args(&pool, &tuxgt_core::data_dir(), &host, &game)
                                .await;
                        let restart = match stopped {
                            Some(c) => c.restart_detached(),
                            None => Ok(String::new()),
                        };
                        Ok((report, restart))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((report, restart)) => {
                        this.status = match (report, restart) {
                            (Ok(line), Ok(msg)) if msg.is_empty() => line,
                            (Ok(line), Ok(msg)) => format!("{line} · {msg}"),
                            (Ok(line), Err(e)) => format!("{line} · client restart failed: {e}"),
                            (Err(e), Ok(_)) => e.to_string(),
                            (Err(e), Err(re)) => {
                                format!("client restart failed: {re} (save also failed: {e})")
                            }
                        };
                        this.reload_game_env(cx);
                        this.reload_launch_state(cx);
                    }
                    Err(e) => this.status = e.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn discard_args_ui(&mut self, game: &str, cx: &mut Context<Self>) {
        // Discard replaces the whole working copy; an open editor would
        // hold pre-discard text and could save it back over the restore.
        self.args_editing = None;
        cx.notify();
        let game = game.to_string();
        let retry_game = game.clone();
        let transfer = self.hide_state.installing();
        let was_unmodded = self.active_unmodded();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = tuxgt_core::open_db_shared(&tuxgt_core::data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let dir = tuxgt_core::data_dir();
                        tuxgt_core::ensure_profile_write(&pool, &dir, &game).await?;
                        tuxgt_core::discard(&pool, &dir, &host, &game).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.note_saved_default(was_unmodded, cx);
                        this.reload_game_env(cx);
                    }
                    Err(e)
                        if this.hold_profile_retry(
                            &e,
                            {
                                let retry_game = retry_game.clone();
                                move |this, cx| this.discard_args_ui(&retry_game, cx)
                            },
                            cx,
                        ) => {}
                    Err(e) => this.status = e.to_string(),
                }
                this.reload_launch_state(cx);
                cx.notify();
            });
        })
        .detach();
    }
}
