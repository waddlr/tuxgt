use std::ops::Range;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;

use super::super::theme::types;
use super::super::widgets;
use super::super::{AddForm, SettingsModsTab, Shell};

impl Shell {
    /// Template Add button: the pick runs E88 classify and prefills the
    /// tweak form from the packaged template for `mod_type` (lock 10).
    /// Recipe TOML stays a classify branch, never a separate button.
    pub(crate) fn mods_add_button(
        &self,
        view: Entity<Self>,
        id: &'static str,
        label_key: &str,
        mod_type: &'static str,
        cx: &App,
    ) -> impl IntoElement {
        widgets::btn(id, cx)
            .secondary()
            .child(widgets::blabel(self.strings.get(label_key), cx))
            .tooltip(self.strings.get("gui-prompt-add-package"))
            .on_click({
                let view = view.clone();
                move |_, window, cx| pick_add(view.clone(), mod_type, window, cx)
            })
    }

    /// Catalog chrome: Re-sync official reloads packaged Mod/Template TOMLs
    /// into the listing (lock 12). One button for the whole Mods page.
    pub(crate) fn mods_resync_button(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        widgets::btn("official-resync", cx)
            .secondary()
            .child(widgets::bicon(IconName::RotateCw))
            .child(widgets::blabel(
                self.strings.get("gui-action-resync-official"),
                cx,
            ))
            .on_click({
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| this.resync_official_ui(cx));
                }
            })
    }

    pub(crate) fn mods_chrome(&self, id: &'static str, cx: &App) -> Stateful<Div> {
        widgets::section_card(id, cx)
    }

    /// E91: the open Add/Rescan form when it belongs on this Mods inner tab.
    pub(crate) fn add_form_for_tab(
        &self,
        tab: SettingsModsTab,
        cx: &App,
    ) -> Option<Entity<AddForm>> {
        self.add_form
            .clone()
            .filter(|form| SettingsModsTab::for_type(&form.read(cx).mod_type) == tab)
    }

    pub(crate) fn mods_optiscaler_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // `gui.mod-config-edit` takeover: an open Global editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_global_id().is_some() {
            return self
                .mods_chrome("mods-optiscaler", cx)
                .child(self.config_page(view, cx));
        }
        let user_empty = tab_rows(&self.instances, SettingsModsTab::Optiscaler, false)
            .next()
            .is_none();
        // Takeover: an open Add/Rescan form replaces the whole tab content
        // (game picker pattern) so the form paints at the top.
        if let Some(form) = self.add_form_for_tab(SettingsModsTab::Optiscaler, cx) {
            return self
                .mods_chrome("mods-optiscaler", cx)
                .child(self.add_panel_box(view, form, cx));
        }
        self.mods_chrome("mods-optiscaler", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-mods-official"),
                cx,
            ))
            .children(
                tab_rows(&self.instances, SettingsModsTab::Optiscaler, true)
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(widgets::section_title(
                        self.strings.get("gui-section-mods-user"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.mods_add_button(
                        view.clone(),
                        "mod-add-optiscaler",
                        "gui-action-add-optiscaler",
                        "optiscaler",
                        cx,
                    )),
            )
            .children(
                tab_rows(&self.instances, SettingsModsTab::Optiscaler, false)
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .when(user_empty, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-empty-no-user-mods"),
                    cx,
                ))
            })
    }

    pub(crate) fn mods_reshade_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // `gui.mod-config-edit` takeover: an open Global editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_global_id().is_some() {
            return self
                .mods_chrome("mods-reshade", cx)
                .child(self.config_page(view, cx));
        }
        let user_empty = tab_rows(&self.instances, SettingsModsTab::Reshade, false)
            .filter(|i| i.mod_type == "reshade")
            .next()
            .is_none();
        let add_form = self.add_form_for_tab(SettingsModsTab::Reshade, cx);
        // Takeover: any open form (Add/Rescan, HDR packs, ReShade packs)
        // replaces the whole tab content like the game picker, so a long
        // User-Mods/User-Packs list never buries the form at the bottom.
        let add_open = add_form.is_some();
        let mint_open = self.family_mint.is_some() || self.extras_mint.is_some();
        if add_open || mint_open {
            return self
                .mods_chrome("mods-reshade", cx)
                .when_some(add_form, |this, form| {
                    this.child(self.add_panel_box(view.clone(), form, cx))
                })
                .when(!add_open && self.family_mint.is_some(), |this| {
                    this.child(self.family_mint_box(view.clone(), cx))
                })
                .when(!add_open && self.extras_mint.is_some(), |this| {
                    this.child(self.reshade_extras_box(view.clone(), cx))
                });
        }
        self.mods_chrome("mods-reshade", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-mods-official"),
                cx,
            ))
            .children(
                tab_rows(&self.instances, SettingsModsTab::Reshade, true)
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(widgets::section_title(
                        self.strings.get("gui-section-mods-user"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.mods_add_button(
                        view.clone(),
                        "mod-add-reshade",
                        "gui-action-add-reshade",
                        "reshade",
                        cx,
                    )),
            )
            .children(
                tab_rows(&self.instances, SettingsModsTab::Reshade, false)
                    .filter(|i| i.mod_type == "reshade")
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .when(user_empty, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-empty-no-user-mods"),
                    cx,
                ))
            })
            .child(
                v_flex()
                    .gap_2()
                    .child({
                        let mut actions = vec![widgets::SectionAction::new(
                            "reshade-extras-open",
                            self.strings.get("gui-action-reshade-extras"),
                            {
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.open_reshade_extras(window, cx)
                                    });
                                }
                            },
                        )];
                        if !self.family_templates.is_empty() {
                            actions.push(widgets::SectionAction::new(
                                "family-add-open",
                                self.strings.get("gui-action-family-add"),
                                {
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.open_family_mint(window, cx)
                                        });
                                    }
                                },
                            ));
                        }
                        actions.push(widgets::SectionAction::new(
                            "mod-add-pack",
                            self.strings.get("gui-action-add-pack"),
                            {
                                let view = view.clone();
                                move |_, window, cx| pick_add_pack(view.clone(), window, cx)
                            },
                        ));
                        widgets::section_header(
                            "mods-packs-head",
                            self.strings.get("gui-section-mods-packs"),
                            self.page_scroll.bounds().size.width,
                            actions,
                            cx,
                        )
                    })
                    .child(Styled::h(
                        Input::new(&self.packs_filter_input)
                            .xsmall()
                            .text_size(types(cx).body_md.size)
                            .cleanable(true),
                        types(cx).control_h,
                    ))
                    .child(self.reshade_pack_rows(view.clone(), cx)),
            )
    }

    /// User Packs body. The page scroller moves the whole tab. Off-screen
    /// cards are a spacer of `catalog_row_h`, so a long list does not rebuild
    /// every card on each wheel tick.
    fn reshade_pack_rows(&self, view: Entity<Self>, cx: &App) -> AnyElement {
        let needle = self
            .packs_filter_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_lowercase();
        let ids: Vec<String> = pack_rows(&self.instances)
            .filter(|i| !i.official && super::Shell::mod_matches_needle(&i.label, &i.id, &needle))
            .map(|i| i.id.clone())
            .collect();
        if ids.is_empty() {
            return widgets::muted(self.strings.get("gui-empty-mods-filter"), cx)
                .into_any_element();
        }
        // `gap_2` is 0.5rem. Rem is `theme.font_size`, same as `catalog_row_h`.
        let gap = cx.theme().font_size * 0.5;
        let heights: Vec<Pixels> = ids.iter().map(|id| self.catalog_row_h(id, cx)).collect();
        let viewport = self.page_scroll.bounds().size.height;
        let scroll_top = -self.page_scroll.offset().y;
        let (view_top, view_bot) = if viewport <= px(32.) {
            (px(-10_000.), px(100_000.))
        } else if let Some(list_top) = self.pack_list_top.get() {
            let overscan = px(240.);
            (
                scroll_top - overscan - list_top,
                scroll_top + viewport + overscan - list_top,
            )
        } else {
            (px(0.), viewport + px(240.))
        };
        let windowed = pack_window(&heights, gap, view_top, view_bot);
        let slot = self.pack_list_top.clone();
        let page = self.page_scroll.clone();
        let shell = view.clone();
        let mut col = v_flex().w_full().child(
            canvas(
                move |bounds, window, cx| {
                    let page_bounds = page.bounds();
                    let content_y = bounds.origin.y - page_bounds.origin.y - page.offset().y;
                    let prev = slot.get();
                    let jumped = prev.is_none_or(|p| (p - content_y).abs() > px(32.));
                    if prev != Some(content_y) {
                        slot.set(Some(content_y));
                    }
                    if jumped {
                        window.defer(cx, move |_, cx| {
                            shell.update(cx, |_, cx| cx.notify());
                        });
                    }
                },
                |_, _, _, _| {},
            )
            .h(px(0.))
            .w_full(),
        );
        if windowed.range.is_empty() {
            return col
                .child(div().w_full().h(windowed.total))
                .into_any_element();
        }
        if windowed.leading > px(0.) {
            col = col.child(div().w_full().h(windowed.leading));
        }
        col = col.child(
            v_flex().w_full().gap_2().children(
                ids.iter()
                    .enumerate()
                    .filter(|(i, _)| windowed.range.contains(i))
                    .filter_map(|(_, id)| {
                        let row = self.instances.iter().find(|r| &r.id == id)?;
                        Some(self.mod_row(view.clone(), row, cx).into_any_element())
                    }),
            ),
        );
        if windowed.trailing > px(0.) {
            col = col.child(div().w_full().h(windowed.trailing));
        }
        col.into_any_element()
    }

    pub(crate) fn mods_custom_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // `gui.mod-config-edit` takeover: an open Global editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_global_id().is_some() {
            return self
                .mods_chrome("mods-custom", cx)
                .child(self.config_page(view, cx));
        }
        let user_empty = tab_rows(&self.instances, SettingsModsTab::Custom, false)
            .next()
            .is_none();
        // Takeover: an open Add/Rescan form replaces the whole tab content
        // (game picker pattern) so the form paints at the top.
        if let Some(form) = self.add_form_for_tab(SettingsModsTab::Custom, cx) {
            return self
                .mods_chrome("mods-custom", cx)
                .child(self.add_panel_box(view, form, cx));
        }
        self.mods_chrome("mods-custom", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-mods-official"),
                cx,
            ))
            .children(
                tab_rows(&self.instances, SettingsModsTab::Custom, true)
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(widgets::section_title(
                        self.strings.get("gui-section-mods-user"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.mods_add_button(
                        view.clone(),
                        "mod-add-custom",
                        "gui-action-add-custom",
                        "custom",
                        cx,
                    )),
            )
            .children(
                tab_rows(&self.instances, SettingsModsTab::Custom, false)
                    .map(|i| self.mod_row(view.clone(), i, cx).into_any_element()),
            )
            .when(user_empty, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-empty-no-user-mods"),
                    cx,
                ))
            })
    }
}

