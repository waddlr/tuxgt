use super::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::*;

use super::super::theme::types;
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn detect_dropdown(
        &self,
        snap: &tuxgt_core::DetectSnapshot,
        game_id: &str,
        values: &'static [(&'static str, &'static str)],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let field = snap.key;
        let game = game_id.to_string();
        let options: Vec<(&'static str, String)> = values
            .iter()
            .map(|(value, key)| (*value, self.strings.get(key)))
            .collect();
        widgets::value_btn(
            SharedString::from(format!("detv-{field}")),
            self.strings.get("gui-action-override"),
            {
                let view = view.clone();
                let unset = self.strings.get("gui-action-unset-detected");
                move |menu, _, _| {
                    let mut menu = menu.item(PopupMenuItem::new(unset.clone()).on_click({
                        let view = view.clone();
                        let game = game.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.save_override_ui(&game, field, None, cx);
                            });
                        }
                    }));
                    for (v, label) in options.clone() {
                        let view = view.clone();
                        let game = game.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.save_override_ui(&game, field, Some(v.to_string()), cx);
                            });
                        }));
                    }
                    menu
                }
            },
            cx,
        )
    }

    pub(crate) fn detect_path(
        &self,
        snap: &tuxgt_core::DetectSnapshot,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let field = snap.key;
        let game = game_id.to_string();
        let browse_game = game.clone();
        let dir = field == "prefix";
        let has_override = snap.override_.is_some();
        widgets::value_btn(
            SharedString::from(format!("detb-{field}")),
            self.strings.get("gui-action-override"),
            {
                let view = view.clone();
                let browse = self.strings.get("gui-action-browse");
                let unset = self.strings.get("gui-action-unset-detected");
                move |menu, _, _| {
                    let mut menu = menu.item(PopupMenuItem::new(browse.clone()).on_click({
                        let view = view.clone();
                        let browse_game = browse_game.clone();
                        move |_, _, cx| {
                            pick_override_path(view.clone(), browse_game.clone(), field, dir, cx)
                        }
                    }));
                    if has_override {
                        menu = menu.item(PopupMenuItem::new(unset.clone()).on_click({
                            let view = view.clone();
                            let game = game.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.save_override_ui(&game, field, None, cx);
                                });
                            }
                        }));
                    }
                    menu
                }
            },
            cx,
        )
    }

    pub(crate) fn detect_text(
        &self,
        snap: &tuxgt_core::DetectSnapshot,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let field = snap.key;
        let game = game_id.to_string();
        if self.override_edit == Some(field) {
            let save_view = view.clone();
            let save_game = game.clone();
            let cancel_view = view.clone();
            v_flex()
                .gap_1()
                .child(Styled::h(
                    Input::new(&self.override_input)
                        .xsmall()
                        .text_size(types(cx).body_md.size)
                        .cleanable(true),
                    types(cx).control_h,
                ))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            widgets::btn(SharedString::from(format!("dets-{field}")), cx)
                                .child(widgets::blabel(self.strings.get("gui-action-save"), cx))
                                .on_click(move |_, _, cx| {
                                    let value = {
                                        let this = save_view.read(cx);
                                        this.override_input.read(cx).value().to_string()
                                    };
                                    save_view.update(cx, |this, cx| {
                                        this.save_override_ui(&save_game, field, Some(value), cx);
                                    });
                                }),
                        )
                        .child(
                            widgets::btn(SharedString::from(format!("detx-{field}")), cx)
                                .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                                .on_click(move |_, _, cx| {
                                    cancel_view.update(cx, |this, cx| {
                                        this.override_edit = None;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .into_any_element()
        } else {
            let edit_cur = snap.override_.clone().unwrap_or_default();
            let has_override = snap.override_.is_some();
            widgets::value_btn(
                SharedString::from(format!("dete-{field}")),
                self.strings.get("gui-action-override"),
                {
                    let view = view.clone();
                    let edit = self.strings.get("gui-action-edit");
                    let unset = self.strings.get("gui-action-unset-detected");
                    move |menu, _, _| {
                        let mut menu = menu.item(PopupMenuItem::new(edit.clone()).on_click({
                            let view = view.clone();
                            let edit_cur = edit_cur.clone();
                            move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.override_edit = Some(field);
                                    this.redetect_confirm = false;
                                    cx.notify();
                                });
                                let input = view.read(cx).override_input.clone();
                                input.update(cx, |inp, cx| {
                                    inp.set_value(edit_cur.clone(), window, cx)
                                });
                            }
                        }));
                        if has_override {
                            menu = menu.item(PopupMenuItem::new(unset.clone()).on_click({
                                let view = view.clone();
                                let game = game.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.save_override_ui(&game, field, None, cx);
                                    });
                                }
                            }));
                        }
                        menu
                    }
                },
                cx,
            )
            .into_any_element()
        }
    }
}
