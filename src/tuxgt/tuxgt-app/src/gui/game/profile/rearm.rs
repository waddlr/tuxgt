use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared};

use super::super::super::{rt_block, ClientStopOp, Note, Shell};

impl Shell {
    /// Deferred remembered-Apply re-arm: the stop was authorized by the
    /// ClientStop card, so stop, re-apply through the core helper (which
    /// clears the memory), then restart — the restore flow in reverse.
    pub(crate) fn profile_rearm_ui(&mut self, game: String, cx: &mut Context<Self>) {
        self.profile_rearm_game = None;
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let stopped = tuxgt_core::StoreClient::stop_for_write(&game, true)?;
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let line =
                            tuxgt_core::rearm_remembered_launch(&pool, &data_dir(), &host, &game)
                                .await?
                                .unwrap_or_default();
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
                        this.status = match restart {
                            Ok(msg) if !msg.is_empty() && !line.is_empty() => {
                                format!("{line} · {msg}")
                            }
                            Ok(msg) if !msg.is_empty() => msg,
                            Ok(_) => line,
                            Err(e) => {
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

    pub(crate) fn queue_profile_rearm(&mut self, game: &str, cx: &mut Context<Self>) {
        self.profile_rearm_game = Some(game.to_string());
        self.flush_profile_rearm(cx);
    }

    pub(crate) fn flush_profile_rearm(&mut self, cx: &mut Context<Self>) {
        let Some(game) = self.profile_rearm_game.clone() else {
            return;
        };
        if self.pending_confirm.is_some() {
            return;
        }
        if self.park_client_stop(&game, ClientStopOp::ProfileRearm, true, cx) {
            return;
        }
        self.profile_rearm_ui(game, cx);
    }
}
