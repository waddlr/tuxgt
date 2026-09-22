use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tab::TabBar;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ConfigNavPending, InstanceRow, SettingsModsTab, Shell};

impl Shell {
    pub(crate) fn settings_mods(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let tab_ix = match self.mods_tab {
            SettingsModsTab::Optiscaler => 0,
            SettingsModsTab::Reshade => 1,
            SettingsModsTab::Custom => 2,
        };
        v_flex()
            .id("settings-mods")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .child(
                TabBar::new("mods-tabs")
                    .underline()
                    .small()
                    .selected_index(tab_ix)
                    .on_click({
                        let view = view.clone();
                        move |ix, _, cx| {
                            let target = match *ix {
                                1 => SettingsModsTab::Reshade,
                                2 => SettingsModsTab::Custom,
                                _ => SettingsModsTab::Optiscaler,
                            };
                            view.update(cx, |this, cx| {
                                if target == this.mods_tab {
                                    return;
                                }
                                if !this.try_leave_config(
                                    ConfigNavPending::ModsTab(target),
                                    cx,
                                ) {
                                    return;
                                }
                                this.mods_tab = target;
                                this.persist_mods_tab();
                                this.add_form = None;
                                this.scroll_page_top();
                                cx.notify();
                            });
                        }
                    })
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-optiscaler"),
                    ))
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-reshade"),
                    ))
                    .child(settings_tab(
                        IconName::File,
                        self.strings.get("gui-tab-mods-custom"),
                    )),
            )
            .child(widgets::muted(
                self.strings.get("gui-note-settings-mods"),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(self.mods_resync_button(view.clone(), cx))
                    .child({
                        let ids = self.visible_user_mod_ids(cx);
                        let empty = ids.is_empty();
                        widgets::destroy_btn(
                            "mods-remove-visible",
                            self.strings.get("gui-action-remove-visible"),
                            {
                                let view = view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.confirm_remove_visible_mods(window, cx);
                                    });
                                }
                            },
                            cx,
                        )
                        .disabled(empty)
                    }),
            )
            .child(if self.pending_archive_password.is_some() {
                self.archive_password_box(view.clone(), cx)
                    .into_any_element()
            } else {
                match self.mods_tab {
                    SettingsModsTab::Optiscaler => {
                        self.mods_optiscaler_box(view, cx).into_any_element()
                    }
                    SettingsModsTab::Reshade => self.mods_reshade_box(view, cx).into_any_element(),
                    SettingsModsTab::Custom => self.mods_custom_box(view, cx).into_any_element(),
                }
            })
    }

    pub(crate) fn mod_row(
        &self,
        view: Entity<Self>,
        i: &InstanceRow,
        cx: &App,
    ) -> impl IntoElement {
        let id = i.id.clone();
        // Official recipes ship their URL; `manual_url` as a painted source only makes
        // sense where the URL is user input (user Mods / templates), so officials show id only.
        if i.official {
            // E104: poll-fresh Available note + payload-only Update.
            // Officials stay enable-only otherwise (E86 lock 12).
            if self.catalog_updates.contains(&i.id) {
                let id = i.id.clone();
                let id2 = i.id.clone();
                let view2 = view.clone();
                return v_flex()
                    .w_full()
                    .gap_1()
                    .child(widgets::labeled_row(
                        i.ids.row.clone(),
                        i.label.clone(),
                        Some(SharedString::from(i.id.clone())),
                        Switch::new(i.ids.enable.clone())
                            .checked(i.enabled)
                            .xsmall()
                            .on_click({
                                let view = view.clone();
                                move |on, _, cx| {
                                    let on = *on;
                                    let id = id.clone();
                                    view.update(cx, |this, cx| {
                                        this.set_instance_offered_ui(id, on, cx);
                                    });
                                }
                            }),
                        cx,
                    ))
                    .child(
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
                                        view2.update(cx, |this, cx| {
                                            this.refresh_catalog_payload_ui(id2.clone(), cx);
                                        });
                                    }),
                            ),
                    )
                    .into_any_element();
            }
            return widgets::labeled_row(
                i.ids.row.clone(),
                i.label.clone(),
                Some(SharedString::from(i.id.clone())),
                Switch::new(i.ids.enable.clone())
                    .checked(i.enabled)
                    .xsmall()
                    .on_click({
                        let view = view.clone();
                        move |on, _, cx| {
                            let on = *on;
                            let id = id.clone();
                            view.update(cx, |this, cx| {
                                this.set_instance_offered_ui(id, on, cx);
                            });
                        }
                    }),
                cx,
            )
            .into_any_element();
        }
        let b = cx.theme();
        let subtitle = SharedString::from(format!("{}  ·  {}", i.id, source_label(&i.source)));
        let (preview_buttons, preview_bodies) = self.preview_controls(
            &i.id,
            "pack",
            &i.mod_type,
            &i.effect_files,
            i.asset.as_deref(),
            i.payload_present,
            view.clone(),
            cx,
            true,
        );
        h_flex()
            .id(i.ids.row.clone())
            .w_full()
            .overflow_hidden()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.group_box)
            .child(widgets::accent_stripe(widgets::mod_stripe(&i.mod_type, cx)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .p_2()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Switch::new(i.ids.enable.clone())
                                    .checked(i.enabled)
                                    .xsmall()
                                    .on_click({
                                        let view = view.clone();
                                        move |on, _, cx| {
                                            let on = *on;
                                            let id = id.clone();
                                            view.update(cx, |this, cx| {
                                                this.set_instance_offered_ui(id, on, cx);
                                            });
                                        }
                                    }),
                            )
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .tx(types(cx).headline_md)
                                            .truncate()
                                            .child(i.label.clone()),
                                    )
                                    .child(widgets::muted(subtitle, cx)),
                            )
                            .child(div().flex_1())
                            .child(
                                widgets::btn(i.ids.export.clone(), cx)
                                    .secondary()
                                    .child(widgets::blabel(
                                        self.strings.get("gui-action-export"),
                                        cx,
                                    ))
                                    .on_click({
                                        let view = view.clone();
                                        let id = i.id.clone();
                                        move |_, window, cx| {
                                            pick_export(view.clone(), id.clone(), window, cx);
                                        }
                                    }),
                            )
                            .child(
                                widgets::btn(i.ids.rescan.clone(), cx)
                                    .secondary()
                                    .child(widgets::blabel(
                                        self.strings.get("gui-action-rescan-instance"),
                                        cx,
                                    ))
                                    .on_click({
                                        let view = view.clone();
                                        let id = i.id.clone();
                                        move |_, _, cx| {
                                            start_rescan(view.clone(), id.clone(), cx);
                                        }
                                    }),
                            )
                            .child(widgets::destroy_btn(
                                i.ids.remove.clone(),
                                self.strings.get("gui-action-remove"),
                                {
                                    let view = view.clone();
                                    let id = i.id.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.remove_instance_ui(id.clone(), cx);
                                        });
                                    }
                                },
                                cx,
                            )),
                    )
                    // E104: poll-fresh Available note + payload-only Update
                    // under the header row. No manifest touch here.
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
                                            view.update(cx, |this, cx| {
                                                this.refresh_catalog_payload_ui(id.clone(), cx);
                                            });
                                        }),
                                ),
                        )
                    })
                    .when(!preview_buttons.is_empty(), |this| {
                        this.child(h_flex().gap_1().items_center().children(preview_buttons))
                    })
                    .children(preview_bodies),
            )
            .into_any_element()
    }
}
