use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::super::widgets;
use super::super::super::Shell;
use super::dialog::ProfileDialog;
use super::NewStart;

impl Shell {
    /// Profile dialogs as a shell-root modal (client-stop pattern):
    /// dim + centered card over every game tab. Click-outside, Escape,
    /// and Cancel drop the parked dialog without running its action.
    pub(crate) fn profile_dialog_layer(&self, view: Entity<Self>, cx: &App) -> Option<AnyElement> {
        let dialog = self.profile_dialog.as_ref()?;
        let content = match dialog {
            ProfileDialog::New {
                name_input,
                start,
                base,
                rows,
            } => self.profile_new_body(view.clone(), name_input, *start, base, rows, cx),
            ProfileDialog::Rename { input, .. } => {
                let mut body = v_flex()
                    .gap_2()
                    .child(widgets::section_title(
                        self.strings.get("gui-profile-rename"),
                        cx,
                    ))
                    .child(Input::new(input).xsmall());
                body = body.child(self.dialog_buttons(
                    view.clone(),
                    "profile-rename-save",
                    &self.strings.get("gui-action-save"),
                    "profile-rename-cancel",
                    &self.strings.get("gui-action-cancel"),
                    Shell::profile_rename_save,
                    cx,
                ));
                body.into_any_element()
            }
            ProfileDialog::Delete { name } => {
                let mut args = FluentArgs::new();
                args.set("name", name.clone());
                v_flex()
                    .gap_2()
                    .child(widgets::section_title(
                        self.strings.get_args("gui-profile-delete-ask", Some(&args)),
                        cx,
                    ))
                    .child(self.dialog_buttons(
                        view.clone(),
                        "profile-delete-yes",
                        &self.strings.get("gui-profile-delete"),
                        "profile-delete-no",
                        &self.strings.get("gui-action-cancel"),
                        Shell::profile_delete_confirm,
                        cx,
                    ))
                    .into_any_element()
            }
            ProfileDialog::Files { dests, .. } => v_flex()
                .gap_2()
                .child(widgets::section_title(
                    self.strings.get("gui-profile-files"),
                    cx,
                ))
                .child(
                    v_flex()
                        .id("profile-files-list")
                        .gap_1()
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .children(dests.iter().map(|d| widgets::mono(d.clone(), cx))),
                )
                .child(self.dialog_buttons(
                    view.clone(),
                    "profile-files-yes",
                    &self.strings.get("gui-action-confirm"),
                    "profile-files-no",
                    &self.strings.get("gui-action-cancel"),
                    Shell::profile_files_confirm,
                    cx,
                ))
                .into_any_element(),
            ProfileDialog::Restore { .. } => v_flex()
                .gap_2()
                .child(widgets::section_title(
                    self.strings.get("gui-profile-restore"),
                    cx,
                ))
                .child(self.dialog_buttons(
                    view.clone(),
                    "profile-files-yes",
                    &self.strings.get("gui-action-confirm"),
                    "profile-files-no",
                    &self.strings.get("gui-action-cancel"),
                    Shell::profile_restore_confirm,
                    cx,
                ))
                .into_any_element(),
        };
        let cancel_view = view.clone();
        Some(
            v_flex()
                .id("profile-dialog-modal")
                .absolute()
                .inset_0()
                .bg(rgba(0x0e0e10d9))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    cancel_view.update(cx, |this, cx| this.close_profile_dialog(window, cx));
                })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .id("profile-dialog-panel")
                        .w(px(420.))
                        .max_h(px(560.))
                        .gap_2()
                        .p_4()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().sidebar)
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(content),
                )
                .into_any_element(),
        )
    }

    fn profile_new_body(
        &self,
        view: Entity<Self>,
        name_input: &Entity<gpui_kit::component::input::InputState>,
        start: NewStart,
        base: &str,
        rows: &[(String, String)],
        cx: &App,
    ) -> AnyElement {
        let base_name = rows
            .iter()
            .find(|(id, _)| id == base)
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        let mut args = FluentArgs::new();
        args.set("base", base_name);
        let mut body = v_flex()
            .gap_2()
            .child(widgets::section_title(
                self.strings.get("gui-profile-new"),
                cx,
            ))
            .child(Input::new(name_input).xsmall())
            .child(choice_row(
                "profile-new-empty",
                &self.strings.get("gui-profile-empty"),
                start == NewStart::Empty,
                view.clone(),
                |this, cx| this.profile_new_set_start(NewStart::Empty, cx),
                cx,
            ));
        if !start.shows_base() {
            body = body.child(widgets::muted(
                self.strings.get("gui-profile-empty-note"),
                cx,
            ));
        }
        body = body.child(choice_row(
            "profile-new-copy",
            &self.strings.get("gui-profile-copy"),
            start == NewStart::Copy,
            view.clone(),
            |this, cx| this.profile_new_set_start(NewStart::Copy, cx),
            cx,
        ));
        if start.shows_base() {
            body = body.child(widgets::muted(
                self.strings.get_args("gui-profile-copy-note", Some(&args)),
                cx,
            ));
            let mut list = v_flex()
                .id("profile-base-list")
                .gap_1()
                .max_h(px(240.))
                .overflow_y_scroll();
            for (id, name) in rows {
                list = list.child(choice_row(
                    &format!("profile-base-{id}"),
                    name,
                    base == id,
                    view.clone(),
                    {
                        let id = id.clone();
                        move |this, cx| this.profile_new_set_base(id.clone(), cx)
                    },
                    cx,
                ));
            }
            body = body.child(list);
        }
        body.child(self.dialog_buttons(
            view,
            "profile-create",
            &self.strings.get("gui-profile-create"),
            "profile-create-cancel",
            &self.strings.get("gui-action-cancel"),
            Shell::profile_new_create,
            cx,
        ))
        .into_any_element()
    }

    /// Primary/Cancel button pair shared by every profile dialog. The
    /// primary runs the variant's confirm handler; Cancel only closes.
    #[allow(clippy::too_many_arguments)]
    fn dialog_buttons(
        &self,
        view: Entity<Self>,
        yes_id: &'static str,
        yes_label: &str,
        no_id: &'static str,
        no_label: &str,
        confirm: fn(&mut Shell, &mut Window, &mut Context<Shell>),
        cx: &App,
    ) -> AnyElement {
        let yes_view = view.clone();
        h_flex()
            .gap_2()
            .child(
                widgets::btn(yes_id, cx)
                    .primary()
                    .child(widgets::blabel(yes_label.to_string(), cx))
                    .on_click(move |_, window, cx| {
                        yes_view.update(cx, |this, cx| confirm(this, window, cx));
                    }),
            )
            .child(
                widgets::btn(no_id, cx)
                    .ghost()
                    .child(widgets::blabel(no_label.to_string(), cx))
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| this.close_profile_dialog(window, cx));
                    }),
            )
            .into_any_element()
    }
}

fn choice_row(
    id: &str,
    label: &str,
    on: bool,
    view: Entity<Shell>,
    pick: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
    cx: &App,
) -> AnyElement {
    let glyph = if on { "●" } else { "○" };
    h_flex()
        .id(SharedString::from(id.to_string()))
        .gap_2()
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| pick(this, cx));
        })
        .child(widgets::mono(glyph, cx))
        .child(widgets::blabel(label.to_string(), cx))
        .into_any_element()
}
