use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::{ConfigNavPending, Shell, widgets};
use super::{config_abs_path, ConfigLevel};

impl Shell {
    /// `gui.mod-config-edit` dirty check: buffer vs the mounted file on
    /// disk (a missing staged file mounts empty, so any typing is dirty;
    /// externally-only files hold no buffer, so never dirty).
    fn config_dirty(&self, cx: &App) -> bool {
        let Some(edit) = self.config_edit.as_ref() else {
            return false;
        };
        if edit.external_only {
            return false;
        }
        let buf = self.config_input.read(cx).value().to_string();
        let disk = match config_abs_path(&edit.level, &edit.rel) {
            Ok(p) => std::fs::read_to_string(&p).unwrap_or_default(),
            // Unresolvable path: treat typing as unsaved, never silently drop.
            Err(_) => return true,
        };
        buf != disk
    }

    /// Navigation guard for the open config editor. Same-place moves keep
    /// the editor; clean buffers close silently; dirty buffers park the
    /// original intent and report false (the discard modal paints instead).
    /// Never overwrites an already-parked intent: a blocked nav leaves
    /// history untouched, so the modal owns the next move.
    pub(crate) fn try_leave_config(
        &mut self,
        pending: ConfigNavPending,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.config_edit.is_none() {
            return true;
        }
        if self.config_nav_pending.is_some() {
            return false;
        }
        if let ConfigNavPending::Place(place) = &pending {
            if *place == self.current_place() {
                return true;
            }
        }
        if self.config_dirty(cx) {
            self.config_nav_pending = Some(pending);
            cx.notify();
            return false;
        }
        self.config_edit = None;
        true
    }

    /// Modal Discard: drop the buffer with the usual discarded note, then
    /// replay the parked intent so history matches an unblocked nav.
    pub(crate) fn confirm_config_navigate(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.config_nav_pending.take() else {
            return;
        };
        if let Some(open) = self.config_edit.take() {
            let mut args = FluentArgs::new();
            args.set("file", open.rel);
            self.status = self
                .strings
                .get_args("gui-note-config-discarded", Some(&args));
        }
        match pending {
            ConfigNavPending::Place(place) => {
                let _ = self.push_if_new(place.clone());
                self.apply_place(place, cx);
            }
            ConfigNavPending::Back => self.go_back(cx),
            ConfigNavPending::Forward => self.go_forward(cx),
            ConfigNavPending::ModsTab(tab) => {
                // Replay the exact body from settings/mods.rs inner-tab on_click
                // (around lines 35-43): direct assign + persist + side effects.
                // This is NOT a Place change, so no push_if_new/apply_place.
                self.mods_tab = tab;
                self.persist_mods_tab();
                self.add_form = None;
                self.scroll_page_top();
                cx.notify();
            }
        }
    }

    /// Modal Cancel / click-outside: stay on the editor, drop the park.
    /// Disk unchanged.
    pub(crate) fn cancel_config_navigate(&mut self, cx: &mut Context<Self>) {
        self.config_nav_pending = None;
        cx.notify();
    }

    /// Unsaved-edits discard confirm as a shell-root modal (client-stop
    /// pattern). Mounted in `render.rs` next to the notice layer, so it
    /// paints over the editor page the blocked nav never left.
    /// Click-outside stays; Discard runs the parked place.
    pub(crate) fn config_discard_layer(
        &self,
        view: Entity<Self>,
        cx: &App,
    ) -> Option<AnyElement> {
        let edit = self.config_edit.as_ref()?;
        self.config_nav_pending.as_ref()?;
        let level = match &edit.level {
            ConfigLevel::Global { id } => format!("payload {id}"),
            ConfigLevel::Staged { game, instance } => format!("{game}/{instance}"),
        };
        let mut args = FluentArgs::new();
        args.set("file", edit.rel.clone());
        let confirm_view = view.clone();
        let stay_view = view.clone();
        let backdrop_view = view.clone();
        Some(
            v_flex()
                .id("config-discard-modal")
                .absolute()
                .inset_0()
                .bg(rgba(0x0e0e10d9))
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    backdrop_view.update(cx, |this, cx| this.cancel_config_navigate(cx));
                })
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .id("config-discard-panel")
                        .w(px(420.))
                        .gap_2()
                        .p_4()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().sidebar)
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(widgets::muted(format!("{level} · {}", edit.rel), cx))
                        .child(widgets::muted(
                            self.strings
                                .get_args("gui-note-config-unsaved", Some(&args)),
                            cx,
                        ))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    widgets::btn("config-discard-yes", cx)
                                        .primary()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-action-discard"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            confirm_view.update(cx, |this, cx| {
                                                this.confirm_config_navigate(cx);
                                            });
                                        }),
                                )
                                .child(
                                    widgets::btn("config-discard-no", cx)
                                        .ghost()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-action-cancel"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            stay_view.update(cx, |this, cx| {
                                                this.cancel_config_navigate(cx);
                                            });
                                        }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
