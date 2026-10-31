use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::ArgView;

use super::super::theme::types;
use super::super::widgets;
use super::super::{ArgsEdit, Shell};

impl Shell {
    /// Open the shared editor on `target`, pre-filled with the stored
    /// value. One editor at a time: opening replaces any open row.
    pub(crate) fn open_args_editor(
        view: Entity<Self>,
        target: ArgsEdit,
        prefill: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        view.update(cx, |this, cx| {
            this.args_editing = Some(target);
            cx.notify();
        });
        let input = view.read(cx).args_input.clone();
        input.update(cx, |inp, cx| {
            inp.set_value(prefill, window, cx);
        });
    }

    /// Save from the editor: a freeform value goes through `set_arg`
    /// (`set_game_arg`); custom text goes through `set_custom_args`, with
    /// empty text meaning clear (core rejects empty custom text).
    pub(crate) fn apply_args_edit(&mut self, value: String, cx: &mut Context<Self>) {
        match self.args_editing.take() {
            Some(ArgsEdit::Freeform(id)) => self.set_game_arg(id, Some(value), cx),
            Some(ArgsEdit::Custom) if value.trim().is_empty() => self.clear_custom_args(cx),
            Some(ArgsEdit::Custom) => self.set_custom_args(value, cx),
            None => {}
        }
        cx.notify();
    }

    /// Clear the edited row (editor Clear, or the custom trash while
    /// the editor targets custom): a freeform row is disabled (the value
    /// is kept, like flag-off and enum None); custom text is removed.
    /// Taking `args_editing` closes the editor so its text cannot be
    /// saved back over the clear.
    pub(crate) fn clear_args_edit(&mut self, cx: &mut Context<Self>) {
        match self.args_editing.take() {
            Some(ArgsEdit::Freeform(id)) => self.disable_game_args(vec![id], cx),
            Some(ArgsEdit::Custom) => self.clear_custom_args(cx),
            None => {}
        }
        cx.notify();
    }

    /// The shared editor row: input plus Save/Cancel, with Clear while
    /// the freeform row it targets is enabled.
    pub(crate) fn args_editor(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let show_clear = match &self.args_editing {
            Some(ArgsEdit::Freeform(id)) => self
                .arg_rows
                .iter()
                .find(|r| &r.id == id)
                .is_some_and(|r| r.enabled),
            _ => false,
        };
        let save_view = view.clone();
        let cancel_view = view.clone();
        let clear_view = view;
        v_flex()
            .gap_1()
            .child(Styled::h(
                Input::new(&self.args_input)
                    .xsmall()
                    .text_size(types(cx).body_md.size)
                    .cleanable(true),
                types(cx).control_h,
            ))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("args-edit-save", cx)
                            .child(widgets::blabel(self.strings.get("gui-action-save"), cx))
                            .on_click(move |_, _, cx| {
                                let value =
                                    save_view.read(cx).args_input.read(cx).value().to_string();
                                save_view.update(cx, |this, cx| {
                                    this.apply_args_edit(value, cx);
                                });
                            }),
                    )
                    .child(
                        widgets::btn("args-edit-cancel", cx)
                            .ghost()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.args_editing = None;
                                    cx.notify();
                                });
                            }),
                    )
                    .when(show_clear, |this| {
                        this.child(
                            widgets::btn("args-edit-clear", cx)
                                .ghost()
                                .child(widgets::blabel(self.strings.get("gui-action-clear"), cx))
                                .on_click(move |_, _, cx| {
                                    clear_view.update(cx, |this, cx| {
                                        this.clear_args_edit(cx);
                                    });
                                }),
                        )
                    }),
            )
    }

    /// Freeform row control: the enabled value (else None, like the
    /// enum menu); click opens the shared editor pre-filled with the
    /// stored value, so a disabled row keeps its text for re-enable.
    pub(crate) fn arg_freeform_btn(
        &self,
        row: &ArgView,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let label: SharedString = if row.enabled && !row.value.is_empty() {
            row.value.clone().into()
        } else {
            self.strings.get("gui-args-none").into()
        };
        let id = row.id.clone();
        let prefill = row.value.clone();
        widgets::btn(format!("arg-free-{id}"), cx)
            .secondary()
            .child(widgets::blabel(label, cx))
            .on_click(move |_, window, cx| {
                Self::open_args_editor(
                    view.clone(),
                    ArgsEdit::Freeform(id.clone()),
                    prefill.clone(),
                    window,
                    cx,
                );
            })
    }

    /// The custom-arguments row: Add when empty, Edit + trash when set,
    /// with the current string as a muted line and the shared editor
    /// under the row while it targets custom.
    pub(crate) fn args_custom_block(&self, view: Entity<Self>, cx: &App) -> gpui_kit::AnyElement {
        let custom = self.args_custom.trim().to_string();
        let editing = self.args_editing == Some(ArgsEdit::Custom);
        let control: gpui_kit::AnyElement = if custom.is_empty() {
            let add_view = view.clone();
            widgets::btn("args-custom-add", cx)
                .secondary()
                .child(widgets::blabel(self.strings.get("gui-action-add"), cx))
                .on_click(move |_, window, cx| {
                    Self::open_args_editor(
                        add_view.clone(),
                        ArgsEdit::Custom,
                        String::new(),
                        window,
                        cx,
                    );
                })
                .into_any_element()
        } else {
            let edit_view = view.clone();
            let rm_view = view.clone();
            let prefill = custom.clone();
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    widgets::btn("args-custom-edit", cx)
                        .secondary()
                        .child(widgets::blabel(self.strings.get("gui-action-edit"), cx))
                        .on_click(move |_, window, cx| {
                            Self::open_args_editor(
                                edit_view.clone(),
                                ArgsEdit::Custom,
                                prefill.clone(),
                                window,
                                cx,
                            );
                        }),
                )
                .child(widgets::destroy_btn(
                    "args-custom-rm",
                    self.strings.get("gui-action-remove"),
                    move |_, _, cx| {
                        rm_view.update(cx, |this, cx| {
                            if this.args_editing == Some(ArgsEdit::Custom) {
                                this.clear_args_edit(cx);
                            } else {
                                this.clear_custom_args(cx);
                            }
                        });
                    },
                    cx,
                ))
                .into_any_element()
        };
        v_flex()
            .gap_1()
            .child(widgets::labeled_row(
                "args-custom",
                self.strings.get("gui-args-custom"),
                None,
                control,
                cx,
            ))
            .when(!custom.is_empty() && !editing, |this| {
                this.child(widgets::muted(custom.clone(), cx))
            })
            .when(editing, |this| this.child(self.args_editor(view, cx)))
            .into_any_element()
    }
}
