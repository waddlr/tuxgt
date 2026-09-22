use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::FluentArgs;

use super::super::theme::{brand, types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

/// Open a link with the system handler — the same `xdg-open` path as
/// `widgets::open_btn` (Link), not a new crate (E57).
pub(crate) fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub(crate) fn protondb_url(appid: &str) -> String {
    format!("https://www.protondb.com/app/{appid}")
}

pub(crate) fn steam_store_url(appid: &str) -> String {
    format!("https://store.steampowered.com/app/{appid}")
}

impl Shell {
    pub(crate) fn game_header(
        &self,
        g: &tuxgt_core::GameRow,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let brand = brand(cx);
        let t = types(cx);
        // Hero thumb through the capped decode cache (never full-res): a
        // missing thumb kicks the background render and paints plain.
        let hero = widgets::art_image(&g.id, widgets::ArtKind::Hero);
        widgets::kick_art_decode(&g.id, widgets::ArtKind::Hero, view.clone(), cx);
        let tier = self.tiers.get(&g.id).cloned();
        let flag = self.awacy.get(&g.id).cloned();
        let appid = self.resolved_appid(g);
        let mods_n = self.enabled_n(&g.id);
        let knobs_n = self.knobs_n();
        let client = self.is_client_game(&g.id);
        let applied = self.applied.get(&g.id).copied().unwrap_or(false);
        let handled = self.handle.get(&g.id).copied().unwrap_or(false);
        let needs = self.launch_needs;
        let enable_play = client && !handled && !applied && needs.channel_needed();
        // Hooked = protonfixes/launch-option path active (handled/applied).
        let hooked = handled || applied;
        let bg = cx.theme().background;
        // True fade over the content zone: transparent at button half-height,
        // darkest-translucent at the title. Buttons carry their own fills.
        let fade = linear_gradient(
            180.,
            linear_color_stop(bg.opacity(0.), 0.35),
            linear_color_stop(bg.opacity(0.85), 1.),
        );
        v_flex()
            .id("game-head")
            .relative()
            .overflow_hidden()
            .min_h(px(180.))
            .bg(brand.wash)
            .when_some(hero, |this, art| {
                this.child(
                    img(ImageSource::Render(art))
                        .absolute()
                        .inset_0()
                        .w_full()
                        .h_full()
                        .object_fit(ObjectFit::Cover),
                )
            })
            .child(div().absolute().inset_0().bg(fade))
            .child(
                v_flex()
                    .relative()
                    .items_start()
                    .justify_end()
                    .flex_1()
                    .p_3()
                    .gap_1()
                    .child(
                        h_flex()
                            .id("hero-play")
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .when(enable_play, |this| {
                                this.child(
                                    widgets::page_cta(
                                        "enable-play",
                                        self.strings.get("gui-action-enable-play"),
                                        cx,
                                    )
                                    .primary()
                                    .flex_none()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| this.enable_and_play_ui(cx));
                                        }
                                    }),
                                )
                            })
                            .child({
                                let btn = widgets::page_cta(
                                    "play",
                                    self.strings.get("gui-action-play"),
                                    cx,
                                );
                                // Gray for vanilla, primary for any hooked play.
                                let btn = if hooked {
                                    btn.primary()
                                } else {
                                    btn.secondary()
                                };
                                btn.flex_none().on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if enable_play && !applied {
                                                this.status =
                                                    this.strings.get("gui-status-vanilla-play");
                                            }
                                            this.launch_play(None, None, cx);
                                        });
                                    }
                                })
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .when_some(tier.filter(|t| t != "none"), |this, tier| {
                                this.child(self.protondb_chip(&tier, appid.clone(), cx))
                            })
                            .when_some(flag, |this, f| {
                                this.child(widgets::pill(
                                    f.chip_label(&self.strings),
                                    cx.theme().danger,
                                    b.border,
                                    cx,
                                ))
                            })
                            .child(self.hero_status(mods_n, knobs_n, cx)),
                    )
                    .child(
                        div()
                            .tx(t.headline_lg)
                            // One step above the token (20 at Default), still
                            // following `font_scale`.
                            .text_size(t.headline_lg.size + px(2.))
                            .text_color(b.foreground)
                            .child(g.display_name().to_string()),
                    )
            )
    }

    pub(crate) fn knobs_n(&self) -> usize {
        self.knob_count
            + self.custom_count
            + self.wrappers.len()
            + self.detect.iter().filter(|s| s.override_.is_some()).count()
    }

    /// The mods/knobs pills above the title. Solid pill wells (readable on
    /// any art); a muted pill when unmodified.
    pub(crate) fn hero_status(&self, mods_n: usize, knobs_n: usize, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        if mods_n == 0 && knobs_n == 0 {
            return widgets::pill(
                self.strings.get("gui-meta-unmodified"),
                b.muted_foreground,
                b.border,
                cx,
            )
            .into_any_element();
        }
        let mut args = FluentArgs::new();
        args.set("count", mods_n.to_string());
        let mods = self.strings.get_args("gui-meta-mods", Some(&args));
        let mut args = FluentArgs::new();
        args.set("count", knobs_n.to_string());
        let knobs = self.strings.get_args("gui-meta-knobs", Some(&args));
        h_flex()
            .gap_2()
            .items_center()
            .child(widgets::pill(mods, b.primary, b.border, cx))
            .child(widgets::pill(knobs, b.primary, b.border, cx))
            .into_any_element()
    }
}
