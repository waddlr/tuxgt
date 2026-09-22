use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::{FluentArgs, StoreClient};

use super::launch::LaunchMode;
use super::super::{ClientStopOp, ConfirmOp, Nav, PendingConfirm, Shell};
use super::super::widgets;

impl Shell {
    pub(crate) fn confirm_pending(&mut self, cx: &mut Context<Self>) {
        let (game, instance) = match &self.pending_confirm {
            Some(PendingConfirm::Overwrite { game, instance, .. })
            | Some(PendingConfirm::Requires { game, instance, .. }) => {
                (game.as_str(), instance.as_str())
            }
            _ => ("-", "-"),
        };
        tracing::debug!(action = "confirm-pending", game, instance);
        match self.pending_confirm.clone() {
            None => {}
            Some(PendingConfirm::Overwrite {
                game, instance, op, ..
            }) => {
                self.pending_confirm = None;
                match op {
                    ConfirmOp::Install { with_requires } => {
                        self.install_mod_ui(&game, &instance, with_requires, true, cx);
                    }
                    ConfirmOp::Update { adapter } => {
                        self.update_mod_ui(&game, &instance, Some(adapter), true, false, cx);
                    }
                    ConfirmOp::UpdateForce { adapter, foreign_done } => {
                        // Config leg retries unforced-`yes` (a foreign
                        // NeedConfirm still parks as the foreign leg);
                        // foreign leg retries `yes` (both consents
                        // collected, force kept so the wipe lands).
                        self.update_mod_ui(&game, &instance, Some(adapter), foreign_done, true, cx);
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
                    ConfirmOp::HostInstall | ConfirmOp::HostUninstall => {}
                }
            }
            Some(PendingConfirm::Host { op, .. }) => {
                self.pending_confirm = None;
                match op {
                    ConfirmOp::HostInstall => self.host_install_ui(true, cx),
                    ConfirmOp::HostUninstall => self.host_uninstall_ui(true, cx),
                    _ => {}
                }
            }
            Some(PendingConfirm::Requires {
                game,
                instance,
                chosen,
                ..
            }) => match chosen {
                Some(req) => {
                    self.pending_confirm = None;
                    self.install_mod_ui(&game, &instance, Some(req), false, cx);
                }
                None => {
                    self.status = self.strings.get("gui-status-pick-requires");
                    cx.notify();
                }
            },
            Some(PendingConfirm::ClientStop { game, op }) => {
                self.pending_confirm = None;
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
            self.pending_confirm = None;
            cx.notify();
            return;
        }
        let (game, instance) = match &self.pending_confirm {
            Some(PendingConfirm::Overwrite { game, instance, .. })
            | Some(PendingConfirm::Requires { game, instance, .. }) => {
                (game.as_str(), instance.as_str())
            }
            _ => ("-", "-"),
        };
        tracing::debug!(action = "cancel-confirm", game, instance);
        // Drop the parked queue item (install mirrors this) and clear both queues.
        if let Some(PendingConfirm::Overwrite {
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
        self.pending_confirm = None;
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

    pub(crate) fn set_requires_choice(&mut self, id: String, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-requires-choice", source = id.as_str());
        if let Some(PendingConfirm::Requires { chosen, .. }) = self.pending_confirm.as_mut() {
            *chosen = Some(id);
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
            PendingConfirm::Overwrite { op, dests, .. } => {
                // `gui.mod-config-edit`: the config leg names the wipe
                // (payload edits discarded, staged edits overwritten); the
                // foreign leg names foreign overwrites like every other op.
                let note_key = match op {
                    ConfirmOp::UpdateForce { foreign_done: false, .. } => {
                        "gui-confirm-config-overwrite"
                    }
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
            PendingConfirm::Requires {
                req_type,
                candidates,
                chosen,
                ..
            } => {
                let req_label =
                    widgets::id_label(widgets::ValKind::ModType, &req_type, &self.strings);
                let mut args = FluentArgs::new();
                args.set("type", req_label.clone());
                let note = v_flex()
                    .gap_1()
                    .child(widgets::muted(
                        self.strings.get_args("gui-note-requires", Some(&args)),
                        cx,
                    ))
                    .child(widgets::mono(req_label, cx))
                    .into_any_element();
                let picks = candidates
                    .iter()
                    .map(|(id, label)| {
                        let picked = chosen.as_deref() == Some(id.as_str());
                        let pick_view = view.clone();
                        let pick_id = id.clone();
                        let btn = widgets::btn(SharedString::from(format!("reqpick-{id}")), cx);
                        let btn = if picked {
                            btn.primary()
                        } else {
                            btn.secondary()
                        };
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(btn.child(widgets::blabel(label.clone(), cx)).on_click(
                                move |_, _, cx| {
                                    pick_view.update(cx, |this, cx| {
                                        this.set_requires_choice(pick_id.clone(), cx);
                                    });
                                },
                            ))
                            .child(widgets::mono(id.clone(), cx))
                            .into_any_element()
                    })
                    .collect();
                (note, picks, self.strings.get("gui-action-install-requires"))
            }
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
        v_flex()
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
            )
            .into_any_element()
    }
    /// Running-client stop confirm as a shell-root modal. The Launch Mode
    /// radio lives on General but the old inline card only painted on
    /// Mods, so the prompt surfaced on the wrong tab. Mounted in
    /// `render.rs` next to the notice layer: dim + centered card over
    /// every game tab. Click-outside cancels; Cancel mutates nothing.
    pub(crate) fn client_stop_layer(&self, view: Entity<Self>, cx: &App) -> Option<AnyElement> {
        let Some(PendingConfirm::ClientStop { game, .. }) = self.pending_confirm.as_ref() else {
            return None;
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
/// a colon-free prefix (`foreign game-dir dests`, `config-overwrite`).
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

/// Truncated dest label for the Load-conflicts section (60 chars + `…`).
pub(crate) fn dest_short(dest: &str) -> String {
    const CAP: usize = 60;
    if dest.chars().count() <= CAP {
        dest.to_string()
    } else {
        format!("{}…", dest.chars().take(CAP).collect::<String>())
    }
}
