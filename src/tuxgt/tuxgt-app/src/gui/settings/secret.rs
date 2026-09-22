use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::FluentArgs;

use super::super::theme::types;
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn secret_key_editor(
        &self,
        source: &'static str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let set = self.secret_states.get(source).copied().unwrap_or(false);
        let editing = self.secret_editing;
        let show_input = !set || editing;
        let save_view = view.clone();
        let cancel_view = view.clone();
        let clear_view = view.clone();
        let edit_view = view.clone();
        let test_view = view;
        v_flex()
            .gap_1()
            .when(show_input, |this| {
                this.child(Styled::h(
                    Input::new(&self.secret_input)
                        .xsmall()
                        .text_size(types(cx).body_md.size)
                        .cleanable(true),
                    types(cx).control_h,
                ))
            })
            .child(
                h_flex()
                    .gap_1()
                    .when(show_input, |this| {
                        this.child(
                            widgets::btn("secret-save", cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-save"), cx))
                                .on_click(move |_, window, cx| {
                                    let value = save_view
                                        .read(cx)
                                        .secret_input
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    save_view.update(cx, |this, cx| {
                                        this.save_secret_ui(source, &value, window, cx);
                                    });
                                }),
                        )
                    })
                    .when(!editing, |this| {
                        this.child(
                            widgets::btn("secret-test", cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-test"), cx))
                                .on_click(move |_, _, cx| {
                                    test_view.update(cx, |this, cx| {
                                        this.test_secret_ui(source, cx);
                                    });
                                }),
                        )
                    })
                    .when(set && !editing, |this| {
                        this.child(
                            widgets::btn("secret-clear", cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-clear"), cx))
                                .on_click(move |_, _, cx| {
                                    clear_view.update(cx, |this, cx| {
                                        this.clear_secret_ui(source, cx);
                                    });
                                }),
                        )
                        .child(
                            widgets::btn("secret-edit", cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-edit"), cx))
                                .on_click(move |_, window, cx| {
                                    edit_view.update(cx, |this, cx| {
                                        this.secret_editing = true;
                                        cx.notify();
                                    });
                                    let input = edit_view.read(cx).secret_input.clone();
                                    input.update(cx, |inp, cx| {
                                        inp.set_value(String::new(), window, cx);
                                    });
                                }),
                        )
                    })
                    .when(editing, |this| {
                        this.child(
                            widgets::btn("secret-cancel", cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                                .on_click(move |_, window, cx| {
                                    cancel_view.update(cx, |this, cx| {
                                        this.secret_editing = false;
                                        cx.notify();
                                    });
                                    let input = cancel_view.read(cx).secret_input.clone();
                                    input.update(cx, |inp, cx| {
                                        inp.set_value(String::new(), window, cx);
                                    });
                                }),
                        )
                    }),
            )
    }

    /// E44: write the keyring key off the UI thread, then refresh the pills.
    /// Empty input is an error status, never a write.
    pub(crate) fn save_secret_ui(
        &mut self,
        source: &'static str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if value.trim().is_empty() {
            self.status = self.strings.get("gui-status-secret-empty");
            cx.notify();
            return;
        }
        tracing::debug!(action = "save-secret", source);
        let secret = value.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { tuxgt_core::secret_manager_set(source, &secret) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => {
                        let mut args = FluentArgs::new();
                        args.set("source", source);
                        this.status = this
                            .strings
                            .get_args("gui-status-secret-saved", Some(&args));
                        this.secret_editing = false;
                        let input = this.secret_input.clone();
                        input.update(cx, |inp, cx| {
                            inp.set_value(String::new(), window, cx);
                        });
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-secrets", Some(&args));
                    }
                }
                this.reload_secrets(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// E44: remove the keyring entry, then refresh the pills.
    pub(crate) fn clear_secret_ui(&mut self, source: &'static str, cx: &mut Context<Self>) {
        tracing::debug!(action = "clear-secret", source);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { tuxgt_core::secret_manager_clear(source) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        let mut args = FluentArgs::new();
                        args.set("source", source);
                        this.status = this
                            .strings
                            .get_args("gui-status-secret-cleared", Some(&args));
                        this.secret_editing = false;
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-secrets", Some(&args));
                    }
                }
                this.reload_secrets(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// E44: probe the stored key. The core report (valid / rejected) lands in
    /// the status line; transport and status errors take the error prefix.
    /// `secret_manager_test` uses the blocking reqwest client, which must not
    /// run on the executor thread — hand it to a blocking pool thread.
    pub(crate) fn test_secret_ui(&mut self, source: &'static str, cx: &mut Context<Self>) {
        tracing::debug!(action = "test-secret", source);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async move {
                        tokio::task::spawn_blocking(move || tuxgt_core::secret_manager_test(source))
                            .await
                            .map_err(|e| tuxgt_core::Error::SecretManager(e.to_string()))?
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(report) => this.status = report,
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-secrets", Some(&args));
                    }
                }
                this.reload_secrets(cx);
                cx.notify();
            });
        })
        .detach();
    }
}
