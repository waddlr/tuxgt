use super::*;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, GameRow};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn game_card(
        &self,
        g: &GameRow,
        card_w: f32,
        card_h: f32,
        decode: bool,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let t = types(cx);
        let id = g.id.clone();
        let name = g.display_name().to_string();
        let art = if decode {
            let art = widgets::art_image(&g.id, widgets::ArtKind::Grid);
            widgets::kick_art_decode(&g.id, widgets::ArtKind::Grid, view.clone(), cx);
            art
        } else {
            None
        };
        let tier = self.tiers.get(&g.id).cloned();
        let flag = self.awacy.get(&g.id).cloned();
        let hovered = self.hover_card.as_deref() == Some(g.id.as_str());
        let mods_n = self.enabled_n(&g.id);
        let hover_id = g.id.clone();
        let hover_view = view.clone();
        let play_id = g.id.clone();
        let play_view = view.clone();
        let game_id = g.id.clone();
        let game_view = view.clone();
        let remove_id = g.id.clone();
        let remove_view = view.clone();
        let is_manual = g.manager == "manual";
        widgets::hairline_card(SharedString::from(g.id.clone()), cx)
            .w(px(card_w))
            .h(px(card_h))
            .overflow_hidden()
            .cursor_pointer()
            .hover(|s| s.border_color(cx.theme().primary))
            .on_hover(move |is_hovered, _, cx| {
                let hover_id = hover_id.clone();
                hover_view.update(cx, |this, cx| {
                    if *is_hovered {
                        this.hover_card = Some(hover_id);
                    } else if this.hover_card.as_deref() == Some(hover_id.as_str()) {
                        this.hover_card = None;
                    }
                    cx.notify();
                });
            })
            .on_click(move |_, _, cx| {
                let id = id.clone();
                view.update(cx, |this, cx| this.select_game(id, cx));
            })
            .child(
                div()
                    .relative()
                    .w(px(card_w))
                    .h(px(card_h))
                    .bg(b.tab_bar_segmented)
                    .overflow_hidden()
                    .child(
                        art.map(|img_data| {
                            img(ImageSource::Render(img_data))
                                .w(px(card_w))
                                .h(px(card_h))
                                .object_fit(ObjectFit::Cover)
                                .into_any_element()
                        })
                        .unwrap_or_else(|| {
                            h_flex()
                                .w(px(card_w))
                                .h(px(card_h))
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .tx(t.headline_lg)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(g.initials()),
                                )
                                .into_any_element()
                        }),
                    )
                    .when(hovered, |this| {
                        let store_l = game_store_label(g, &self.games, &self.strings);
                        let ink: Hsla = rgb(0xf4f6f8).into();
                        this.child(
                            div()
                                .absolute()
                                .inset_0()
                                .bg(rgba(0x0e0e10d9))
                                .child(
                                    div()
                                        .absolute()
                                        .top_1()
                                        .left_1()
                                        .right_1()
                                        .tx(t.headline_md)
                                        .text_color(ink)
                                        .min_w_0()
                                        .truncate()
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap_2()
                                        .child(
                                            widgets::page_cta(
                                                format!("play-{play_id}"),
                                                self.strings.get("gui-action-play"),
                                                cx,
                                            )
                                            .primary()
                                            .on_click(
                                                move |_, _, cx| {
                                                    let play_id = play_id.clone();
                                                    play_view.update(cx, |this, cx| {
                                                        this.play_game(play_id, cx);
                                                    });
                                                },
                                            ),
                                        )
                                        .child(
                                            widgets::btn(format!("open-{game_id}"), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-open-game"),
                                                    cx,
                                                ))
                                                .on_click(move |_, _, cx| {
                                                    let game_id = game_id.clone();
                                                    game_view.update(cx, |this, cx| {
                                                        this.select_game(game_id, cx);
                                                    });
                                                }),
                                        )
                                        .when(is_manual, |this| {
                                            this.child(widgets::destroy_btn(
                                                format!("remove-{remove_id}"),
                                                self.strings.get("gui-action-remove"),
                                                move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    let remove_id = remove_id.clone();
                                                    remove_view.update(cx, |this, cx| {
                                                        this.manual_remove = Some(remove_id);
                                                        cx.notify();
                                                    });
                                                },
                                                cx,
                                            ))
                                        }),
                                )
                                .child(
                                    v_flex()
                                        .absolute()
                                        .bottom_1()
                                        .left_1()
                                        .gap_1()
                                        .items_start()
                                        .child(widgets::pill(
                                            store_l,
                                            cx.theme().muted_foreground,
                                            b.border,
                                            cx,
                                        ))
                                        .when_some(
                                            tier.clone().filter(|t| t != "none"),
                                            |this, t| {
                                                this.child(widgets::pill(
                                                    widgets::id_label(
                                                        widgets::ValKind::Tier,
                                                        &t,
                                                        &self.strings,
                                                    ),
                                                    widgets::protondb_color(&t, cx),
                                                    b.border,
                                                    cx,
                                                ))
                                            },
                                        )
                                        .when_some(flag.clone(), |this, f| {
                                            let danger = cx.theme().danger;
                                            this.child(widgets::pill(
                                                f.chip_label(&self.strings),
                                                if f.blocking() { danger } else { b.warning },
                                                b.border,
                                                cx,
                                            ))
                                        })
                                        .when_some(g.api.clone(), |this, a| {
                                            this.child(widgets::pill(
                                                widgets::id_label(
                                                    widgets::ValKind::Api,
                                                    &a,
                                                    &self.strings,
                                                ),
                                                cx.theme().foreground,
                                                b.border,
                                                cx,
                                            ))
                                        }),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .bottom_1()
                                        .right_1()
                                        .tx(t.body_md)
                                        .text_color(ink)
                                        .child(if mods_n == 0 {
                                            self.strings.get("gui-library-unmodded")
                                        } else {
                                            let mut args = FluentArgs::new();
                                            args.set("count", mods_n.to_string());
                                            self.strings.get_args("gui-chip-mods", Some(&args))
                                        }),
                                ),
                        )
                    }),
            )
    }
}
