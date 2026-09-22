use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::*;

use super::super::theme::types;
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn archive_password_box(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        let title = self.strings.get("gui-archive-password-title");
        let note = self.strings.get("gui-archive-password-note");
        let input = self.archive_password_input.clone();
        let submit_view = view.clone();
        let cancel_view = view;
        let input_control = Styled::h(
            Input::new(&input)
                .xsmall()
                .text_size(types(cx).body_md.size),
            types(cx).control_h,
        );
        let body = v_flex()
            .gap_2()
            .child(widgets::muted(note, cx))
            .child(input_control);
        let footer = h_flex()
            .w_full()
            .gap_2()
            .justify_end()
            .child(
                widgets::btn("archive-password-submit", cx)
                    .primary()
                    .child(widgets::blabel(
                        self.strings.get("gui-action-archive-password-submit"),
                        cx,
                    ))
                    .on_click(move |_, window, cx| {
                        let password = submit_view
                            .read(cx)
                            .archive_password_input
                            .read(cx)
                            .value()
                            .to_string();
                        let pending = submit_view.update(cx, |this, cx| {
                            let pending = this.pending_archive_password.take();
                            this.archive_password_input.update(cx, |input, cx| {
                                input.set_value(String::new(), window, cx);
                            });
                            pending
                        });
                        if let Some(pending) = pending {
                            super::pick_package_with_password(
                                submit_view.clone(),
                                pending.mod_type,
                                pending.files,
                                pending.directories,
                                pending.prompt_key,
                                Some(password),
                                Some(pending.path),
                                cx,
                            );
                        }
                    }),
            )
            .child(
                widgets::btn("archive-password-cancel", cx)
                    .secondary()
                    .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                    .on_click(move |_, window, cx| {
                        cancel_view.update(cx, |this, cx| {
                            this.pending_archive_password = None;
                            this.archive_password_input.update(cx, |input, cx| {
                                input.set_value(String::new(), window, cx);
                            });
                            cx.notify();
                        });
                    }),
            );
        widgets::section_card("archive-password-panel", cx)
            .child(widgets::section_title(title, cx))
            .child(body)
            .child(footer)
            .into_any_element()
    }
}
