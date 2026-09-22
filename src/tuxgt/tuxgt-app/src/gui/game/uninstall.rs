use super::*;
use gpui_kit::*;
use tuxgt_core::{
    config_dir, data_dir, open_db_shared, set_file_keep, set_instance_enabled, set_instance_slot,
    uninstall_instance, Error, FluentArgs,
};

use super::super::{rt_block, ConfirmOp, PendingConfirm, Shell};

impl Shell {
    pub(crate) fn uninstall_visible_ui(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        tracing::debug!(action = "uninstall-visible", game = self.selected.as_deref().unwrap_or("-"), count = ids.len());
        if ids.is_empty() {
            return;
        }
        self.uninstall_queue = ids;
        self.uninstall_done = 0;
        self.continue_uninstall_queue(cx);
    }

    pub(crate) fn continue_uninstall_queue(&mut self, cx: &mut Context<Self>) {
        if self.uninstall_current.is_some() {
            return;
        }
        let Some(game) = self.selected.clone() else {
            self.uninstall_queue.clear();
            return;
        };
        if self.uninstall_queue.is_empty() {
            return;
        }
        let next = self.uninstall_queue.remove(0);
        self.uninstall_current = Some((game.clone(), next.clone()));
        self.uninstall_mod_ui(&game, &next, false, cx);
    }

    pub(crate) fn uninstall_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "uninstall-mod", game, instance);
        let game = game.to_string();
        let instance = instance.to_string();
        let inst_cb = instance.clone();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        uninstall_instance(&pool, &data, &config_dir(), &game, &instance, yes).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = match &result {
                    Ok(()) => "uninstalled",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "uninstall-mod", game = done.as_str(), instance = inst_cb.as_str(), outcome);
                match result {
                    Ok(()) => {
                        let queued = this
                            .uninstall_current
                            .as_ref()
                            .is_some_and(|(g, i)| g == &done && i == &inst_cb);
                        if queued {
                            this.uninstall_current = None;
                        }
                        this.refresh_mods(&done, cx);
                        if queued {
                            this.uninstall_done += 1;
                        }
                        if queued && !this.uninstall_queue.is_empty() {
                            this.continue_uninstall_queue(cx);
                        } else if queued {
                            let mut args = FluentArgs::new();
                            args.set("n", this.uninstall_done.to_string());
                            this.status = this
                                .strings
                                .get_args("gui-status-uninstalled-n", Some(&args));
                            this.uninstall_done = 0;
                        } else {
                            this.status = this.strings.get("gui-status-uninstalled");
                        }
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::Uninstall,
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                    }
                    Err(e) => {
                        this.status = format!("{e}");
                        this.uninstall_current = None;
                        this.uninstall_queue.clear();
                        this.uninstall_done = 0;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn toggle_mod(
        &mut self,
        game: &str,
        instance: &str,
        on: bool,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "toggle-mod", game, instance, on);
        let game = game.to_string();
        let instance = instance.to_string();
        let inst_cb = instance.clone();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        set_instance_enabled(&pool, &data, &game, &instance, on, yes).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = match &result {
                    Ok(_) => "toggled",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "toggle-mod", game = done.as_str(), instance = inst_cb.as_str(), outcome);
                match result {
                    Ok(m) => {
                        this.refresh_mods(&m.game, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", m.instance.clone());
                        let key = if m.enabled {
                            "gui-status-instance-enabled"
                        } else {
                            "gui-status-instance-disabled"
                        };
                        this.status = this.strings.get_args(key, Some(&args));
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::Enable { on },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                    }
                    Err(e) => {
                        this.refresh_mods(&done, cx);
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keep/omit one dest on an installed card (E64). Core rewrites the
    /// FileManifest, restages, prewires; foreign game-dir dests ask first.
    pub(crate) fn toggle_file(
        &mut self,
        game: &str,
        instance: &str,
        dest: &str,
        on: bool,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "toggle-file", game, instance, dest, on);
        let game = game.to_string();
        let instance = instance.to_string();
        let dest = dest.to_string();
        let inst_cb = instance.clone();
        let dest_cb = dest.clone();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        set_file_keep(&pool, &data, &game, &instance, &dest, on, yes).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = match &result {
                    Ok(_) => "file-toggled",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "toggle-file", game = done.as_str(), instance = inst_cb.as_str(), dest = dest_cb.as_str(), outcome);
                match result {
                    Ok(m) => {
                        this.refresh_mods(&m.game, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", m.instance.clone());
                        args.set("dest", dest_cb.clone());
                        let key = if on {
                            "gui-status-file-kept"
                        } else {
                            "gui-status-file-omitted"
                        };
                        this.status = this.strings.get_args(key, Some(&args));
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::FileKeep {
                                dest: dest_cb.clone(),
                                on,
                            },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                    }
                    Err(e) => {
                        this.refresh_mods(&done, cx);
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Rewrite the claiming Load dest to a proxy slot on an installed card
    /// (E91 `gui.mod-slot`). Core rewrites the dest, restages, prewires;
    /// foreign game-dir dests ask first via the E34 confirm card.
    pub(crate) fn slot_change_ui(
        &mut self,
        game: &str,
        instance: &str,
        slot: &str,
        yes: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "slot-change", game, instance, slot);
        let game = game.to_string();
        let instance = instance.to_string();
        let slot = slot.to_string();
        let inst_cb = instance.clone();
        let slot_cb = slot.clone();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        set_instance_slot(&pool, &data, &game, &instance, &slot, yes).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = match &result {
                    Ok(_) => "slot-changed",
                    Err(Error::NeedConfirm(_)) => "need-confirm",
                    Err(_) => "error",
                };
                tracing::debug!(action = "slot-change", game = done.as_str(), instance = inst_cb.as_str(), slot = slot_cb.as_str(), outcome);
                match result {
                    Ok(m) => {
                        this.refresh_mods(&m.game, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", m.instance.clone());
                        args.set("slot", slot_cb.clone());
                        this.status = this
                            .strings
                            .get_args("gui-status-slot-changed", Some(&args));
                    }
                    Err(Error::NeedConfirm(msg)) if !yes => {
                        this.pending_confirm = Some(PendingConfirm::Overwrite {
                            game: done.clone(),
                            instance: inst_cb.clone(),
                            op: ConfirmOp::Slot {
                                slot: slot_cb.clone(),
                            },
                            dests: confirm_dests(&msg).into_boxed_slice(),
                        });
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
