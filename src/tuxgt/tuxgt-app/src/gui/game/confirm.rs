use std::collections::HashMap;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, list_mods, FluentArgs, StoreClient};

use super::super::widgets;
use super::super::{ClientStopOp, ConfirmOp, Nav, PendingConfirm, Shell, SlotChoiceOp};
use super::launch::LaunchMode;

impl Shell {
    /// R12: consuming the parked confirm releases the veto it held — the
    /// op Confirm retries takes a guard of its own, and Cancel drops the
    /// batch. Every site that clears the stash goes through here so the
    /// veto and the work it protects cannot drift apart.
    pub(crate) fn clear_pending_confirm(&mut self) {
        self.pending_confirm = None;
        self.install_hold = None;
    }
}

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
                            super::splice_requires_order(&chosen, &instance, &rest, &info);
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

    /// Inline confirm card (same style as the redetect confirm): dest/type
    /// lines plus Confirm / Cancel. Cancel makes no second core call.
    pub(crate) fn confirm_card(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(pending) = self.pending_confirm.clone() else {
            return div().into_any_element();
        };
        let confirm_view = view.clone();
        let cancel_view = view.clone();
        let (copy, dests, confirm_label): (AnyElement, Vec<AnyElement>, String) = match &pending {
            PendingConfirm::Overwrite {
                op: ConfirmOp::AdapterConvertUnhook { .. },
                dests,
                ..
            } if dests.is_empty() => (
                // One-click hooked Install: the exact unhook copy, no dest
                // lines. A re-parked unhook with dests falls through to the
                // overwrite card below.
                widgets::muted(self.strings.get("gui-note-adapter-unhook-confirm"), cx)
                    .into_any_element(),
                Vec::new(),
                self.strings.get("gui-action-confirm"),
            ),
            PendingConfirm::Overwrite { op, dests, .. } => {
                // `gui.mod-config-edit`: the config leg names the wipe
                // (payload edits discarded, staged edits overwritten); the
                // foreign leg names foreign overwrites like every other op.
                let note_key = match op {
                    ConfirmOp::UpdateForce {
                        foreign_done: false,
                        ..
                    } => "gui-confirm-config-overwrite",
                    _ => "gui-note-overwrite-confirm",
                };
                (
                    widgets::muted(self.strings.get(note_key), cx).into_any_element(),
                    dests
                        .iter()
                        .map(|d| widgets::mono(d.clone(), cx).into_any_element())
                        .collect(),
                    self.strings.get("gui-action-confirm"),
                )
            }
            // E69 Settings host install/uninstall: same card, host copy and
            // the stashed path list. Cancel makes no core call.
            PendingConfirm::Host { op, paths } => {
                let key = match op {
                    ConfirmOp::HostInstall => "gui-note-host-install-confirm",
                    _ => "gui-note-host-uninstall-confirm",
                };
                (
                    widgets::muted(self.strings.get(key), cx).into_any_element(),
                    paths
                        .iter()
                        .map(|d| widgets::mono(d.clone(), cx).into_any_element())
                        .collect(),
                    self.strings.get("gui-action-confirm"),
                )
            }
            PendingConfirm::Requires { deps, .. } => {
                let note =
                    widgets::muted(self.strings.get("gui-note-requires"), cx).into_any_element();
                // One line per missing dep: a single candidate is a plain
                // bullet, several share one line with a glyph radio each
                // (kit has no Radio component; glyph + label, click to
                // pick). Radio ids carry the dep so two lines offering
                // the same candidate stay distinct.
                let mut picks = Vec::with_capacity(deps.len());
                for dep in deps.iter() {
                    if dep.candidates.len() == 1 {
                        picks.push(
                            widgets::mono(format!("- {}", dep.candidates[0].1), cx)
                                .into_any_element(),
                        );
                    } else {
                        let mut row = h_flex().gap_2().child(widgets::mono("-".to_string(), cx));
                        for (id, label) in dep.candidates.iter() {
                            let pick_view = view.clone();
                            let pick_req = dep.req.clone();
                            let pick_id = id.clone();
                            let glyph = if dep.chosen.as_deref() == Some(id.as_str()) {
                                "●"
                            } else {
                                "○"
                            };
                            row = row.child(
                                h_flex()
                                    .id(SharedString::from(format!("reqradio-{}-{id}", dep.req)))
                                    .gap_1()
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        pick_view.update(cx, |this, cx| {
                                            this.set_requires_choice(
                                                pick_req.clone(),
                                                pick_id.clone(),
                                                cx,
                                            );
                                        });
                                    })
                                    .child(widgets::mono(glyph, cx))
                                    .child(widgets::mono(label.clone(), cx)),
                            );
                        }
                        picks.push(row.into_any_element());
                    }
                }
                (note, picks, self.strings.get("gui-action-install-requires"))
            }
            PendingConfirm::SlotChoice { op, picks, .. } => (
                widgets::muted(self.strings.get("gui-note-slot-choice"), cx).into_any_element(),
                self.slot_choice_rows(view.clone(), picks, self.slot_self_warns(op), cx),
                self.strings.get("gui-action-confirm"),
            ),
            PendingConfirm::ClientStop { game, .. } => {
                let client = StoreClient::for_game(game)
                    .map(|c| c.name().to_string())
                    .unwrap_or_default();
                let mut args = FluentArgs::new();
                args.set("client", client);
                (
                    widgets::muted(
                        self.strings.get_args("gui-note-client-stop", Some(&args)),
                        cx,
                    )
                    .into_any_element(),
                    vec![widgets::mono(game.clone(), cx).into_any_element()],
                    self.strings.get("gui-action-confirm"),
                )
            }
        };
        let card = v_flex()
            .id("install-confirm")
            .gap_1()
            .child(copy)
            .children(dests)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("install-confirm-yes", cx)
                            .primary()
                            .child(widgets::blabel(confirm_label, cx))
                            .on_click(move |_, _, cx| {
                                confirm_view.update(cx, |this, cx| {
                                    this.confirm_pending(cx);
                                });
                            }),
                    )
                    .child(
                        widgets::btn("install-confirm-no", cx)
                            .ghost()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_confirm(cx);
                                });
                            }),
                    ),
            );
        // The Requires follow-up is the one inline confirm that needs an
        // action to proceed: card it with a primary border so it reads
        // as blocking, like the registry consent card.
        if matches!(&pending, PendingConfirm::Requires { .. }) {
            card.p_2()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().tab_bar_segmented)
                .into_any_element()
        } else {
            card.into_any_element()
        }
    }
    /// Game-scoped confirms as a shell-root modal. The Launch Mode radio and
    /// the R37 adapter row both live on General, but the inline card only
    /// paints on Mods, so a prompt raised there surfaced on the wrong tab.
    /// Mounted in `render.rs` next to the notice layer: dim + centered card
    /// over every game tab. Click-outside cancels; Cancel mutates nothing.
    pub(crate) fn client_stop_layer(&self, view: Entity<Self>, cx: &App) -> Option<AnyElement> {
        let game = match self.pending_confirm.as_ref() {
            Some(PendingConfirm::ClientStop { game, .. }) => game,
            // R37: the adapter conversion is raised on General, so its
            // foreign-overwrite consent needs the same shell-root surface.
            // The one-click unhook consent is raised there too.
            Some(PendingConfirm::Overwrite {
                game,
                op: ConfirmOp::AdapterConvert { .. } | ConfirmOp::AdapterConvertUnhook { .. },
                ..
            }) => game,
            // Same for the conversion's slot-choice card; install-ish
            // slot cards paint inline on Mods with the other install
            // confirms.
            Some(PendingConfirm::SlotChoice {
                game,
                op: SlotChoiceOp::AdapterConvert { .. },
                ..
            }) => game,
            _ => return None,
        };
        // Navigating away (or switching games) merely hides the modal:
        // the park survives, so returning re-shows it.
        if self.nav != Nav::Game || self.selected.as_deref() != Some(game.as_str()) {
            return None;
        }
        let cancel_view = view.clone();
        Some(
            v_flex()
                .id("client-stop-modal")
                .absolute()
                .inset_0()
                .bg(rgba(0x0e0e10d9))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    cancel_view.update(cx, |this, cx| this.cancel_confirm(cx));
                })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .id("client-stop-panel")
                        .w(px(420.))
                        .gap_2()
                        .p_4()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().sidebar)
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(self.confirm_card(view, cx)),
                )
                .into_any_element(),
        )
    }
}

/// Dests after the message's first colon (`pfx:` dests stage verbatim, so
/// everything past the first colon is the list). All current producers use
/// a colon-free prefix (`foreign game-dir dests`, `config-overwrite`,
/// `need-slot` — whose list holds instance ids, not dests).
pub(crate) fn confirm_dests(msg: &str) -> Vec<String> {
    let dests: Vec<String> = msg
        .split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or("")
        .split(", ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if dests.is_empty() {
        vec![msg.to_string()]
    } else {
        dests
    }
}

/// Truncated dest label for conflict rows (60 chars + `…`).
pub(crate) fn dest_short(dest: &str) -> String {
    const CAP: usize = 60;
    if dest.chars().count() <= CAP {
        dest.to_string()
    } else {
        format!("{}…", dest.chars().take(CAP).collect::<String>())
    }
}
