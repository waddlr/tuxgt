use super::*;
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, set_instance_slot, Error, FluentArgs};

use super::super::{rt_block, ConfirmOp, PendingConfirm, Shell};

impl Shell {
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
        // A mutation vetoes a Hide like a transfer: the stub handoff would
        // kill it mid-write. Held inside the future: a local would drop on
        // return, before the op starts.
        let transfer = self.hide_state.installing();
        let was_unmodded = self.active_unmodded();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
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
                tracing::debug!(
                    action = "slot-change",
                    game = done.as_str(),
                    instance = inst_cb.as_str(),
                    slot = slot_cb.as_str(),
                    outcome
                );
                match result {
                    Ok(m) => {
                        this.note_saved_default(was_unmodded, cx);
                        this.refresh_mods(&m.game, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", m.instance.clone());
                        args.set(
                            "slot",
                            super::super::slot_show::status_slot_name(
                                &slot_cb,
                                &m.files,
                                &this.strings.get("gui-slot-self-label"),
                            ),
                        );
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
                    Err(e)
                        if this.hold_profile_retry(
                            &e,
                            {
                                let game = done.clone();
                                let instance = inst_cb.clone();
                                let slot = slot_cb.clone();
                                move |this, cx| {
                                    this.slot_change_ui(&game, &instance, &slot, yes, cx)
                                }
                            },
                            cx,
                        ) => {}
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
