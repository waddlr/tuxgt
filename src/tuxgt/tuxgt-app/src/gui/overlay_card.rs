use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::notice::{Lane, NoticeKind};
use super::theme::{types, TypeStyled as _};
use super::*;

impl Shell {
    /// E101: one card. X is kit `small` (24px) — never `xsmall` (20px) —
    /// ghost and hover-only on both surfaces.
    pub(crate) fn notice_card(
        &self,
        view: Entity<Self>,
        n: &notice::Notice,
        sidecar: bool,
        cx: &App,
    ) -> AnyElement {
        let t = types(cx);
        let th = cx.theme();
        let (icon, tint) = match n.kind {
            NoticeKind::Info => (IconName::Info, th.primary),
            NoticeKind::Ok => (IconName::CircleCheck, th.success),
            NoticeKind::Warn => (IconName::TriangleAlert, th.warning),
            NoticeKind::Err => (IconName::CircleX, th.danger),
        };
        let surface = if sidecar { "s" } else { "o" };
        let tip = if sidecar {
            self.strings.get("gui-notice-dismiss")
        } else {
            self.strings.get("gui-notice-snooze")
        };
        // E104: Attention cards navigate (catalog → Settings Mods, game →
        // that game's Mods tab). Other lanes are not clickable; the X
        // still dismisses.
        let nav_key = (n.lane == Lane::Attention).then(|| n.key.clone());
        let group = SharedString::from(format!("n-{}", n.id));
        div()
            .id(SharedString::from(format!("notice-{surface}-{}", n.id)))
            .group(group.clone())
            .relative()
            .w_full()
            .rounded(px(4.))
            .border_1()
            .border_color(th.border)
            .bg(th.group_box)
            .px_3()
            .py_2()
            .pr_6()
            .occlude()
            .when_some(nav_key, |this, key| {
                this.cursor_pointer().on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.follow_attention(&key, cx));
                    }
                })
            })
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .child(Icon::new(icon).small().text_color(tint))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .tx(t.body_md)
                            .child(n.text.clone())
                            // The `app:` card applies in place instead of
                            // navigating; the body click still goes to
                            // Settings General.
                            .when(
                                n.lane == Lane::Attention && n.key.starts_with("app:"),
                                |this| {
                                    this.child(
                                        widgets::btn(
                                            SharedString::from(format!(
                                                "notice-app-update-{}",
                                                n.id
                                            )),
                                            cx,
                                        )
                                        .primary()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-mod-action-update"),
                                            cx,
                                        ))
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                cx.stop_propagation();
                                                view.update(cx, |this, cx| {
                                                    this.apply_app_update_ui(cx)
                                                });
                                            }
                                        }),
                                    )
                                },
                            )
                            .when(n.lane == Lane::Live, |this| {
                                this.child(
                                    Progress::new(SharedString::from(format!(
                                        "notice-bar-{}",
                                        n.id
                                    )))
                                    .small()
                                    .w_full()
                                    .when_some(n.progress, |bar, pct| bar.value(pct))
                                    .when(n.progress.is_none(), |bar| bar.loading(true)),
                                )
                            })
                            .when(
                                n.lane == Lane::Live
                                    && self.app_update_live.as_ref().is_some_and(|l| l.id == n.id)
                                    && self.app_update_can_cancel(),
                                |this| {
                                    this.child(
                                        widgets::btn(
                                            SharedString::from(format!(
                                                "notice-app-cancel-{}",
                                                n.id
                                            )),
                                            cx,
                                        )
                                        .secondary()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-action-cancel"),
                                            cx,
                                        ))
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                cx.stop_propagation();
                                                view.update(cx, |this, cx| {
                                                    this.cancel_app_update_ui(cx)
                                                });
                                            }
                                        }),
                                    )
                                },
                            ),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .invisible()
                    .group_hover(group, |this| this.visible())
                    .child(
                        Button::new(SharedString::from(format!("notice-x-{surface}-{}", n.id)))
                            .ghost()
                            .small()
                            .tooltip(tip)
                            .child(Icon::new(IconName::Close).small())
                            .on_click({
                                let view = view.clone();
                                let id = n.id;
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    view.update(cx, |this, cx| {
                                        this.dismiss_notice(id, sidecar, cx)
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }
}
