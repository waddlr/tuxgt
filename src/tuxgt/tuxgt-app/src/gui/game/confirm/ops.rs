use gpui_kit::*;
use tuxgt_core::StoreClient;

use super::super::super::{ClientStopOp, ConfirmOp, PendingConfirm, Shell, SlotChoiceOp};

impl Shell {
    /// Cancel: the stashed op is dropped and, with it, any parked install
    /// batch — `install_queue` only runs again through an install result.
    pub(crate) fn cancel_confirm(&mut self, cx: &mut Context<Self>) {
        // ClientStop parks no queue: Cancel only clears the stash, never
        // the install/uninstall queues below.
        if matches!(
            self.pending_confirm,
            Some(PendingConfirm::ClientStop { .. })
        ) {
            self.clear_pending_confirm();
            cx.notify();
            return;
        }
        let (game, instance) = match &self.pending_confirm {
            Some(PendingConfirm::Overwrite { game, instance, .. })
            | Some(PendingConfirm::Requires { game, instance, .. }) => {
                (game.as_str(), instance.as_str())
            }
            Some(PendingConfirm::SlotChoice { game, .. }) => (game.as_str(), "-"),
            _ => ("-", "-"),
        };
        tracing::debug!(action = "cancel-confirm", game, instance);
        // Drop the parked queue item (install mirrors this) and clear both queues.
        if let Some(PendingConfirm::SlotChoice { op, .. }) = self.pending_confirm.clone() {
            // Game-scoped conversion: drop the stash only, like its
            // Overwrite sibling; install-ish ops drop the batch.
            match op {
                SlotChoiceOp::AdapterConvert { .. } => {}
                _ => {
                    self.install_queue.clear();
                    self.install_current = None;
                }
            }
        } else if let Some(PendingConfirm::Overwrite {
            game, instance, op, ..
        }) = self.pending_confirm.clone()
        {
            match op {
                ConfirmOp::Uninstall => {
                    if self
                        .uninstall_current
                        .as_ref()
                        .is_some_and(|(g, i)| g == &game && i == &instance)
                    {
                        self.uninstall_current = None;
                    }
                    self.uninstall_queue.clear();
                    self.uninstall_done = 0;
                }
                // R37: the conversion consent is game-scoped and owns no
                // queue, so Cancel drops the stash and nothing else. The
                // unhook consent is game-scoped the same way.
                ConfirmOp::AdapterConvert { .. }
                | ConfirmOp::AdapterConvertUnhook { .. }
                | ConfirmOp::Resync { .. } => {}
                _ => {
                    self.install_queue.clear();
                    self.install_current = None;
                }
            }
        } else {
            self.install_queue.clear();
            self.install_current = None;
            self.uninstall_queue.clear();
            self.uninstall_current = None;
            self.uninstall_done = 0;
        }
        self.clear_pending_confirm();
        cx.notify();
    }
    /// Running-client guard shared by the Launch Mode radio and Enable &
    /// Play: when the op would write the store under a live client, park a
    /// ClientStop confirm (Confirm stops the client first) and report true.
    /// Never overwrites an already-parked confirm: install batches park
    /// here too, and dropping one wedges its queue.
    pub(crate) fn park_client_stop(
        &mut self,
        game: &str,
        op: ClientStopOp,
        writes_store: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !writes_store {
            return false;
        }
        if self.pending_confirm.is_some() {
            self.status = self.strings.get("gui-status-resolve-confirm-first");
            cx.notify();
            return true;
        }
        if StoreClient::for_game(game).is_some_and(|c| c.running()) {
            self.pending_confirm = Some(PendingConfirm::ClientStop {
                game: game.to_string(),
                op,
            });
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn set_requires_choice(&mut self, req: String, id: String, cx: &mut Context<Self>) {
        tracing::debug!(
            action = "set-requires-choice",
            req = req.as_str(),
            source = id.as_str()
        );
        if let Some(pending) = self.pending_confirm.as_mut() {
            pending.set_requires_chosen(&req, &id);
        }
        cx.notify();
    }
}
