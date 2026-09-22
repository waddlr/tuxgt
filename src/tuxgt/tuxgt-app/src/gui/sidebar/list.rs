use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    h_flex, v_flex, v_virtual_list, ActiveTheme, Icon, IconName, Sizable as _,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::{FluentArgs, GameIndexRow};

use super::super::theme::{types, TypeStyled as _};

use super::super::*;

/// E107: one owner for the rail art glyph — the decoded image at `w`×`h`
/// (cover) or the game's initials. The well around it differs per rail state.
fn art_glyph(art: Option<Arc<RenderImage>>, w: f32, h: f32, g: &GameIndexRow) -> AnyElement {
    art.map(|img_data| {
        img(ImageSource::Render(img_data))
            .w(px(w))
            .h(px(h))
            .object_fit(ObjectFit::Cover)
            .into_any_element()
    })
    .unwrap_or_else(|| div().child(g.initials()).into_any_element())
}

/// Sidebar quick-play visibility: the row button is a plain launch — it
/// never arms a channel — so it hides where the hero pairs Play with
/// Enable & Play (one button would be ambiguous and must never
/// auto-enable). Exact for the selected row; other rows approximate with
/// installed mods (env/wrapper-only needs can slip through, install-only
/// mods hide) since per-game needs are selected-game-only state.
fn quick_play_visible(
    manager: &str,
    selected: bool,
    armed: bool,
    channel_needed: bool,
    mods_n: usize,
) -> bool {
    if manager != "steam" && manager != "heroic" {
        return true;
    }
    if selected {
        return armed || !channel_needed;
    }
    // Armed state is known for the selected game only; installed mods on
    // any other row mean the hero might pair Play with Enable & Play.
    mods_n == 0
}

impl Shell {
    /// Row height at the active scale. Expanded: two truncated lines plus
    /// `py_1` — rows are positioned by the virtual list, so the height must fit
    /// the content or Large glyphs overlap (E35 rule). Collapsed: the 32px
    /// square plus `py_1` (E107, landmine #4).
    pub(crate) fn sidebar_row_h(collapsed: bool, cx: &App) -> Pixels {
        if collapsed {
            return px(40.);
        }
        let t = types(cx);
        let text = t.headline_md.line + t.body_md.line.max(t.label_lg.line);
        text.max(px(28.)) + px(8.)
    }

    /// Virtual list: only the visible range is built. Search stays outside it;
    /// the list owns the sidebar viewport (landmines #4).
    pub(crate) fn sidebar_list(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let indices = Rc::new(self.visible_indices());
        let h = Self::sidebar_row_h(self.prefs.sidebar_collapsed, cx);
        let sizes: Rc<Vec<Size<Pixels>>> =
            Rc::new(indices.iter().map(|_| size(px(0.), h)).collect());
        v_virtual_list(view, "sb-rows", sizes, move |this, range, _window, cx| {
            let view = cx.entity();
            range
                .filter_map(|i| {
                    let e = this.index.get(*indices.get(i)?)?;
                    Some(this.sidebar_row(e, view.clone(), cx))
                })
                .collect()
        })
        .gap_1()
        .track_scroll(&self.sidebar_scroll)
    }

