use std::time::Instant;

use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::{h_flex, ActiveTheme, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::theme::{types, TypeStyled as _};

use super::notice_store::BellState;
use super::titlebar_buttons::{chrome_btn, should_start_move, BarDrag, ChromeColors};
use super::*;

impl Shell {
    pub(crate) fn titlebar(
        &self,
        view: Entity<Self>,
        frame_r: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = types(cx);
        let title_bar = cx.theme().title_bar;
        let border = cx.theme().border;
        let warning = cx.theme().warning;
        // T25: one theme read per paint for all 7 chrome buttons.
        let chrome = ChromeColors {
            fg: cx.theme().foreground,
            hover: cx.theme().secondary_hover,
            hover_fg: cx.theme().secondary_foreground,
            active: cx.theme().secondary_active,
        };
        let icon = icon_file();
        // Client-only buttons: under server-side decorations the manager
        // paints its own set (kit `WindowControls` gates the same way).
        let client = matches!(window.window_decorations(), Decorations::Client { .. });
        let supported = window.window_controls();
        let maximized = window.is_maximized();
        let drag = window.use_state(cx, |_, _| BarDrag { should_move: false });
        div()
            .id("title-bar")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .h(px(34.))
            .w_full()
            .flex_shrink_0()
            .bg(title_bar)
            .border_b_1()
            .border_color(border)
            .rounded_tl(frame_r)
            .rounded_tr(frame_r)
            .on_double_click(|_, window, _| window.zoom_window())
            .on_mouse_down_out(window.listener_for(&drag, |state, _, _, _| {
                state.should_move = false;
            }))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&drag, |state, _, _, _| {
                    state.should_move = true;
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&drag, |state, _, _, _| {
                    state.should_move = false;
                }),
            )
            .on_mouse_move(window.listener_for(
                &drag,
                |state, event: &MouseMoveEvent, window, _| {
                    if should_start_move(state.should_move, event.pressed_button) {
                        window.start_window_move();
                    }
                    state.should_move = false;
                },
            ))
            .when(client, |this| {
                this.child(
                    div()
                        .top_0()
                        .left_0()
                        .absolute()
                        .size_full()
                        .h_full()
                        .on_mouse_down(MouseButton::Right, move |ev, window, _| {
                            window.show_window_menu(ev.position)
                        }),
                )
            })
            .child(
                h_flex()
                    .id("tb")
                    .w_full()
                    .items_center()
                    .gap_0()
                    .child(
                        h_flex()
                            .id("tb-brand")
                            .items_center()
                            .gap_1()
                            // `#tb` is gap_0: the brand→toggle 16px lives here.
                            .pr_4()
                            .flex_shrink_0()
                            .child(img(icon).w(px(18.)).h(px(18.)).rounded(px(3.)))
                            .child(div().tx(t.headline_md).child(self.strings.get("gui-title"))),
                    )
                    .child(chrome_btn(
                        "tb-sidebar",
                        "wb-side",
                        if self.prefs.sidebar_collapsed {
                            IconName::PanelLeftOpen
                        } else {
                            IconName::PanelLeftClose
                        },
                        chrome,
                        self.strings.get(if self.prefs.sidebar_collapsed {
                            "gui-tip-sidebar-expand"
                        } else {
                            "gui-tip-sidebar-collapse"
                        }),
                        false,
                        false,
                        {
                            let view = view.clone();
                            move |_, cx| {
                                view.update(cx, |this, cx| {
                                    this.prefs.sidebar_collapsed = !this.prefs.sidebar_collapsed;
                                    this.prefs.save();
                                    cx.notify();
                                });
                            }
                        },
                    ))
                    .child(
                        h_flex()
                            .id("tb-hist")
                            .items_center()
                            .gap_0()
                            .flex_shrink_0()
                            .child(chrome_btn(
                                "tb-back",
                                "wb-back",
                                IconName::ArrowLeft,
                                ChromeColors {
                                    fg: if self.hist_back.is_empty() {
                                        cx.theme().muted_foreground
                                    } else {
                                        chrome.fg
                                    },
                                    ..chrome
                                },
                                self.strings.get("gui-tip-back"),
                                self.hist_back.is_empty(),
                                false,
                                {
                                    let view = view.clone();
                                    move |_, cx| {
                                        view.update(cx, |this, cx| this.go_back(cx));
                                    }
                                },
                            ))
                            .child(chrome_btn(
                                "tb-fwd",
                                "wb-fwd",
                                IconName::ArrowRight,
                                ChromeColors {
                                    fg: if self.hist_fwd.is_empty() {
                                        cx.theme().muted_foreground
                                    } else {
                                        chrome.fg
                                    },
                                    ..chrome
                                },
                                self.strings.get("gui-tip-forward"),
                                self.hist_fwd.is_empty(),
                                false,
                                {
                                    let view = view.clone();
                                    move |_, cx| {
                                        view.update(cx, |this, cx| this.go_forward(cx));
                                    }
                                },
                            )),
                    )
                    .child(
                        div()
                            .id("tb-title")
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .tx(t.headline_md)
                            .truncate()
                            .child(self.context_title()),
                    )
                    .child({
                        let icon_fg = match self.notices.bell_state(Instant::now()) {
                            BellState::Live => cx.theme().primary,
                            BellState::Attention => warning,
                            BellState::Idle => chrome.fg,
                        };
                        chrome_btn(
                            "tb-bell",
                            "wb-bell",
                            IconName::Bell,
                            ChromeColors {
                                fg: icon_fg,
                                ..chrome
                            },
                            self.strings.get("gui-tip-notifications"),
                            false,
                            self.sidecar_open,
                            {
                                let view = view.clone();
                                move |window, cx| {
                                    view.update(cx, |this, cx| this.toggle_sidecar(window, cx));
                                }
                            },
                        )
                    })
                    .child(
                        div()
                            .id("tb-rule")
                            .w(px(1.))
                            .h(px(12.))
                            .mx_1()
                            .flex_shrink_0()
                            .bg(border),
                    )
                    .when(client && supported.minimize, |this| {
                        this.child(chrome_btn(
                            "tb-min",
                            "wb-min",
                            IconName::WindowMinimize,
                            chrome,
                            "",
                            false,
                            false,
                            |window, _| window.minimize_window(),
                        ))
                    })
                    .when(client && supported.maximize, |this| {
                        let (id, icon) = if maximized {
                            ("tb-restore", IconName::WindowRestore)
                        } else {
                            ("tb-max", IconName::WindowMaximize)
                        };
                        this.child(chrome_btn(
                            id,
                            "wb-max",
                            icon,
                            chrome,
                            "",
                            false,
                            false,
                            |window, _| window.zoom_window(),
                        ))
                    })
                    .when(client, |this| {
                        this.child(chrome_btn(
                            "tb-close",
                            "wb-close",
                            IconName::WindowClose,
                            chrome,
                            "",
                            false,
                            false,
                            |window, _| window.remove_window(),
                        ))
                    }),
            )
    }
}
