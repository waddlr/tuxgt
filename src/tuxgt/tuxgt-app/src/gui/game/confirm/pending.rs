use std::collections::HashMap;

use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, list_mods};

use super::super::super::{ClientStopOp, ConfirmOp, PendingConfirm, Shell};
use super::super::launch::LaunchMode;

impl Shell {
    pub(crate) fn confirm_pending(&mut self, cx: &mut Context<Self>) {
        let (game, instance) = match &self.pending_confirm {
            Some(PendingConfirm::Overwrite { game, instance, .. })
            | Some(PendingConfirm::Requires { game, instance, .. }) => {
                (game.as_str(), instance.as_str())
            }
            Some(PendingConfirm::SlotChoice { game, .. }) => (game.as_str(), "-"),
            _ => ("-", "-"),
        };
        tracing::debug!(action = "confirm-pending", game, instance);
        match self.pending_confirm.clone() {
            None => {}
            Some(PendingConfirm::Overwrite {
                game,
                instance,
                op,
                dests,
                ..
            }) => {
                self.clear_pending_confirm();
                match op {
                    ConfirmOp::Install {
                        with_requires,
                        slot,
                    } => {
                        self.install_mod_ui(&game, &instance, with_requires, true, None, slot, cx);
                    }
                    ConfirmOp::Update { adapter, slot } => {
                        self.update_mod_ui(&game, &instance, adapter, true, false, None, slot, cx);
                    }
                    ConfirmOp::UpdateForce {
                        adapter,
                        foreign_done,
                        slot,
                    } => {
                        // Config leg retries unforced-`yes` (a foreign
                        // NeedConfirm still parks as the foreign leg);
                        // foreign leg retries `yes` (both consents
                        // collected, force kept so the wipe lands).
                        self.update_mod_ui(
                            &game,
                            &instance,
                            adapter,
                            foreign_done,
                            true,
                            None,
                            slot,
                            cx,
                        );
                    }
                    ConfirmOp::Enable { on } => {
                        self.toggle_mod(&game, &instance, on, true, cx);
                    }
                    ConfirmOp::Uninstall => {
                        // `uninstall_current` survives the NeedConfirm park, so the
                        // success path resumes the queue without re-arming here.
                        self.uninstall_mod_ui(&game, &instance, true, cx);
                    }
                    ConfirmOp::FileKeep { dest, on } => {
                        self.toggle_file(&game, &instance, &dest, on, true, cx);
                    }
                    ConfirmOp::Slot { slot } => {
                        self.slot_change_ui(&game, &instance, &slot, true, cx);
                    }
                    // E69: host ops never ride `Overwrite`; this arm is
                    // unreachable, kept so the match stays exhaustive.
                    ConfirmOp::AdapterConvert {
                        adapter,
                        slots_chosen,
                        picks,
                        stop_confirmed,
                    } => {
                        // The card collected the foreign-overwrite consent.
                        // Picks stay on the op so the retry still renames;
                        // `stop_confirmed` is the earlier stop consent.
                        self.adapter_convert_op(
                            game,
                            adapter,
                            stop_confirmed,
                            true,
                            false,
                            slots_chosen,
                            picks,
                            cx,
                        );
                    }
                    ConfirmOp::AdapterConvertUnhook {
                        adapter,
                        slots_chosen,
                        picks,
                    } => {
                        // One-click hooked Install: the Continue authorized
                        // the stop, so one click covers unhook + conversion.
                        // Dests presence carries the foreign-overwrite
                        // consent from the second card (empty = unhook card).
                        self.adapter_convert_op(
                            game,
                            adapter,
                            true,
                            !dests.is_empty(),
                            true,
                            slots_chosen,
                            picks,
                            cx,
                        );
                    }
                    ConfirmOp::Resync { instance, picks } => {
                        self.repick_then_resync(game, instance, picks, true, cx);
                    }
                    // E69: host ops never ride `Overwrite`; this arm is
                    // unreachable, kept so the match stays exhaustive.
                    ConfirmOp::HostInstall | ConfirmOp::HostUninstall => {}
                }
            }
            Some(PendingConfirm::Host { op, .. }) => {
                self.clear_pending_confirm();
                match op {
                    ConfirmOp::HostInstall => self.host_install_ui(true, cx),
                    ConfirmOp::HostUninstall => self.host_uninstall_ui(true, cx),
                    _ => {}
                }
            }
            Some(PendingConfirm::Requires {
                game,
                instance,
                deps,
                ..
            }) => {
                // The card paints on its own game tab, so a confirm from
                // anywhere else is stale: leave it parked.
                if self.selected.as_deref() != Some(game.as_str()) {
                    return;
                }
                let chosen: Option<Vec<String>> = deps.iter().map(|d| d.chosen.clone()).collect();
                match chosen {
                    Some(chosen) => {
                        self.clear_pending_confirm();
                        // Queue every chosen dep topo-first, then the
                        // target, ahead of the rest of the batch.
                        let data = data_dir();
                        let info: HashMap<String, (String, Vec<String>)> =
                            list_mods(&config_dir(), &data)
                                .map(|l| {
                                    l.mods
                                        .iter()
                                        .map(|m| {
                                            (
                                                m.id.clone(),
                                                (m.mod_type.clone(), m.requires.to_vec()),
                                            )
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                        let rest: Vec<String> = std::mem::take(&mut self.install_queue);
                        self.install_queue =
                            super::super::splice_requires_order(&chosen, &instance, &rest, &info);
                        self.install_current = None;
                        self.continue_install_queue(cx);
                    }
                    None => {
                        self.status = self.strings.get("gui-status-pick-requires");
                        cx.notify();
                    }
                }
            }
            Some(PendingConfirm::SlotChoice { game, op, picks }) => {
                self.confirm_slot_choice(game, op, picks, cx);
            }
            Some(PendingConfirm::ClientStop { game, op }) => {
                self.clear_pending_confirm();
                match op {
                    ClientStopOp::ApplyMode => {
                        self.launch_mode_op_ui(game, LaunchMode::Apply, true, cx)
                    }
                    ClientStopOp::VanillaMode => {
                        self.launch_mode_op_ui(game, LaunchMode::Vanilla, true, cx)
                    }
                    ClientStopOp::HookMode => {
                        self.launch_mode_op_ui(game, LaunchMode::Hook, true, cx)
                    }
                    ClientStopOp::EnablePlay { hook } => {
                        self.enable_and_play_op_ui(game, hook, true, cx)
                    }
                    ClientStopOp::AdapterConvert {
                        adapter,
                        slots_chosen,
                        picks,
                    } => {
                        // The card authorized the stop only; a foreign
                        // game-dir overwrite still needs its own consent.
                        // Install-to-preload picks ride along and land
                        // after the stop, so Cancel wrote nothing.
                        self.adapter_convert_op(
                            game,
                            adapter,
                            true,
                            false,
                            false,
                            slots_chosen,
                            picks,
                            cx,
                        )
                    }
                }
            }
        }
    }
}