    pub(crate) fn sidebar_row(&self, g: &GameIndexRow, view: Entity<Self>, cx: &App) -> AnyElement {
        let th = cx.theme();
        let t = types(cx);
        let selected = self.nav == Nav::Game && self.selected.as_deref() == Some(g.id.as_str());
        let collapsed = self.prefs.sidebar_collapsed;
        let id = SharedString::from(format!("sb-{}", g.id));
        let group = SharedString::from(format!("sb-hover-{}", g.id));
        let name = g.display_name().to_string();
        let mods_n = self.enabled_n(&g.id);
        let row = h_flex()
            .id(id)
            .group(group.clone())
            .relative()
            .w_full()
            .gap_2()
            .when(collapsed, |this| this.justify_center().gap_0())
            .px_1()
            .py_1()
            .rounded(px(4.))
            .cursor_pointer()
            .when(selected, |t| {
                t.bg(th.sidebar_accent)
                    .border_l_2()
                    .border_color(cx.theme().primary)
            })
            .when(!selected, |t| t.hover(|s| s.bg(th.group_box)))
            .on_click({
                let id = g.id.clone();
                let view = view.clone();
                move |_, _, cx| {
                    let id = id.clone();
                    view.update(cx, |this, cx| this.select_game(id, cx));
                }
            });
        if collapsed {
            let art = {
                let cached = widgets::art_image(&g.id, widgets::ArtKind::Rail);
                widgets::kick_art_decode(&g.id, widgets::ArtKind::Rail, view.clone(), cx);
                cached
            };
            let tooltip_name = name.clone();
            return row
                .tooltip(move |window, cx| Tooltip::new(tooltip_name.clone()).build(window, cx))
                .child(
                    div()
                        .w(px(32.))
                        .h(px(32.))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(th.border)
                        .bg(th.tab_bar_segmented)
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .tx(t.label_lg)
                        .text_color(cx.theme().muted_foreground)
                        .child(art_glyph(art, 32., 32., g)),
                )
                .into_any_element();
        }
        let armed = self.handle.get(&g.id).copied().unwrap_or(false)
            || self.applied.get(&g.id).copied().unwrap_or(false);
        let quick_play = quick_play_visible(
            &g.manager,
            selected,
            armed,
            self.launch_needs.channel_needed(),
            mods_n,
        );
        let art = {
            let cached = widgets::art_image(&g.id, widgets::ArtKind::Side);
            widgets::kick_art_decode(&g.id, widgets::ArtKind::Side, view.clone(), cx);
            cached
        };
        row.child(
            div()
                .w(px(20.))
                .h(px(28.))
                .rounded(px(2.))
                .border_1()
                .border_color(th.border)
                .bg(th.tab_bar_segmented)
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .tx(t.label_lg)
                .text_color(cx.theme().muted_foreground)
                .child(art_glyph(art, 20., 28., g)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .id(SharedString::from(format!("sb-name-{}", g.id)))
                        .tx(t.headline_md)
                        .min_w_0()
                        .truncate()
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(name.clone())
                        .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx)),
                )
                .when(mods_n > 0, |this| {
                    let mut args = FluentArgs::new();
                    args.set("count", mods_n.to_string());
                    this.child(
                        div()
                            .tx(t.body_md)
                            .text_color(cx.theme().muted_foreground)
                            .child(self.strings.get_args("gui-chip-mods", Some(&args))),
                    )
                }),
        )
        .when(quick_play, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_1()
                    .flex()
                    .items_center()
                    .invisible()
                    .group_hover(group, |this| this.visible())
                    .child(
                        div()
                            .bg(if selected {
                                th.sidebar_accent.opacity(0.9)
                            } else {
                                th.group_box.opacity(0.9)
                            })
                            .rounded(px(4.))
                            .child(
                                Button::new(SharedString::from(format!("sb-play-{}", g.id)))
                                    .ghost()
                                    .small()
                                    .tooltip(self.strings.get("gui-action-play"))
                                    .child(Icon::new(IconName::Play).small())
                                    .on_click({
                                        let id = g.id.clone();
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            let id = id.clone();
                                            view.update(cx, |this, cx| {
                                                this.launch_play(Some(id), None, cx);
                                            });
                                        }
                                    }),
                            ),
                    ),
            )
        })
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::quick_play_visible;

    #[test]
    fn manual_rows_always_offer_quick_play() {
        assert!(quick_play_visible("manual", false, false, false, 0));
        assert!(quick_play_visible("manual", false, false, true, 3));
        assert!(quick_play_visible("manual", true, false, true, 3));
    }

    #[test]
    fn selected_client_hides_only_next_to_enable_and_play() {
        // Vanilla single Play.
        assert!(quick_play_visible("steam", true, false, false, 0));
        // Armed single Play.
        assert!(quick_play_visible("steam", true, true, true, 2));
        assert!(quick_play_visible("heroic", true, true, true, 2));
        // Paired Enable & Play + Play: ambiguous, hide.
        assert!(!quick_play_visible("steam", true, false, true, 2));
        assert!(!quick_play_visible("heroic", true, false, true, 0));
    }

    #[test]
    fn unselected_client_uses_installed_mods_as_proxy() {
        assert!(quick_play_visible("steam", false, false, false, 0));
        assert!(!quick_play_visible("steam", false, false, false, 1));
    }
}
