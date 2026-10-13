use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::super::*;

use super::super::super::theme::{types, TypeStyled as _};
use super::super::super::widgets;
use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::Icon;

use super::super::super::{InstanceRow, Shell};
use super::provide_ui::{clear_provided, pick_provide};

impl Shell {
    /// One catalog card. Official, provided, pack, family, and user mods share
    /// it: name on the left, controls on the right, one Details disclosure.
    /// Rescan and Export stay on a local recipe the user authored.
    pub(crate) fn mod_row(
        &self,
        view: Entity<Self>,
        i: &InstanceRow,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let provided = i.source == "provided";
        let ready = i.provided.as_ref().is_none_or(|p| p.ready);
        let details_open = self.preview_open.contains(&format!("det:{}", i.id));
        let missing = i
            .provided
            .as_ref()
            .map(|p| p.missing.clone())
            .unwrap_or_default();
        let ambiguous = i
            .provided
            .as_ref()
            .map(|p| p.ambiguous.clone())
            .unwrap_or_default();
        let present = i
            .provided
            .as_ref()
            .map(|p| p.present.clone())
            .unwrap_or_default();
        let missing_el = pattern_list(self.strings.get("gui-provide-missing"), &missing, cx);
        let ambiguous_el = pattern_list(self.strings.get("gui-provide-ambiguous"), &ambiguous, cx);
        let present_el = pattern_list(self.strings.get("gui-provide-present"), &present, cx);
        let tools = authors_local_recipe(i);
        h_flex()
            .id(i.ids.row.clone())
            .w_full()
            .overflow_hidden()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.group_box)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                v_flex().min_w_0().flex_1().child(
                                    div()
                                        .min_w_0()
                                        .tx(types(cx).headline_md)
                                        .truncate()
                                        .child(i.label.clone()),
                                ),
                            )
                            .child(
                                h_flex()
                                    .flex_shrink_0()
                                    .gap_1()
                                    .items_center()
                                    .child(self.details_button(view.clone(), i, details_open, cx))
                                    .when(tools, |this| {
                                        let export_id = i.id.clone();
                                        let export_view = view.clone();
                                        let rescan_id = i.id.clone();
                                        let rescan_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.export.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-export"),
                                                    cx,
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    cx.stop_propagation();
                                                    pick_export(
                                                        export_view.clone(),
                                                        export_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                        )
                                        .child(
                                            widgets::btn(i.ids.rescan.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-rescan-instance"),
                                                    cx,
                                                ))
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    start_rescan(
                                                        rescan_view.clone(),
                                                        rescan_id.clone(),
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(!i.official, |this| {
                                        let remove_id = i.id.clone();
                                        let remove_view = view.clone();
                                        this.child(widgets::destroy_btn(
                                            i.ids.remove.clone(),
                                            self.strings.get("gui-action-remove"),
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                remove_view.update(cx, |this, cx| {
                                                    this.confirm_remove_instance_ui(
                                                        remove_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            },
                                            cx,
                                        ))
                                    })
                                    .when(provided && !ready, |this| {
                                        let provide_id = i.id.clone();
                                        let provide_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.provide.clone(), cx)
                                                .secondary()
                                                .child(widgets::blabel(
                                                    self.strings.get("gui-action-provide-files"),
                                                    cx,
                                                ))
                                                .on_click(move |_, window, cx| {
                                                    cx.stop_propagation();
                                                    pick_provide(
                                                        provide_view.clone(),
                                                        provide_id.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(provided && ready, |this| {
                                        let clear_id = i.id.clone();
                                        let clear_view = view.clone();
                                        this.child(
                                            widgets::btn(i.ids.clear.clone(), cx)
                                                .secondary()
                                                .child(Icon::new(FullIconName::Mop).small())
                                                .tooltip(self.strings.get("gui-action-clear"))
                                                .on_click(move |_, _, cx| {
                                                    cx.stop_propagation();
                                                    clear_provided(
                                                        clear_view.clone(),
                                                        clear_id.clone(),
                                                        cx,
                                                    );
                                                }),
                                        )
                                    })
                                    .when(!provided || ready, |this| {
                                        this.child(self.offer_switch(view.clone(), i))
                                    }),
                            ),
                    )
                    .when(!i.description.is_empty(), |this| {
                        this.child(widgets::muted(i.description.clone(), cx))
                    })
                    .when(!i.note.is_empty(), |this| {
                        this.child(widgets::muted(i.note.clone(), cx))
                    })
                    .when(self.catalog_updates.contains(&i.id), |this| {
                        let id = i.id.clone();
                        let view = view.clone();
                        this.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .tx(types(cx).body_md)
                                        .text_color(cx.theme().warning)
                                        .child(self.strings.get("gui-mod-update-available-short")),
                                )
                                .child(
                                    widgets::btn(i.ids.cat_update.clone(), cx)
                                        .primary()
                                        .child(widgets::blabel(
                                            self.strings.get("gui-mod-action-update"),
                                            cx,
                                        ))
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            view.update(cx, |this, cx| {
                                                this.refresh_catalog_payload_ui(id.clone(), cx);
                                            });
                                        }),
                                ),
                        )
                    })
                    .when(provided && !ready, |this| {
                        this.when_some(missing_el, |this, el| this.child(el))
                            .when_some(ambiguous_el, |this, el| this.child(el))
                            .when_some(present_el, |this, el| this.child(el))
                    })
                    .when(details_open, |this| {
                        this.child(self.details_body(view.clone(), i, cx))
                    }),
            )
    }

    /// Height of one `mod_row`. The User Packs window reserves this for
    /// off-screen cards, so a short value clips the scroll and a tall one
    /// opens a gap. `py_1` and `gap_1` are 0.25rem; the card border is 1px.
    /// Rem is `theme.font_size`, the size `Root` installs.
    pub(crate) fn catalog_row_h(&self, id: &str, cx: &App) -> Pixels {
        let t = types(cx);
        let gap = cx.theme().font_size * 0.25;
        let header = t.headline_md.line.max(t.control_h);
        let mut h = header + gap * 2. + px(2.);
        let (has_description, has_note, pattern_lens) = {
            let Some(i) = self.instances.iter().find(|r| r.id == id) else {
                return h;
            };
            let lenses = if i.source == "provided" {
                i.provided
                    .as_ref()
                    .filter(|p| !p.ready)
                    .map(|p| [p.missing.len(), p.ambiguous.len(), p.present.len()])
            } else {
                None
            };
            (!i.description.is_empty(), !i.note.is_empty(), lenses)
        };
        if has_description {
            h += gap + t.body_md.line;
        }
        if has_note {
            h += gap + t.body_md.line;
        }
        if self.catalog_updates.contains(id) {
            h += gap + t.control_h;
        }
        if let Some(lenses) = pattern_lens {
            for n in lenses {
                if n == 0 {
                    continue;
                }
                let n = n as f32;
                h += gap + t.body_md.line + (gap + t.label_lg.line) * n;
            }
        }
        if self.preview_open.contains(&format!("det:{id}")) {
            h += gap + self.details_h(id, cx);
        }
        h
    }
}

/// Heading plus each pattern. `None` when the list is empty so the heading stays hidden.
pub(crate) fn pattern_list(heading: String, items: &[String], cx: &App) -> Option<AnyElement> {
    if items.is_empty() {
        return None;
    }
    Some(
        v_flex()
            .w_full()
            .gap_1()
            .px_1()
            .child(
                div()
                    .tx(types(cx).body_md)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(heading),
            )
            .children(items.iter().map(|item| widgets::mono(item.clone(), cx)))
            .into_any_element(),
    )
}
