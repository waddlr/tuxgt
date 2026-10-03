use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{types, TypeStyled as _};

use super::notice::NoticeKind;
use super::*;

impl Shell {
    pub(crate) fn emit_activity(&mut self, kind: NoticeKind, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.notices.emit_activity(kind, text);
        self.schedule_autohide(kind, cx);
    }

    /// E101: one Live card — overlay + sidecar, uncapped, X snoozes.
    /// E102 drives it from the install paths. Live never expires, so this
    /// schedules no autohide; `finish_live` converts it to an Activity toast.
    pub(crate) fn emit_live(
        &mut self,
        kind: NoticeKind,
        text: String,
        cx: &mut Context<Self>,
    ) -> u64 {
        let id = self.notices.emit_live(kind, text);
        cx.notify();
        id
    }

    /// E102: store one Live card's percent and repaint only when it changed.
    pub(crate) fn set_live_progress(
        &mut self,
        id: u64,
        percent: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        if self.notices.set_live_progress(id, percent) {
            cx.notify();
        }
    }

    /// E102: finish a Live card as an Activity toast (success or error).
    /// Kind drives autohide: `Ok` leaves the overlay after 5s, `Err` waits for X.
    pub(crate) fn finish_live(
        &mut self,
        id: u64,
        kind: NoticeKind,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.notices.finish_live(id, kind, text);
        self.schedule_autohide(kind, cx);
        cx.notify();
    }

    /// E101: one Attention card — sidecar only, deduped by `key`, X is a
    /// session-dismiss. E104 fills the lane from the catalog poll.
    pub(crate) fn emit_attention(
        &mut self,
        key: &str,
        kind: NoticeKind,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.notices.emit_attention(key, kind, text);
        cx.notify();
    }

    /// info/ok leave the overlay after `notice::AUTOHIDE`; warn/err have no
    /// timer (they wait for X). One one-shot sleep per emit, never a poll.
    pub(crate) fn schedule_autohide(&mut self, kind: NoticeKind, cx: &mut Context<Self>) {
        if !matches!(kind, NoticeKind::Info | NoticeKind::Ok) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(notice::AUTOHIDE).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// E104 catalog poll cadence: every 2h while the GUI is up. The
    /// 2h timer never starts a run — it only clears due-ness, so an
    /// unfocused/occluded/minimized window (or one behind a fullscreen
    /// game, which paints nothing active) waits for the next active
    /// paint. Focus return is the catch-up. Core overview timer rule.
    pub(crate) const UPDATE_POLL_SECS: u64 = 2 * 60 * 60;

    /// E104: run the catalog poll now unless one is in flight or not due
    /// (2h since `update_last_poll`; None = never ran = due). Only the
    /// startup kick and active paints call this; the re-arm timer only
    /// clears due-ness and lets the next active paint start the run.
    pub(crate) fn toggle_sidecar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidecar_open = !self.sidecar_open;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn close_sidecar(&mut self, cx: &mut Context<Self>) {
        self.sidecar_open = false;
        cx.notify();
    }

    /// E101: X on a card. Live snoozes that surface — never cancels (E102
    /// keeps downloading); Activity overlay X hides the toast, sidecar X
    /// deletes the row; Attention X session-dismisses.
    pub(crate) fn dismiss_notice(&mut self, id: u64, sidecar: bool, cx: &mut Context<Self>) {
        if sidecar {
            self.notices.dismiss_sidecar(id);
        } else {
            self.notices.dismiss_overlay(id);
        }
        cx.notify();
    }

    /// E70: warning toast when the E67 session report (`host_install`) has a
    /// required intended path not `ok`. Reads the current field — the E69
    /// Settings card, when landed, rewrites it after install/uninstall, so
    /// this never re-runs verify and never duplicates E69. Returns `None`
    /// when healthy. Callers gate on the op (handle-on / Apply); this helper
    /// only classifies. Warning only: never blocks the op.
    pub(crate) fn notice_layer(&self, view: Entity<Self>, cx: &App) -> Option<AnyElement> {
        let now = Instant::now();
        if self.sidecar_open {
            return Some(self.notice_sidecar(view, now, cx).into_any_element());
        }
        if !self.notices.has_overlay(now) {
            return None;
        }
        Some(
            v_flex()
                .id("notice-overlay")
                .absolute()
                .top(px(50.))
                .right_3()
                .w(px(360.))
                .gap_2()
                .children(
                    self.notices
                        .overlay(now)
                        .into_iter()
                        .map(|n| self.notice_card(view.clone(), n, false, cx)),
                )
                .into_any_element(),
        )
    }

    /// E101: the sidecar — no pane, cards + a ghost Clear all. A
    /// transparent click-catcher covers the shell behind the panel, so a
    /// click outside (or Escape) closes it. The catcher starts below the
    /// 34px titlebar so the bell stays clickable while open (the bell
    /// toggles, Escape closes).
    pub(crate) fn notice_sidecar(
        &self,
        view: Entity<Self>,
        now: Instant,
        cx: &App,
    ) -> impl IntoElement {
        let t = types(cx);
        let th = cx.theme();
        let rows = self.notices.sidecar(now);
        let empty = rows.is_empty();
        v_flex()
            .id("notice-sidecar")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("notice-catcher")
                    .absolute()
                    .inset_0()
                    .top(px(34.))
                    .on_mouse_down(MouseButton::Left, {
                        let view = view.clone();
                        move |_, _, cx| {
                            cx.stop_propagation();
                            view.update(cx, |this, cx| this.close_sidecar(cx));
                        }
                    }),
            )
            .child(
                v_flex()
                    .id("notice-panel")
                    .absolute()
                    .top(px(50.))
                    .right_3()
                    .w(px(360.))
                    .max_h(px(520.))
                    .overflow_hidden()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(th.border)
                    .bg(th.sidebar)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .id("notice-head")
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(th.border)
                            .child(
                                div()
                                    .tx(t.headline_sm)
                                    .child(self.strings.get("gui-notice-title")),
                            )
                            .child(
                                Button::new("notice-clear")
                                    .ghost()
                                    .small()
                                    .child(widgets::blabel(
                                        self.strings.get("gui-notice-clear-all"),
                                        cx,
                                    ))
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            view.update(cx, |this, cx| {
                                                this.notices.clear_activity();
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("notice-list")
                            .w_full()
                            .max_h(px(440.))
                            .overflow_y_scroll()
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .gap_2()
                            .p_2()
                            .when(empty, |this| {
                                this.child(widgets::muted(self.strings.get("gui-notice-empty"), cx))
                            })
                            .children(
                                rows.into_iter()
                                    .map(|n| self.notice_card(view.clone(), n, true, cx)),
                            ),
                    ),
            )
    }
}
