use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::StoreClient;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ClientStopOp, PendingConfirm, Shell, SlotChoiceOp, SlotPick};

impl Shell {
    /// Slot-choice card dropdown: the picked stem becomes its instance
    /// row's choice, like the Requires card radios.
    pub(crate) fn set_slot_choice_ui(
        &mut self,
        instance: String,
        slot: &str,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            action = "set-slot-choice",
            instance = instance.as_str(),
            slot
        );
        if let Some(pending) = self.pending_confirm.as_mut() {
            pending.set_slot_pick(&instance, slot);
        }
        cx.notify();
    }

    /// Dropdown rows for the slot-choice card: instance name plus a stem
    /// picker reusing the installed-card Slot menu.
    pub(crate) fn slot_choice_rows(
        &self,
        view: Entity<Self>,
        picks: &[SlotPick],
        warn_self: bool,
        cx: &App,
    ) -> Vec<AnyElement> {
        let b = cx.theme();
        let t = types(cx);
        picks
            .iter()
            .map(|pick| {
                let change_view = view.clone();
                let inst = pick.instance.clone();
                let unknown = self.strings.get("gui-slot-self-label");
                let shown =
                    super::super::slot_show::choice_label(&pick.slot, &pick.stock, &unknown);
                let stock = pick.stock.clone();
                let menu_unknown = unknown.clone();
                let row = h_flex()
                    .gap_2()
                    .items_center()
                    .child(widgets::mono(pick.instance.clone(), cx))
                    .child(
                        widgets::value_btn(
                            SharedString::from(format!("slot-choice-{inst}")),
                            shown,
                            move |menu, _, _| {
                                let mut menu = menu;
                                for (label, value) in
                                    super::super::slot_show::slot_menu(&stock, &menu_unknown)
                                {
                                    menu = menu.item(PopupMenuItem::new(label).on_click({
                                        let view = change_view.clone();
                                        let inst = inst.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.set_slot_choice_ui(inst.clone(), value, cx);
                                            });
                                        }
                                    }));
                                }
                                menu
                            },
                            cx,
                        )
                        .into_any_element(),
                    );
                let mut col = v_flex().gap_1().child(row);
                if warn_self && tuxgt_core::is_self_slot(&pick.slot) {
                    col = col.child(
                        div()
                            .tx(t.body_md)
                            .text_color(b.warning)
                            .child(self.strings.get("gui-slot-self-warning")),
                    );
                }
                col.into_any_element()
            })
            .collect()
    }

    /// `<self>` on an Install-adapter landing does not load. Preload does.
    pub(crate) fn slot_self_warns(&self, op: &SlotChoiceOp) -> bool {
        match op {
            SlotChoiceOp::AdapterConvert { adapter, .. } => tuxgt_core::is_install(adapter),
            SlotChoiceOp::Install { .. }
            | SlotChoiceOp::Update { .. }
            | SlotChoiceOp::UpdateForce { .. }
            | SlotChoiceOp::Resync { .. } => self
                .selected_game()
                .is_some_and(|g| tuxgt_core::is_install(&g.adapter)),
        }
    }

    /// Slot-choice Continue: the picks land, then the stashed op replays.
    /// Install/Update carry the pick in `InstallOpts`. A conversion to
    /// Install commits the picks before the guarded copy. A conversion to
    /// preload keeps the picks on that conversion so Cancel and a failed
    /// convert still have the proxy DLL.
    pub(crate) fn confirm_slot_choice(
        &mut self,
        game: String,
        op: SlotChoiceOp,
        picks: Box<[SlotPick]>,
        cx: &mut Context<Self>,
    ) {
        // The card paints on its own game tab, so a confirm from anywhere
        // else is stale: leave it parked.
        if self.selected.as_deref() != Some(game.as_str()) {
            return;
        }
        self.clear_pending_confirm();
        match op {
            SlotChoiceOp::Install {
                target,
                with_requires,
            } => {
                let Some(first) = picks.first() else {
                    return;
                };
                if first.instance == target {
                    // The named instance itself needs the name: retry it
                    // with the pick.
                    self.install_mod_ui(
                        &game,
                        &target,
                        with_requires,
                        false,
                        None,
                        Some(first.slot.clone()),
                        cx,
                    );
                } else {
                    // A nested require needs the name: install it first with
                    // the pick, then retry the target behind it (its require
                    // is satisfied by then, so neither leg repeats it).
                    self.install_queue.insert(0, target.clone());
                    self.install_current = Some((game.clone(), first.instance.clone()));
                    self.install_mod_ui(
                        &game,
                        &first.instance,
                        None,
                        false,
                        None,
                        Some(first.slot.clone()),
                        cx,
                    );
                }
            }
            SlotChoiceOp::Update { adapter } => {
                let Some(first) = picks.first() else {
                    return;
                };
                self.update_mod_ui(
                    &game,
                    &first.instance,
                    adapter,
                    false,
                    false,
                    None,
                    Some(first.slot.clone()),
                    cx,
                );
            }
            SlotChoiceOp::UpdateForce {
                adapter,
                foreign_done,
            } => {
                let Some(first) = picks.first() else {
                    return;
                };
                self.update_mod_ui(
                    &game,
                    &first.instance,
                    adapter,
                    foreign_done,
                    true,
                    None,
                    Some(first.slot.clone()),
                    cx,
                );
            }
            SlotChoiceOp::AdapterConvert { adapter, unhook } => {
                self.slot_choice_convert_ui(game, adapter, unhook, picks, cx);
            }
            SlotChoiceOp::Resync { instance } => {
                self.repick_then_resync(game, instance, picks, false, cx);
            }
        }
    }

    /// Install-to-preload rewrites the game dir (the old proxy comes out).
    /// That write waits until the store-client stop is confirmed, so Cancel
    /// of the stop leaves the game dir alone.
    pub(crate) fn defer_install_slot_write(current_install: bool, client_running: bool) -> bool {
        current_install && client_running
    }

    /// Conversion leg of a slot-choice Continue. Both modes' choices are
    /// saved inside the conversion, and the rename rolls back with it. A
    /// running store client is stopped only after that confirm, so Cancel
    /// writes no dest. An unhook card already authorized the stop.
    fn slot_choice_convert_ui(
        &mut self,
        game: String,
        adapter: String,
        unhook: bool,
        picks: Box<[SlotPick]>,
        cx: &mut Context<Self>,
    ) {
        let leaving_install = self
            .selected_game()
            .is_some_and(|g| tuxgt_core::is_install(&g.adapter));
        let client_running = StoreClient::for_game(&game).is_some_and(|c| c.running());
        let wait_for_stop = Self::defer_install_slot_write(leaving_install, client_running)
            || (client_running && !leaving_install && !unhook);
        if wait_for_stop {
            self.pending_confirm = Some(PendingConfirm::ClientStop {
                game,
                op: ClientStopOp::AdapterConvert {
                    adapter,
                    slots_chosen: true,
                    picks,
                },
            });
            cx.notify();
            return;
        }
        // Do not rename here. Overwrite consent and a failed convert both
        // have to see the names of the mode the game is still on.
        self.adapter_convert_op(game, adapter, unhook, false, unhook, true, picks, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::Shell;

    #[test]
    fn install_slot_write_waits_for_the_stop() {
        assert!(Shell::defer_install_slot_write(true, true));
        assert!(!Shell::defer_install_slot_write(true, false));
        assert!(!Shell::defer_install_slot_write(false, true));
        assert!(!Shell::defer_install_slot_write(false, false));
    }
}
