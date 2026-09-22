use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, v_virtual_list, ActiveTheme};
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
        let name = g.display_name().to_string();
        let mods_n = self.enabled_n(&g.id);
        let row = h_flex()
            .id(id)
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
        .into_any_element()
    }
}