/// Which pack cards intersect `[view_top, view_bot)`, plus the spacer heights
/// that keep the off-screen runs the same size as the cards they replace.
/// `gap` sits between cards, not after the last one. A card that merely
/// touches the edge is outside the window.
fn pack_window(heights: &[Pixels], gap: Pixels, view_top: Pixels, view_bot: Pixels) -> PackWindow {
    let n = heights.len();
    let mut prefix = Vec::with_capacity(n);
    let mut y = px(0.);
    for (i, h) in heights.iter().enumerate() {
        prefix.push(y);
        y += *h;
        if i + 1 != n {
            y += gap;
        }
    }
    let total = y;
    let mut start = 0;
    while start < n && prefix[start] + heights[start] <= view_top {
        start += 1;
    }
    let mut end = start;
    while end < n && prefix[end] < view_bot {
        end += 1;
    }
    if start == end {
        return PackWindow {
            range: start..end,
            leading: px(0.),
            trailing: total,
            total,
        };
    }
    let last = end - 1;
    PackWindow {
        range: start..end,
        leading: prefix[start],
        trailing: total - (prefix[last] + heights[last]),
        total,
    }
}

struct PackWindow {
    range: Range<usize>,
    leading: Pixels,
    trailing: Pixels,
    total: Pixels,
}

#[cfg(test)]
mod tests {
    use super::pack_window;
    use gpui_kit::px;

    #[test]
    fn pack_window_keeps_the_full_list_height() {
        let heights = [px(48.), px(48.), px(48.), px(48.)];
        let gap = px(8.);
        let full = pack_window(&heights, gap, px(0.), px(100.));
        assert_eq!(full.range, 0..2);
        assert_eq!(full.leading, px(0.));
        assert_eq!(full.trailing, px(112.));
        assert_eq!(full.total, px(216.));
        assert_eq!(
            full.leading + px(48.) + gap + px(48.) + full.trailing,
            full.total
        );

        let mid = pack_window(&heights, gap, px(100.), px(200.));
        assert_eq!(mid.range, 1..4);
        assert_eq!(mid.leading, px(56.));
        assert_eq!(mid.trailing, px(0.));
        assert_eq!(mid.leading + px(48.) * 3. + gap * 2., mid.total);

        let above = pack_window(&heights, gap, px(-100.), px(-10.));
        assert!(above.range.is_empty());
        assert_eq!(above.trailing, above.total);

        let below = pack_window(&heights, gap, px(300.), px(400.));
        assert!(below.range.is_empty());
        assert_eq!(below.trailing, below.total);
    }
}
