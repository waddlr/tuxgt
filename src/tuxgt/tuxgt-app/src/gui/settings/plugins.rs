use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName, Sizable as _};
use gpui_kit::*;

use tuxgt_core::{FluentArgs, PluginHost};

use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn settings_core_plugins(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        v_flex()
            .id("settings-core-plugins")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .child(widgets::muted(
                self.strings.get("gui-note-settings-core-plugins"),
                cx,
            ))
            .child(
                self.core_section(
                    "managers",
                    self.strings.get("gui-section-core-managers"),
                    ["steam", "heroic", "manual"]
                        .into_iter()
                        .filter_map(|id| self.core_plugin_row(view.clone(), id, None, cx)),
                    cx,
                ),
            )
            .child(
                self.core_section(
                    "metadata",
                    self.strings.get("gui-section-core-metadata"),
                    ["protondb", "steamgriddb", "awacy"]
                        .into_iter()
                        .filter_map(|id| {
                            let key_editor = (id == "steamgriddb").then(|| {
                                self.secret_key_editor("steamgriddb", view.clone(), cx)
                                    .into_any_element()
                            });
                            self.core_plugin_row(view.clone(), id, key_editor, cx)
                        }),
                    cx,
                ),
            )
            .child(
                self.core_section(
                    "capabilities",
                    self.strings.get("gui-section-core-capabilities"),
                    ["env", "wrapper"]
                        .into_iter()
                        .filter_map(|id| self.core_plugin_row(view.clone(), id, None, cx)),
                    cx,
                ),
            )
            .child(self.core_mods_section(view.clone(), cx))
    }

    pub(crate) fn core_section<E: IntoElement>(
        &self,
        id: &str,
        title: String,
        rows: impl Iterator<Item = E>,
        cx: &App,
    ) -> impl IntoElement {
        widgets::section_card(SharedString::from(format!("core-{id}")), cx)
            .child(widgets::section_title(title, cx))
            .children(rows)
    }

    pub(crate) fn core_plugin_row(
        &self,
        view: Entity<Self>,
        id: &str,
        extra: Option<AnyElement>,
        cx: &App,
    ) -> Option<impl IntoElement> {
        let row = self.plugins.iter().find(|p| p.id == id)?.clone();
        let pid = row.id.clone();
        let b = cx.theme();
        let enabled = row.enabled;
        let is_sgdb = id == "steamgriddb";
        let switch = Switch::new(SharedString::from(format!("pe-{id}")))
            .checked(enabled)
            .xsmall()
            .on_click({
                let view = view.clone();
                move |on, _, cx| {
                    let on = *on;
                    let pid = pid.clone();
                    view.update(cx, |this, cx| {
                        this.set_plugin_enabled_ui(pid, on, cx);
                    });
                }
            });
        let enable_control = if is_sgdb && enabled {
            let gear_view = view.clone();
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    widgets::btn("sgdb-settings", cx)
                        .secondary()
                        .child(widgets::bicon(IconName::Settings))
                        .on_click(move |_, _, cx| {
                            gear_view.update(cx, |this, cx| {
                                this.show_steamgriddb_settings = !this.show_steamgriddb_settings;
                                cx.notify();
                            });
                        }),
                )
                .child(switch)
                .into_any_element()
        } else {
            switch.into_any_element()
        };

        let mut card = v_flex()
            .id(SharedString::from(format!("core-pl-{id}")))
            .gap_1()
            .child(widgets::labeled_row(
                SharedString::from(format!("pl-{id}")),
                row.label.clone(),
                Some(SharedString::from(row.id.clone())),
                enable_control,
                cx,
            ));
        if is_sgdb {
            if enabled && self.show_steamgriddb_settings {
                let set = self
                    .secret_states
                    .get("steamgriddb")
                    .copied()
                    .unwrap_or(false);
                // E140: unset well is a dark surface like every pill —
                // `muted_foreground` as well reads white-on-light.
                let pill_row = widgets::labeled_row(
                    SharedString::from("steamgriddb-key-state"),
                    self.strings.get("gui-section-secret-manager"),
                    None,
                    widgets::pill(
                        self.strings.get(if set {
                            "gui-secret-set"
                        } else {
                            "gui-secret-unset"
                        }),
                        if set {
                            cx.theme().primary
                        } else {
                            b.tab_bar_segmented
                        },
                        b.border,
                        cx,
                    ),
                    cx,
                );
                let subsection = match extra {
                    Some(editor) => v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(b.border)
                        .bg(b.tab_bar_segmented)
                        .child(pill_row)
                        .child(editor),
                    None => v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(b.border)
                        .bg(b.tab_bar_segmented)
                        .child(pill_row),
                };
                card = card.child(subsection);
            }
        } else if let Some(extra) = extra {
            card = card.child(extra);
        }
        Some(card)
    }

    pub(crate) fn core_mods_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        widgets::section_card("core-mods", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-core-mods"),
                cx,
            ))
            .child(widgets::muted(
                self.strings.get("gui-note-settings-plugins"),
                cx,
            ))
            .children(["reshade", "optiscaler"].into_iter().filter_map(|id| {
                let inst = self.instances.iter().find(|i| i.id == id)?.clone();
                let oid = inst.id.clone();
                Some(widgets::labeled_row(
                    SharedString::from(format!("core-mod-{id}")),
                    inst.label.clone(),
                    Some(SharedString::from(inst.id.clone())),
                    Switch::new(SharedString::from(format!("ce-{id}")))
                        .checked(inst.enabled)
                        .xsmall()
                        .on_click({
                            let view = view.clone();
                            move |on, _, cx| {
                                let on = *on;
                                let oid = oid.clone();
                                view.update(cx, |this, cx| {
                                    this.set_instance_offered_ui(oid, on, cx);
                                });
                            }
                        }),
                    cx,
                ))
            }))
    }

    pub(crate) fn set_plugin_enabled_ui(&mut self, id: String, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-plugin-enabled", source = id.as_str(), on);
        match PluginHost::load().and_then(|mut h| h.set_enabled(&id, on)) {
            Ok(()) => {
                if let Some(row) = self.plugins.iter_mut().find(|r| r.id == id) {
                    row.enabled = on;
                }
                self.knobs = super::load_knobs().into_boxed_slice();
                self.knob_ids = super::knob_ids_for(&self.knobs);
                self.disabled_managers = super::load_disabled();
                // Rows stay in the DB; only the display hide changes, and
                // disabled managers narrow at paint (no base recompute). A
                // failed read keeps the held index: an empty one would drop
                // the selection (and persist that) on a transient DB error.
                if let Some(index) = super::load_index_checked() {
                    self.index = index.into_boxed_slice();
                    self.revalidate_selection();
                }
                let mut args = FluentArgs::new();
                args.set("id", id.clone());
                let key = if on {
                    "gui-status-plugin-enabled"
                } else {
                    "gui-status-plugin-disabled"
                };
                self.status = self.strings.get_args(key, Some(&args));
            }
            Err(e) => self.status = format!("{e}"),
        }
        cx.notify();
    }
}
