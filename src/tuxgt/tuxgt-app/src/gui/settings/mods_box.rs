use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::*;

use super::super::theme::types;
use super::super::widgets;
use super::super::{AddForm, InstanceRow, SettingsModsTab, Shell};

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
            return self.mods_chrome("mods-optiscaler", cx).child(self.config_page(view, cx));
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
            .child(widgets::muted(self.strings.get("gui-note-instances"), cx))
    }

    pub(crate) fn mods_reshade_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // `gui.mod-config-edit` takeover: an open Global editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_global_id().is_some() {
            return self.mods_chrome("mods-reshade", cx).child(self.config_page(view, cx));
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
                    .filter(|i| i.mod_type == "reshade")
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
                    .children({
                        let needle = self
                            .packs_filter_input
                            .read(cx)
                            .value()
                            .to_string()
                            .trim()
                            .to_lowercase();
                        let visible: Vec<&InstanceRow> = pack_rows(&self.instances)
                            .filter(|i| {
                                super::Shell::mod_matches_needle(&i.label, &i.id, &needle)
                            })
                            .collect();
                        if visible.is_empty() {
                            vec![
                                widgets::muted(self.strings.get("gui-empty-mods-filter"), cx)
                                    .into_any_element(),
                            ]
                        } else {
                            visible
                                .into_iter()
                                .map(|i| self.mod_row(view.clone(), i, cx).into_any_element())
                                .collect()
                        }
                    }),
            )
            .child(widgets::muted(self.strings.get("gui-note-instances"), cx))
    }

    pub(crate) fn mods_custom_box(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // `gui.mod-config-edit` takeover: an open Global editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_global_id().is_some() {
            return self.mods_chrome("mods-custom", cx).child(self.config_page(view, cx));
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
            .child(widgets::muted(self.strings.get("gui-note-instances"), cx))
    }
}
