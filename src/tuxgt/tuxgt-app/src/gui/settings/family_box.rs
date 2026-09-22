use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use tuxgt_core::FluentArgs;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    /// HDR packs mint card: vendor-badged rows, per-row game dropdown,
    /// filter + capped viewport, Add/Cancel. Label always auto.
    pub(crate) fn family_mint_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(m) = self.family_mint.as_ref() else {
            return div().into_any_element();
        };
        let b = cx.theme();
        let filter = self.family_filter_input.read(cx).value().to_string();
        let needle = filter.trim().to_lowercase();
        let visible: Vec<usize> = m
            .assets
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                needle.is_empty()
                    || r.name.to_lowercase().contains(&needle)
                    || r.vendor.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        let visible_keys: Vec<String> = visible.iter().map(|&i| m.assets[i].key()).collect();
        let all_on = !visible_keys.is_empty()
            && visible_keys
                .iter()
                .all(|k| m.selected.iter().any(|s| s == k));
        let select_label = self.strings.get("gui-action-select-visible");
        let t = types(cx);
        let list_cap = (t.label_lg.line + t.body_md.line + px(8.)) * 10.;
        let game_opts: Vec<(String, String)> = self
            .index
            .iter()
            .map(|g| (g.id.clone(), g.display_name().to_string()))
            .collect();
        let picked = m.selected.len();
        let mut cargs = FluentArgs::new();
        cargs.set("count", picked.to_string());
        let picked_txt = self.strings.get_args("gui-chip-mods", Some(&cargs));
        let pick_placeholder = self.strings.get("gui-placeholder-family-game");
        let game_label = self.strings.get("gui-label-family-game");
        let game_note = self.strings.get("gui-note-family-game");
        widgets::section_card("family-mint", cx)
            .mt_2()
            .when(m.loading, |this| {
                this.child(widgets::muted(self.strings.get("gui-family-loading"), cx))
            })
            .when_some(m.err.clone(), |this, err| {
                let mut eargs = FluentArgs::new();
                eargs.set("error", err);
                this.child(widgets::muted(
                    self.strings
                        .get_args("gui-family-error-loading", Some(&eargs)),
                    cx,
                ))
            })
            .child(Styled::h(
                Input::new(&self.family_filter_input)
                    .xsmall()
                    .text_size(t.body_md.size)
                    .cleanable(true),
                t.control_h,
            ))
            .when(
                !m.loading && m.err.is_none() && visible.is_empty(),
                |this| this.child(widgets::muted(self.strings.get("gui-family-empty"), cx)),
            )
            .when(!m.loading && m.err.is_none(), |this| {
                this.child(
                    h_flex()
                        .id("family-select-visible")
                        .w_full()
                        .gap_2()
                        .items_center()
                        .px_1()
                        .py_1()
                        .cursor_pointer()
                        .on_click({
                            let view = view.clone();
                            let keys = visible_keys.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.toggle_family_select_visible(keys.clone(), !all_on, cx);
                                });
                            }
                        })
                        .child(
                            div().w(px(24.)).flex_none().child(
                                Checkbox::new("family-select-visible-check")
                                    .checked(all_on)
                                    .accessibility_label(select_label.clone()),
                            ),
                        )
                        .child(div().tx(t.label_lg).child(select_label.clone())),
                )
            })
            .child(
                v_flex()
                    .id("family-assets")
                    .w_full()
                    .max_h(list_cap)
                    .overflow_y_scroll()
                    .track_scroll(&self.inner_scrolls[1])
                    .on_scroll_wheel(widgets::chain_inner(self.inner_scrolls[1].clone()))
                    .children(visible.into_iter().map(|i| {
                        let row = &m.assets[i];
                        let key = row.key();
                        let checked = m.selected.iter().any(|s| s == &key);
                        let row_view = view.clone();
                        let current = m.game_for.get(&key).cloned().unwrap_or_default();
                        let picked_label = game_opts
                            .iter()
                            .find(|(id, _)| *id == current)
                            .map(|(_, n)| n.clone())
                            .unwrap_or_else(|| pick_placeholder.clone());
                        h_flex()
                            .id(SharedString::from(format!("family-asset-{i}")))
                            .w_full()
                            .gap_2()
                            .items_center()
                            .px_1()
                            .py_1()
                            .cursor_pointer()
                            .on_click({
                                let row_view = row_view.clone();
                                let key = key.clone();
                                move |_, _, cx| {
                                    row_view.update(cx, |this, cx| {
                                        this.select_family_asset(key.clone(), cx);
                                    });
                                }
                            })
                            .when(checked, |this| this.bg(b.sidebar))
                            .child(
                                h_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_2()
                                    .child(
                                        Checkbox::new(SharedString::from(format!(
                                            "family-check-{i}"
                                        )))
                                        .checked(checked)
                                        .accessibility_label(row.name.clone()),
                                    )
                                    .child(
                                        v_flex()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .tx(t.label_lg)
                                                            .truncate()
                                                            .child(row.name.clone()),
                                                    )
                                                    .child(widgets::muted(row.vendor.clone(), cx)),
                                            )
                                            .child(widgets::muted(row.tag.clone(), cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("family-game-stop-{i}")))
                                    .on_click(|_, _, cx| cx.stop_propagation())
                                    .child(widgets::value_btn(
                                        SharedString::from(format!("family-game-{i}")),
                                        picked_label.clone(),
                                        {
                                            let view2 = row_view.clone();
                                            let key2 = key.clone();
                                            let game_opts2 = game_opts.clone();
                                            let current2 = current.clone();
                                            move |menu, _, _| {
                                                let mut menu = menu;
                                                for (id, name) in &game_opts2 {
                                                    let on = *id == current2;
                                                    menu = menu.item(
                                                        PopupMenuItem::new(name.clone())
                                                            .checked(on)
                                                            .on_click({
                                                                let view = view2.clone();
                                                                let key = key2.clone();
                                                                let id = id.clone();
                                                                move |_, _, cx| {
                                                                    view.update(cx, |this, cx| {
                                                                        this.set_family_game(
                                                                            key.clone(),
                                                                            id.clone(),
                                                                            cx,
                                                                        );
                                                                    });
                                                                }
                                                            }),
                                                    );
                                                }
                                                menu
                                            }
                                        },
                                        cx,
                                    )),
                            )
                            .into_any_element()
                    })),
            )
            .child(widgets::labeled_row(
                "family-game",
                game_label,
                None,
                widgets::muted(game_note, cx),
                cx,
            ))
            .child(widgets::muted(picked_txt, cx))
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        widgets::btn("family-save", cx)
                            .primary()
                            .child(widgets::blabel(self.strings.get("gui-action-add"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.mint_family_mod(cx));
                                }
                            }),
                    )
                    .child(
                        widgets::btn("family-cancel", cx)
                            .secondary()
                            .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.family_mint = None;
                                        cx.notify();
                                    });
                                }
                            }),
                    ),
            )
            .into_any_element()
    }
}
