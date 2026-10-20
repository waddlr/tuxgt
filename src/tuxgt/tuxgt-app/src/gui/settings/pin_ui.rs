//! `Pin this version` on a catalog card and on an installed game card.
//! One write at a time. The click does not install, uninstall, or update.

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, FluentArgs};

use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn pin_version_button(
        &self,
        view: Entity<Self>,
        element_id: SharedString,
        mod_id: String,
        from_game: Option<String>,
        cx: &App,
    ) -> impl IntoElement {
        let id = mod_id;
        let game = from_game;
        widgets::btn(element_id, cx)
            .secondary()
            .child(widgets::blabel(
                self.strings.get("gui-action-pin-version"),
                cx,
            ))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let id = id.clone();
                let game = game.clone();
                view.update(cx, |this, cx| {
                    this.pin_version_ui(id, game, cx);
                });
            })
    }

    pub(crate) fn pin_copy_line(&self, names: &[String], cx: &App) -> AnyElement {
        let mut args = FluentArgs::new();
        args.set("names", names.join(", "));
        widgets::muted(self.strings.get_args("gui-pin-copy-line", Some(&args)), cx)
            .into_any_element()
    }

    /// Copy-card sentence. `unavailable` paints the missing-bytes line.
    pub(crate) fn pin_card_note(
        &self,
        version: Option<&str>,
        unavailable: bool,
        cx: &App,
    ) -> AnyElement {
        let Some(version) = version.filter(|_| !unavailable) else {
            return widgets::muted(self.strings.get("gui-pin-unavailable"), cx).into_any_element();
        };
        let mut args = FluentArgs::new();
        args.set("version", version.to_string());
        widgets::muted(self.strings.get_args("gui-pin-card-note", Some(&args)), cx)
            .into_any_element()
    }

    pub(crate) fn pin_version_ui(
        &mut self,
        id: String,
        from_game: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.pin_busy {
            return;
        }
        self.pin_busy = true;
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        tuxgt_core::pin_version(
                            &config_dir(),
                            &data_dir(),
                            &id,
                            from_game.as_deref(),
                        )
                        .await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.pin_busy = false;
                match result {
                    Ok(made) => {
                        let mut args = FluentArgs::new();
                        args.set("name", made.label.clone());
                        if made.already {
                            this.status =
                                this.strings.get_args("gui-status-pin-exists", Some(&args));
                        } else {
                            args.set("version", made.version);
                            this.status = this.strings.get_args("gui-status-pinned", Some(&args));
                        }
                        this.instances = super::load_instances().into_boxed_slice();
                        this.refresh_selected_mods();
                        if let Some(game) = this.selected.clone() {
                            this.refresh_mods(&game, cx);
                        }
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-pin", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
