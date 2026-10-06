use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::{FluentArgs, StoreClient};

use super::super::super::widgets;
use super::super::super::{ConfirmOp, Nav, PendingConfirm, Shell, SlotChoiceOp};

impl Shell {
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
