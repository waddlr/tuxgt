use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Enter, Input};
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::*;

use super::super::theme::types;
use super::super::widgets;
use super::super::{PendingArchivePassword, Shell};

impl Shell {
    /// R12: abandoning a parked password releases what it held. The Install
    /// variant parked an install batch behind the card (`install_current`,
    /// the rest of `install_queue`, and the Hide veto in `install_hold`);
    /// Add/Provide held nothing. Every site that drops the stash without
    /// retrying goes through here so the veto and the work it protects
    /// cannot drift apart.
    pub(crate) fn clear_pending_archive_password(&mut self) {
        if let Some(PendingArchivePassword::Install { game, instance, .. }) =
            self.pending_archive_password.take()
        {
            self.clear_install_current(&game, &instance);
            self.install_queue.clear();
            self.install_hold = None;
        }
        self.pending_archive_password = None;
    }

    /// Shared Unlock path for the submit button and the input's Enter key.
    /// A second arrival finds no pending op and returns.
    fn submit_archive_password(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let password = view
            .read(cx)
            .archive_password_input
            .read(cx)
            .value()
            .to_string();
        // Add and Provide retry by reading Shell (hide state).
        // That read panics while this update still holds the
        // Shell lease, which is what Unlock did.
        let pending = view.update(cx, |this, cx| {
            let pending = this.pending_archive_password.take();
            this.archive_password_input.update(cx, |input, cx| {
                input.set_value(String::new(), window, cx);
            });
            pending
        });
        let Some(pending) = pending else {
            return;
        };
        match pending {
            PendingArchivePassword::Add {
                path,
                mod_type,
                files,
                directories,
                prompt_key,
            } => {
                super::pick_package_with_password(
                    view.clone(),
                    mod_type,
                    files,
                    directories,
                    prompt_key,
                    Some(password),
                    Some(path),
                    cx,
                );
            }
            PendingArchivePassword::Provide { id, path } => {
                // A retry supplies the path, so the picker
                // args below are never read.
                super::mods::prompt_provide_with_password(
                    view.clone(),
                    id,
                    true,
                    true,
                    String::new(),
                    Some(password),
                    Some(path),
                    cx,
                );
            }
            PendingArchivePassword::Install {
                game,
                instance,
                adapter,
                with_requires,
                yes,
                redownload,
                force,
                slot,
            } => {
                view.update(cx, |this, cx| {
                    if redownload {
                        this.update_mod_ui(
                            &game,
                            &instance,
                            adapter,
                            yes,
                            force,
                            Some(password),
                            slot,
                            cx,
                        );
                    } else {
                        this.install_mod_ui(
                            &game,
                            &instance,
                            with_requires,
                            yes,
                            Some(password),
                            slot,
                            cx,
                        );
                    }
                });
            }
        }
    }

    pub(crate) fn archive_password_box(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        let title = self.strings.get("gui-archive-password-title");
        let note = self.strings.get("gui-archive-password-note");
        let input = self.archive_password_input.clone();
        let submit_view = view.clone();
        let enter_view = view.clone();
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
                        Self::submit_archive_password(&submit_view, window, cx);
                    }),
            )
            .child(
                widgets::btn("archive-password-cancel", cx)
                    .secondary()
                    .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                    .on_click(move |_, window, cx| {
                        cancel_view.update(cx, |this, cx| {
                            this.clear_pending_archive_password();
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
            // The input propagates its Enter action, so this fires only when
            // the password field is focused — never from a focused button.
            .on_action(move |_: &Enter, window, cx| {
                Self::submit_archive_password(&enter_view, window, cx);
            })
            .into_any_element()
    }
}
