use gpui_kit::component::switch::Switch;
use gpui_kit::component::tab::TabBar;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, IconName, Sizable as _};
use gpui_kit::*;

use super::*;

use super::super::widgets;

use super::super::{ConfigNavPending, InstanceRow, SettingsModsTab, Shell};

mod card;
mod provide_ui;

pub(crate) use provide_ui::prompt_provide_with_password;

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
            .gap_3()
            .flex_shrink_0()
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
                                if !this.try_leave_config(ConfigNavPending::ModsTab(target), cx) {
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

    fn offer_switch(&self, view: Entity<Self>, i: &InstanceRow) -> impl IntoElement {
        let id = i.id.clone();
        Switch::new(i.ids.enable.clone())
            .checked(i.enabled)
            .xsmall()
            .on_click(move |on, _, cx| {
                let on = *on;
                view.update(cx, |this, cx| {
                    this.set_instance_offered_ui(id.clone(), on, cx);
                });
            })
    }
}
