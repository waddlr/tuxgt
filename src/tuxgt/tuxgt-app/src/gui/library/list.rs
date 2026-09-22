use super::*;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, GameRow};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn card_list(
        &self,
        games: &[&GameRow],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let (first, last) = self.card_window(games.len(), Self::list_row_h(cx) + 4., 1);
        v_flex().id("list").gap_1().children(
            games
                .iter()
                .enumerate()
                .map(|(i, g)| self.game_list_row(g, (first..last).contains(&i), view.clone(), cx)),
        )
    }

    /// Library list-row height estimate: two truncated lines plus `p_2`.
    pub(crate) fn list_row_h(cx: &App) -> f32 {
        let t = types(cx);
        (t.headline_md.line + t.body_md.line).as_f32() + 16.
    }

    /// Index range whose boxes can be on screen, with half a viewport of
    /// slack. Only these covers decode; the rest of the library stays
    /// placeholder until scrolled near (E37).
    pub(crate) fn game_list_row(
        &self,
        g: &GameRow,
        decode: bool,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let t = types(cx);
        let id = g.id.clone();
        let remove_view = view.clone();
        let art = if decode {
            let art = widgets::art_image(&g.id, widgets::ArtKind::List);
            widgets::kick_art_decode(&g.id, widgets::ArtKind::List, view.clone(), cx);
            art
        } else {
            None
        };
        let store_l = game_store_label(g, &self.games, &self.strings);
        h_flex()
            .id(SharedString::from(format!("row-{}", g.id)))
            .min_h(t.row_h)
            .gap_2()
            .p_2()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.group_box)
            .cursor_pointer()
            .hover(|s| s.border_color(cx.theme().primary))
            .on_click(move |_, _, cx| {
                let id = id.clone();
                view.update(cx, |this, cx| this.select_game(id, cx));
            })
            .child(
                art.map(|img_data| {
                    img(ImageSource::Render(img_data))
                        .w(px(48.))
                        .h(px(32.))
                        .object_fit(ObjectFit::Cover)
                        .into_any_element()
                })
                .unwrap_or_else(|| {
                    div()
                        .w(px(48.))
                        .h(px(32.))
                        .bg(b.tab_bar_segmented)
                        .into_any_element()
                }),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .tx(t.headline_md)
                            .truncate()
                            .child(g.display_name().to_string()),
                    )
                    .child(
                        div()
                            .tx(t.body_md)
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child({
                                let store_l = store_l.clone();
                                if self.enabled_n(&g.id) > 0 {
                                    let mut args = FluentArgs::new();
                                    args.set("manager", store_l.clone());
                                    args.set("count", self.enabled_n(&g.id).to_string());
                                    self.strings.get_args("gui-chip-manager-mods", Some(&args))
                                } else {
                                    let mut args = FluentArgs::new();
                                    args.set("manager", store_l);
                                    self.strings.get_args("gui-chip-manager-stock", Some(&args))
                                }
                            }),
                    ),
            )
            .child(widgets::pill(
                store_l,
                cx.theme().muted_foreground,
                b.border,
                cx,
            ))
            .when(g.manager == "manual", |this| {
                let remove_id = g.id.clone();
                let remove_view = remove_view.clone();
                this.child(widgets::destroy_btn(
                    SharedString::from(format!("row-remove-{}", g.id)),
                    self.strings.get("gui-action-remove"),
                    move |_, _, cx| {
                        // The row itself opens the game; a destroy control
                        // must not.
                        cx.stop_propagation();
                        let remove_id = remove_id.clone();
                        remove_view.update(cx, |this, cx| {
                            this.manual_remove = Some(remove_id);
                            cx.notify();
                        });
                    },
                    cx,
                ))
            })
    }

    pub(crate) fn library_footer(&self, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        let proton = self
            .games
            .iter()
            .filter(|g| g.platform.as_deref() == Some("proton") || g.prefix_path.is_some())
            .count();
        let mods: usize = self.mod_counts.values().copied().sum();
        h_flex()
            .id("lib-foot")
            .gap_3()
            .p_2()
            .rounded(px(4.))
            .bg(b.sidebar)
            .border_1()
            .border_color(b.border)
            .child({
                let mut args = FluentArgs::new();
                args.set("count", self.games.len().to_string());
                widgets::mono(
                    self.strings
                        .get_args("gui-chip-titles-detected", Some(&args)),
                    cx,
                )
            })
            .child({
                let mut args = FluentArgs::new();
                args.set("count", proton.to_string());
                widgets::mono(self.strings.get_args("gui-chip-prefixes", Some(&args)), cx)
            })
            .child({
                let mut args = FluentArgs::new();
                args.set("count", mods.to_string());
                div()
                    .tx(types(cx).label_lg)
                    .text_color(b.success)
                    .child(self.strings.get_args("gui-meta-mods-enabled", Some(&args)))
            })
    }
}
