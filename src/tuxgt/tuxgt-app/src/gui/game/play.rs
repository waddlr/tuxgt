use gpui_kit::*;
use tuxgt_core::{
    apply_launch, data_dir, has_apply_record, open_db_shared, proton_ge_cachy, set_handle,
    FluentArgs,
};

use super::super::{rt_block, ClientStopOp, Shell};

impl Shell {
    pub(crate) fn launch_play(
        &mut self,
        game: Option<String>,
        apply_report: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = game.or_else(|| self.selected.clone()) else {
            self.status = self.strings.get("gui-status-select-game");
            cx.notify();
            return;
        };
        let launch_id = id.clone();
        let err_id = launch_id.clone();
        // E70: Play while handle is on needs the protonfixes `localfixes`
        // hook (the handled/loader inject path); handle-off Play is vanilla
        // and never warns. Snapshot the warning now so Enable & Play (which
        // arms handle first, then funnels through here) toasts exactly once
        // for the whole op. Warning only — the launch below always runs.
        tracing::debug!(action = "launch-play", game = id.as_str(), applied = apply_report.is_some());
        let health = if self.handle.get(&id).copied().unwrap_or(false) {
            self.host_health_note()
        } else {
            None
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        let paths = tuxgt_core::LaunchPaths::detect()?;
                        let spec = tuxgt_core::build_launch_spec(
                            &pool,
                            &host,
                            &launch_id,
                            &paths,
                            &data_dir(),
                        )
                        .await?;
                        let modded = tuxgt_core::game_manifests(&data_dir(), &launch_id)?
                            .iter()
                            .any(|m| m.enabled);
                        spec.command_detached().spawn().map_err(tuxgt_core::Error::Io)?;
                        let now = tuxgt_core::touch_last_played(&pool, &launch_id).await?;
                        Ok((launch_id, modded, now))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut args = FluentArgs::new();
                let outcome = match result {
                    Ok((gid, modded, now)) => {
                        if let Some(g) = this.games.iter_mut().find(|g| g.id == gid) {
                            g.last_played = Some(now);
                        }
                        // `recent` sort reads the stored base.
                        this.rebuild_base();
                        // E100: a client game only reaches the game with mods
                        // when a channel is armed; store Play with the handle
                        // off is the vanilla client, so it reads `unmodded`
                        // even with enabled instances.
                        let client = this.games.iter().any(|g| {
                            g.id == gid && (g.manager == "steam" || g.manager == "heroic")
                        });
                        let handled = this.handle.get(&gid).copied().unwrap_or(false);
                        let modded = modded && (!client || handled);
                        let name = this
                            .games
                            .iter()
                            .find(|g| g.id == gid)
                            .map(|g| g.display_name().to_string())
                            .unwrap_or_else(|| gid.clone());
                        args.set("name", name);
                        let key = if modded {
                            "gui-status-play-modded"
                        } else {
                            "gui-status-play-unmodded"
                        };
                        tracing::info!(game = gid.as_str(), modded, "spawned");
                        this.strings.get_args(key, Some(&args))
                    }
                    Err(e) => {
                        tracing::error!(game = err_id.as_str(), error = %e, "play failed");
                        args.set("error", e.to_string());
                        this.strings.get_args("gui-status-err-play", Some(&args))
                    }
                };
                this.status = match apply_report {
                    Some(report) => format!("{report} · {outcome}"),
                    None => outcome,
                };
                if let Some(n) = health {
                    this.pending_note = Some(n);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Enable & Play: arm Apply when argv wrappers are on, else Hook when
    /// GE/Cachy (no store write), else Update Launch Options. Play itself
    /// never changes mode; this is the only Play path that arms one.
    pub(crate) fn enable_and_play_ui(&mut self, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            self.status = self.strings.get("gui-status-select-game");
            cx.notify();
            return;
        };
        let needs = self.launch_needs;
        if !needs.channel_needed() {
            return;
        }
        // Click-time Proton + wrappers gate: the selection can move while the arm runs.
        let hook = needs.hook_preferred(proton_ge_cachy(
            self.selected_game().and_then(|g| g.proton.as_deref()),
        ));
        // Running-client guard via the shared park helper. The Hook arm
        // writes only through the auto-restore when a record exists.
        let writes_store = !hook || has_apply_record(&data_dir(), &game);
        if self.park_client_stop(&game, ClientStopOp::EnablePlay { hook }, writes_store, cx) {
            return;
        }
        self.enable_and_play_op_ui(game, hook, false, cx);
    }

    /// Post-guard Enable & Play op: arms one channel, then funnels into
    /// `launch_play`. `confirmed` is the ClientStop authorization the
    /// confirm card passes back as true.
    pub(crate) fn enable_and_play_op_ui(
        &mut self,
        game: String,
        hook: bool,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        let apply_id = game.clone();
        tracing::debug!(action = "enable-and-play", game = game.as_str(), hook);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        // Authorized stop via `stop_for_write`: without
                        // confirm a client that started mid-op aborts the
                        // write (nothing written). The Hook arm without a
                        // record touches no store file: set_handle runs bare.
                        let store_write =
                            !hook || has_apply_record(&data_dir(), &apply_id);
                        let stopped = if store_write {
                            tuxgt_core::StoreClient::stop_for_write(&apply_id, confirmed)?
                        } else {
                            None
                        };
                        let arm = if hook {
                            // E80: Hook restores the trampoline first when applied.
                            set_handle(&pool, &data_dir(), &host, &apply_id, true)
                                .await
                                .map(|_| None)
                        } else {
                            // E80: Apply clears Handle first, then writes the store.
                            apply_launch(&pool, &data_dir(), &host, &apply_id)
                                .await
                                .map(Some)
                        };
                        // The client was stopped on confirm: restart before
                        // reporting, even when the arm failed. Captured as a
                        // value so a restart failure funnels through
                        // launch_play below instead of early-returning with
                        // stale maps.
                        let restart = match stopped {
                            Some(c) => c.restart_detached(),
                            None => Ok(String::new()),
                        };
                        // Disk truth like the Launch Mode op: E80/disarm can
                        // mutate the channels under a background arm.
                        let handled = tuxgt_core::game_handle(&pool, &apply_id)
                            .await
                            .unwrap_or(false);
                        let applied = has_apply_record(&data_dir(), &apply_id);
                        Ok((apply_id, handled, applied, arm, restart, stopped.is_some()))
                    })
                })
                .await;
            match result {
                Ok((game, handled, applied, arm, restart, stopped)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.handle.insert(game.clone(), handled);
                        this.applied.insert(game.clone(), applied);
                        // E70: the funneled `launch_play` below toasts host
                        // health exactly once for the whole op on the Hook
                        // path (handle is already armed when it snapshots);
                        // the Apply path warns here.
                        let report = match arm {
                            Ok(r) => r,
                            Err(e) => {
                                // Arm failed; the client (when stopped) was
                                // already restarted above. Restart failure
                                // dominates the report.
                                this.status = match restart {
                                    Ok(_) => e.to_string(),
                                    Err(re) => {
                                        format!(
                                            "client restart failed: {re} (arm also failed: {e})"
                                        )
                                    }
                                };
                                cx.notify();
                                return;
                            }
                        };
                        if report.is_some() {
                            // Fresh client after our stop/restart: no toast.
                            if !stopped {
                                this.note_heroic_restart(&game);
                            }
                            this.note_host_health();
                        }
                        // A restart failure funnels into Play with the error
                        // dominating the report (maps above already reflect
                        // the successful arm).
                        let report = match restart {
                            Ok(msg) if msg.is_empty() => report,
                            Ok(msg) => Some(match report {
                                Some(r) => format!("{r} · {msg}"),
                                None => msg,
                            }),
                            Err(e) => Some(match report {
                                Some(r) => format!("{r} · client restart failed: {e}"),
                                None => format!("client restart failed: {e}"),
                            }),
                        };
                        // The arm ran: re-read the About reference so it
                        // shows disk truth.
                        this.refresh_launch_state(cx);
                        this.launch_play(Some(game.clone()), report, cx);
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.status = format!("{e}");
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}
