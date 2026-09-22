use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::FluentArgs;

use super::theme::{types, TypeStyled as _};

use super::*;

mod list;

impl Shell {
    pub(crate) fn context_title(&self) -> String {
        match self.nav {
            Nav::Library => self.strings.get("gui-title-library"),
            Nav::Settings => match self.settings_tab {
                SettingsTab::General => self.strings.get("gui-nav-settings"),
                SettingsTab::CorePlugins => {
                    let mut args = FluentArgs::new();
                    args.set("tab", self.strings.get("gui-tab-settings-core-plugins"));
                    self.strings.get_args("gui-title-settings-tab", Some(&args))
                }
                SettingsTab::GameEnv => {
                    let mut args = FluentArgs::new();
                    args.set("tab", self.strings.get("gui-tab-settings-env"));
                    self.strings.get_args("gui-title-settings-tab", Some(&args))
                }
                SettingsTab::Mods => {
                    let mut args = FluentArgs::new();
                    args.set("tab", self.strings.get("gui-tab-settings-mods"));
                    self.strings.get_args("gui-title-settings-tab", Some(&args))
                }
            },
            Nav::Game => {
                let Some(g) = self.selected_game() else {
                    return self.strings.get("gui-title-library");
                };
                let manager =
                    widgets::id_label(widgets::ValKind::Manager, &g.manager, &self.strings);
                let name = g.display_name().to_string();
                match self.game_tab {
                    GameTab::General => {
                        let mut args = FluentArgs::new();
                        args.set("manager", manager);
                        args.set("name", name);
                        self.strings.get_args("gui-title-game", Some(&args))
                    }
                    tab => {
                        let tab_s = match tab {
                            GameTab::Mods => self.strings.get("gui-tab-mods"),
                            GameTab::Env => self.strings.get("gui-title-tab-env"),
                            GameTab::General => unreachable!(),
                        };
                        let mut args = FluentArgs::new();
                        args.set("manager", manager);
                        args.set("name", name);
                        args.set("tab", tab_s);
                        self.strings.get_args("gui-title-game-tab", Some(&args))
                    }
                }
            }
        }
    }

    /// E107: 240px column ↔ 48px icon rail. `sidebar_collapsed` in `ui.toml`
    /// persists the titlebar's icon-only toggle. Widths do not follow
    /// `font_scale` (`overview.md` Sidebar).
    pub(crate) fn sidebar(
        &self,
        view: Entity<Self>,
        frame_r: Pixels,
        cx: &App,
    ) -> impl IntoElement {
        let th = cx.theme();
        let t = types(cx);
        let collapsed = self.prefs.sidebar_collapsed;
        v_flex()
            .id("sidebar")
            .w(px(if collapsed { 48. } else { 240. }))
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .overflow_hidden()
            .rounded_bl(frame_r)
            .bg(th.sidebar)
            .border_r_1()
            .border_color(th.border)
            .child(
                v_flex()
                    .id("sb-nav")
                    .p_1()
                    .gap_1()
                    .border_b_1()
                    .border_color(th.border)
                    .child(
                        h_flex()
                            .id("sb-library")
                            .w_full()
                            .gap_2()
                            .items_center()
                            .when(collapsed, |t| t.justify_center().gap_0())
                            .px_1()
                            .py_1()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .when(self.nav == Nav::Library, |t| {
                                t.bg(th.sidebar_accent)
                                    .border_l_2()
                                    .border_color(cx.theme().primary)
                            })
                            .when(self.nav != Nav::Library, |t| {
                                t.hover(|s| s.bg(th.group_box))
                            })
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.jump_library(cx));
                                }
                            })
                            .child(widgets::bicon(IconName::LayoutDashboard))
                            .when(!collapsed, |this| {
                                this.child(
                                    div()
                                        .tx(t.headline_md)
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(if self.nav == Nav::Library {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .child(self.strings.get("gui-nav-library")),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .id("sb-settings")
                            .w_full()
                            .gap_2()
                            .items_center()
                            .when(collapsed, |t| t.justify_center().gap_0())
                            .px_1()
                            .py_1()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .when(self.nav == Nav::Settings, |t| {
                                t.bg(th.sidebar_accent)
                                    .border_l_2()
                                    .border_color(cx.theme().primary)
                            })
                            .when(self.nav != Nav::Settings, |t| {
                                t.hover(|s| s.bg(th.group_box))
                            })
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.enter_settings(cx));
                                }
                            })
                            .child(widgets::bicon(IconName::Settings))
                            .when(!collapsed, |this| {
                                this.child(
                                    div()
                                        .tx(t.headline_md)
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(if self.nav == Nav::Settings {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .child(self.strings.get("gui-nav-settings")),
                                )
                            }),
                    ),
            )
            .when(!collapsed, |this| {
                this.child(
                    v_flex()
                        .id("sb-search")
                        .p_2()
                        .gap_1()
                        .border_b_1()
                        .border_color(th.border)
                        .child(Styled::h(
                            Input::new(&self.search)
                                .xsmall()
                                .text_size(types(cx).body_md.size)
                                .cleanable(true),
                            types(cx).control_h,
                        )),
                )
            })
            .child(
                v_flex()
                    .id("sb-list")
                    .flex_1()
                    .min_h_0()
                    .p_1()
                    .child(self.sidebar_list(view.clone(), cx)),
            )
    }
}
