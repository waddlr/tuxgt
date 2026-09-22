use gpui_kit::component::button::{ButtonVariants as _, Toggle};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::{is_dll, FluentArgs};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{AddForm, Shell};

impl Shell {
    pub(crate) fn sync_add_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.add_form.clone() else {
            return;
        };
        let (name, dests) = {
            let f = form.read(cx);
            (
                f.name.clone(),
                f.files.iter().map(|x| x.dest.clone()).collect::<Vec<_>>(),
            )
        };
        self.instance_id_input.update(cx, |inp, cx| {
            inp.set_value(name, window, cx);
        });
        if self.add_dest_for.as_ref() != Some(&form) || self.add_dest_inputs.len() != dests.len() {
            let ph = self.strings.get("gui-placeholder-instance-dest");
            let inputs = dests
                .iter()
                .map(|dest| {
                    let inp = cx.new(|cx| InputState::new(window, cx).placeholder(ph.clone()));
                    inp.update(cx, |s, cx| {
                        s.set_value(dest.clone(), window, cx);
                    });
                    cx.subscribe(&inp, |_: &mut Shell, _, ev: &InputEvent, cx| {
                        if matches!(ev, InputEvent::Change) {
                            cx.notify();
                        }
                    })
                    .detach();
                    inp
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            self.add_dest_inputs = inputs;
            self.add_dest_for = Some(form);
        }
    }

    /// E91: inline Add/Rescan panel: Name row (Add only) / rescan id mono,
    /// single-pane file rows (checkbox + src + per-row dest input), Select
    /// Visible first row, Requires picker + gate note, footer Save + Cancel.
    /// Unchecked rows install nothing (no destination); dests flush from
    /// their inputs at Save. Slot is auto-detected (no Slot field).
    /// Bordered card like `mods_chrome`; the title row shows the title only
    /// and the footer carries Save + Cancel.
    pub(crate) fn add_panel_box(
        &self,
        view: Entity<Self>,
        form: Entity<AddForm>,
        cx: &App,
    ) -> AnyElement {
        let f = form.read(cx);
        let adding = f.rescan_id.is_none();
        let mod_type = f.mod_type.clone();
        let files = f.files.clone();
        let select_visible = f.select_visible;
        let includes = f.includes.clone();
        let requires = f.requires.clone();
        let rescan_id = f.rescan_id.clone();
        let title = if adding {
            let mut title_args = FluentArgs::new();
            title_args.set(
                "type",
                widgets::id_label(widgets::ValKind::ModType, &mod_type, &self.strings),
            );
            self.strings.get_args("gui-add-title", Some(&title_args))
        } else {
            let mut title_args = FluentArgs::new();
            title_args.set("id", rescan_id.clone().unwrap_or_default());
            self.strings
                .get_args("gui-add-rescan-title", Some(&title_args))
        };
        let name_label = self.strings.get("gui-add-name");
        let select_visible_label = self.strings.get("gui-action-select-visible");
        let requires_label = self.strings.get("gui-add-requires");
        let requires_pick = self.strings.get("gui-add-requires-pick");
        let gate_note = self.strings.get("gui-add-requires-need");
        let load_label = self.strings.get("gui-mode-load");
        let load_tip = self.strings.get("gui-mode-loaddll");
        let save_label = self.strings.get("gui-action-save");
        let cancel_label = self.strings.get("gui-action-cancel");
        let new_chip = self.strings.get("gui-chip-file-new");
        let requires_opts: Vec<(String, String)> = self
            .instances
            .iter()
            .filter(|i| i.mod_type == "reshade")
            .map(|i| (i.id.clone(), i.label.clone()))
            .collect();
        let name_input = self.instance_id_input.clone();
        let dest_inputs = self.add_dest_inputs.clone();
        let gate = super::add_requires_gate(&mod_type) && requires.is_empty();
        let name_empty = adding && name_input.read(cx).value().trim().is_empty();
        let blocked = gate || name_empty;
        let show_requires = adding && (super::add_requires_gate(&mod_type) || mod_type == "custom");
        let t = types(cx);
        let b = cx.theme();
        let list_cap = (t.label_lg.line + t.body_md.line + px(8.)) * 12.;
        let mut body = v_flex().gap_2();
        if adding {
            body = body.child(widgets::labeled_row(
                "add-name",
                name_label.clone(),
                None,
                Styled::h(
                    Input::new(&name_input).xsmall().text_size(t.body_md.size),
                    t.control_h,
                ),
                cx,
            ));
        } else {
            body = body.child(widgets::mono(rescan_id.clone().unwrap_or_default(), cx));
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        if files.len() > 1 {
            let form_c = form.clone();
            let view_c = view.clone();
            rows.push(
                h_flex()
                    .id("add-select-visible")
                    .w_full()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .py_1()
                    .cursor_pointer()
                    .on_click({
                        let form = form_c.clone();
                        let view = view_c.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_add_select_visible(form.clone(), cx);
                            });
                        }
                    })
                    .child(
                        div().w(px(24.)).flex_none().child(
                            Checkbox::new("add-select-visible-check")
                                .checked(select_visible)
                                .accessibility_label(select_visible_label.clone()),
                        ),
                    )
                    .child(div().tx(t.label_lg).child(select_visible_label.clone()))
                    .into_any_element(),
            );
        }
        for (i, x) in files.iter().enumerate() {
            let form_c = form.clone();
            let view_c = view.clone();
            let dest_inp = dest_inputs.get(i).cloned();
            let inc = includes.get(i).copied().unwrap_or(false);
            let dll = is_dll(&x.dest);
            let keep = x.keep;
            let is_new = x.is_new;
            let src = x.src.clone();
            let mode_control: AnyElement = if keep && dll {
                let mode_form = form.clone();
                let mode_view = view.clone();
                Toggle::new(SharedString::from(format!("add-load-{i}")))
                    .label(load_label.clone())
                    .tooltip(load_tip.clone())
                    .xsmall()
                    .checked(!inc)
                    .on_click(move |next, _, cx| {
                        cx.stop_propagation();
                        let include = !*next;
                        mode_form.update(cx, |f, _| {
                            if let Some(slot) = f.includes.get_mut(i) {
                                *slot = include;
                            }
                        });
                        mode_view.update(cx, |_, cx| cx.notify());
                    })
                    .into_any_element()
            } else {
                div().into_any_element()
            };
            let mode = div().w(px(72.)).flex_none().child(mode_control);
            let mut row = h_flex()
                .id(SharedString::from(format!("add-file-{i}")))
                .w_full()
                .gap_2()
                .items_center()
                .px_1()
                .py_1()
                .cursor_pointer()
                .on_click({
                    let form = form_c.clone();
                    let view = view_c.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.toggle_add_keep(form.clone(), i, cx);
                        });
                    }
                })
                .child(
                    div().w(px(24.)).flex_none().child(
                        Checkbox::new(SharedString::from(format!("add-keep-{i}")))
                            .checked(keep)
                            .accessibility_label(src.clone()),
                    ),
                )
                .child(mode)
                .child(
                    div()
                        .tx(t.label_lg)
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(src),
                )
                .when(is_new, |this| {
                    this.child(widgets::pill(
                        new_chip.clone(),
                        cx.theme().primary,
                        b.border,
                        cx,
                    ))
                });
            if keep {
                if let Some(inp) = dest_inp {
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("add-dest-{i}")))
                            .flex_1()
                            .min_w_0()
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(Styled::h(
                                Input::new(&inp).xsmall().text_size(t.body_md.size),
                                t.control_h,
                            )),
                    );
                }
            }
            rows.push(row.into_any_element());
        }
        body = body.child(
            v_flex()
                .id("add-files")
                .w_full()
                .max_h(list_cap)
                .overflow_y_scroll()
                .track_scroll(&self.inner_scrolls[0])
                .on_scroll_wheel(widgets::chain_inner(self.inner_scrolls[0].clone()))
                .children(rows),
        );
        if show_requires {
            let picked_label = if requires.is_empty() {
                requires_pick.clone()
            } else {
                requires.join(", ")
            };
            body = body.child(widgets::labeled_row(
                "add-requires",
                requires_label.clone(),
                None,
                widgets::value_btn(
                    "add-requires-pick",
                    picked_label,
                    {
                        let form = form.clone();
                        let view = view.clone();
                        let requires = requires.clone();
                        let opts = requires_opts.clone();
                        move |menu, _, _| {
                            let mut menu = menu;
                            for (id, label) in &opts {
                                let on = requires.iter().any(|r| r == id);
                                menu = menu.item(
                                    PopupMenuItem::new(label.clone()).checked(on).on_click({
                                        let form = form.clone();
                                        let view = view.clone();
                                        let id = id.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_add_require(
                                                    form.clone(),
                                                    id.clone(),
                                                    cx,
                                                );
                                            });
                                        }
                                    }),
                                );
                            }
                            menu
                        }
                    },
                    cx,
                ),
                cx,
            ));
            if gate {
                body = body.child(widgets::muted(gate_note.clone(), cx));
            }
        }
        let footer = h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                widgets::btn("add-save", cx)
                    .primary()
                    .child(widgets::blabel(save_label.clone(), cx))
                    .disabled(blocked)
                    .on_click({
                        let form = form.clone();
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.save_add_form(form.clone(), cx);
                            });
                        }
                    }),
            )
            .child(
                widgets::btn("add-cancel", cx)
                    .secondary()
                    .child(widgets::blabel(cancel_label.clone(), cx))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                tracing::debug!(action = "cancel-add-form");
                                this.add_form = None;
                                cx.notify();
                            });
                        }
                    }),
            );
        widgets::section_card("add-panel", cx)
            .child(widgets::section_title(title.clone(), cx))
            .child(body)
            .child(footer)
            .into_any_element()
    }
}
