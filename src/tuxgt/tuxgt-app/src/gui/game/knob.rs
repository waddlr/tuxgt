use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{knob_source, live_knob_value};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{EnvPage, KnobIds, Shell};

impl Shell {
    pub(crate) fn knob_row(
        &self,
        k: &'static tuxgt_core::EnvKnob,
        page: EnvPage,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let unmanaged = self.row_unmanaged(k);
        let game = self.game_row(k.id);
        let global = self.global_knobs.get(k.id);
        let source = match page {
            EnvPage::Game => knob_source(game.as_ref(), global, unmanaged),
            EnvPage::Global => knob_source(None, global, unmanaged),
        };
        let inherit = page == EnvPage::Game
            && matches!(
                source,
                tuxgt_core::KnobSource::Global | tuxgt_core::KnobSource::Unmanaged
            );
        let stored = match page {
            EnvPage::Game => self.knob_values.get(k.id).cloned(),
            EnvPage::Global => global.map(|r| r.value.clone()),
        };
        let enabled = match page {
            EnvPage::Game => game.as_ref().is_some_and(|r| r.enabled),
            EnvPage::Global => global.is_some_and(|r| r.enabled) && !unmanaged,
        };
        let display = if inherit || unmanaged {
            tuxgt_core::effective_knob_value(k, game.as_ref(), global, unmanaged)
                .or(live_knob_value(k))
        } else {
            stored.clone()
        };
        let offer_enable = !(page == EnvPage::Global && unmanaged);
        let boolean = k.values.len() == 1 && k.freeform.is_none();
        // A 0/1 pair is a boolean too: toggle, with the per-value helps on
        // the switch tooltip (same lines the dropdown used to show).
        let binary = !boolean
            && k.freeform.is_none()
            && k.values.len() == 2
            && k.values.iter().any(|v| v.value == "0")
            && k.values.iter().any(|v| v.value == "1");
        // T24: one map lookup per row; the fallback only fires for a knob id
        // the cache never saw (same string, one paint's alloc).
        let ids = self
            .knob_ids
            .get(k.id)
            .cloned()
            .unwrap_or_else(|| KnobIds::for_id(k.id));
        // R40: enable is the left checkbox on both pages. Off on a game row is
        // not unset: the stored value stays and the row inherits global/live.
        let enable_check = Checkbox::new(ids.enable.clone())
            .checked(enabled)
            .tooltip(self.strings.get("gui-tip-knob-enable"))
            .on_change({
                let view = view.clone();
                move |val, _, cx| {
                    let on = *val;
                    view.update(cx, |this, cx| this.set_knob_enabled_ui(k, page, on, cx));
                }
            });
        let value_ctl = if inherit || unmanaged {
            widgets::muted(
                display
                    .clone()
                    .unwrap_or_else(|| self.strings.get("gui-state-unset")),
                cx,
            )
            .into_any_element()
        } else if boolean || binary {
            let on = if binary {
                stored.as_deref() == Some("1")
            } else {
                stored.as_deref().is_some_and(|v| !v.is_empty())
            };
            let switch = Switch::new(ids.value.clone())
                .checked(on)
                .xsmall()
                .on_click({
                    let view = view.clone();
                    move |val, _, cx| {
                        let on = *val;
                        view.update(cx, |this, cx| {
                            if binary {
                                this.write_knob(k.id, page, Some(if on { "1" } else { "0" }), cx);
                            } else if !on && page == EnvPage::Game {
                                this.write_game_omit(k.id, cx);
                            } else {
                                this.set_knob_ui(k.id, page, on, cx);
                            }
                        });
                    }
                });
            if !binary {
                switch.into_any_element()
            } else {
                // Raw div tooltip (same builder as the label): the kit-managed
                // `Switch::tooltip` never paints in this app.
                let lines: Vec<SharedString> = k
                    .values
                    .iter()
                    .map(|v| SharedString::from(format!("{} — {}", v.value, v.help)))
                    .collect();
                div()
                    .id(ids.tip.clone())
                    .tooltip(move |window, cx| {
                        let lines = lines.clone();
                        Tooltip::element(move |_, cx| {
                            v_flex().gap_1().max_w(px(360.)).children(
                                lines
                                    .iter()
                                    .map(|l| div().tx(types(cx).body_md).child(l.clone())),
                            )
                        })
                        .build(window, cx)
                    })
                    .child(switch)
                    .into_any_element()
            }
        } else {
            let unset_label = self.strings.get("gui-state-unset");
            let label = display.clone().unwrap_or_else(|| unset_label.clone());
            widgets::btn(ids.menu.clone(), cx)
                .secondary()
                .child(widgets::blabel(label, cx))
                .dropdown_menu({
                    let view = view.clone();
                    move |menu, _, _| {
                        let mut menu =
                            menu.item(PopupMenuItem::new(unset_label.clone()).on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.set_knob_ui(k.id, page, false, cx)
                                    });
                                }
                            }));
                        for v in k.values {
                            let val = v.value;
                            let view = view.clone();
                            menu = menu.item(
                                PopupMenuItem::new(format!("{} — {}", v.value, v.help)).on_click(
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.write_knob(k.id, page, Some(val), cx);
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    }
                })
                .into_any_element()
        };
        v_flex()
            .child(
                h_flex()
                    .id(k.id)
                    .w_full()
                    .gap_2()
                    .items_center()
                    .px_1()
                    .py_1()
                    .when(offer_enable, |this| this.child(enable_check))
                    .child(v_flex().min_w_0().max_w(px(420.)).child({
                        let keys = SharedString::from(k.env_label());
                        let help = SharedString::from(k.help);
                        div()
                            .id(ids.label.clone())
                            .tx(types(cx).label_lg)
                            .truncate()
                            .child(keys.clone())
                            .tooltip(move |window, cx| {
                                let keys = keys.clone();
                                let help = help.clone();
                                Tooltip::element(move |_, cx| {
                                    v_flex()
                                        .gap_1()
                                        .max_w(px(360.))
                                        .child(div().tx(types(cx).label_lg).child(keys.clone()))
                                        .child(div().tx(types(cx).body_md).child(help.clone()))
                                })
                                .build(window, cx)
                            })
                    }))
                    .child(div().flex_1())
                    .child(h_flex().gap_2().items_center().child(value_ctl).when(
                        page == EnvPage::Global && unmanaged,
                        |this| {
                            // E140: the well must be a dark surface, never a
                            // mid-tone text color — `muted_foreground` as well
                            // reads white-on-light via `overlay_ink`.
                            this.child(widgets::pill(
                                self.strings.get("gui-source-unmanaged"),
                                b.tab_bar_segmented,
                                b.border,
                                cx,
                            ))
                        },
                    )),
            )
            .child(widgets::row_hairline(cx))
            .into_any_element()
    }

    pub(crate) fn set_knob_ui(
        &mut self,
        id: &str,
        page: EnvPage,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if on {
            self.write_knob(id, page, None, cx);
        } else {
            self.write_knob(id, page, Some(""), cx);
        }
    }

    pub(crate) fn set_knob_enabled_ui(
        &mut self,
        k: &'static tuxgt_core::EnvKnob,
        page: EnvPage,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        match page {
            EnvPage::Global => self.write_global_enabled(k.id, on, cx),
            EnvPage::Game => {
                if on {
                    if !self.knob_values.contains_key(k.id) {
                        let g = self.global_knobs.get(k.id);
                        let inherited =
                            tuxgt_core::effective_knob_value(k, None, g, self.row_unmanaged(k))
                                .or_else(|| live_knob_value(k));
                        if let Some(v) = inherited {
                            let owned = v;
                            self.write_knob(k.id, page, Some(&owned), cx);
                        }
                    } else {
                        self.write_game_enabled(k.id, true, cx);
                    }
                } else if self.knob_values.contains_key(k.id) {
                    self.write_game_enabled(k.id, false, cx);
                }
            }
        }
    }
}
