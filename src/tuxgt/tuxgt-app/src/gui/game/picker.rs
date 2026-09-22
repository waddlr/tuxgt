use std::collections::HashMap;

use super::*;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, Sizable as _};
use gpui_kit::*;
use tuxgt_core::{config_dir, data_dir, list_mods, FluentArgs};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ModRow, SettingsModsTab, Shell};

impl Shell {
    pub(crate) fn mods_picker_panel(
        &self,
        game_id: &str,
        view: Entity<Shell>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let t = types(cx);
        let rows = picker_rows(self.mods.get(game_id).cloned().unwrap_or_default());
        let needle = self
            .picker_filter_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_lowercase();
        let checked = self.picker_checked.clone();
        // Select Visible scope: rows in expanded sections, filter applied. A
        // collapsed section is out of scope, so a check/uncheck made before
        // collapsing survives.
        let targets = picker_targets(&rows, &self.picker_collapsed, &needle);
        let all_on =
            !targets.is_empty() && targets.iter().all(|id| checked.iter().any(|c| c == id));
        let select_label = self.strings.get("gui-action-select-visible");
        let select_visible_view = view.clone();
        let list: AnyElement = if rows.is_empty() {
            widgets::muted(self.strings.get("gui-empty-picker-all-installed"), cx)
                .into_any_element()
        } else if !rows
            .iter()
            .any(|r| Shell::mod_matches_needle(&r.label, &r.instance, &needle))
        {
            widgets::muted(self.strings.get("gui-empty-mods-filter"), cx).into_any_element()
        } else {
            v_flex()
                .gap_2()
                .children(SettingsModsTab::ALL.iter().flat_map(|tab| {
                    self.picker_section(*tab, &rows, &checked, &needle, view.clone(), cx)
                }))
                .into_any_element()
        };
        let install_label = if checked.is_empty() {
            self.strings.get("gui-action-install")
        } else {
            let mut nargs = FluentArgs::new();
            nargs.set("n", checked.len().to_string());
            self.strings.get_args("gui-action-install-n", Some(&nargs))
        };
        let header = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(div().flex_1().min_w_0().child(widgets::section_title(
                self.strings.get("gui-title-mods-picker"),
                cx,
            )))
            .child(
                widgets::btn("mods-picker-install", cx)
                    .primary()
                    .child(widgets::blabel(install_label, cx))
                    .disabled(checked.is_empty())
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.mods_picker_open = false;
                                let ids = std::mem::take(&mut this.picker_checked);
                                this.install_picked(ids, cx);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                widgets::btn("mods-picker-cancel", cx)
                    .ghost()
                    .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.mods_picker_open = false;
                                this.picker_checked.clear();
                                cx.notify();
                            });
                        }
                    }),
            );
        v_flex()
            .id("mods-picker")
            .gap_2()
            .p_3()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.sidebar)
            .child(header)
            .child(Styled::h(
                Input::new(&self.picker_filter_input)
                    .xsmall()
                    .text_size(t.body_md.size)
                    .cleanable(true),
                t.control_h,
            ))
            .child(
                h_flex()
                    .id("mods-picker-select-visible")
                    .w_full()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .on_click({
                        let view = select_visible_view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_select_visible_picker(!all_on, cx);
                            });
                        }
                    })
                    .child(
                        Checkbox::new("mods-picker-select-visible-check")
                            .checked(all_on)
                            .accessibility_label(select_label.clone()),
                    )
                    .child(div().tx(t.headline_md).child(select_label)),
            )
            .child(list)
    }

    /// One picker section: clickable header (collapses), then the Settings
    /// Mods page's subheads as visual breaks. Empty sections/subheads paint
    /// nothing; a collapsed section paints its header only.
    pub(crate) fn picker_section(
        &self,
        tab: SettingsModsTab,
        rows: &[ModRow],
        checked: &[String],
        needle: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> Vec<AnyElement> {
        let groups = picker_groups(rows, tab, needle);
        if groups.is_empty() {
            return Vec::new();
        }
        let collapsed = self.picker_collapsed.contains(tab.pref_id());
        let arrow = if collapsed { "▸" } else { "▾" };
        let mut out = vec![h_flex()
            .id(SharedString::from(format!(
                "picker-section-{}",
                tab.pref_id()
            )))
            .w_full()
            .gap_1()
            .items_center()
            .cursor_pointer()
            .on_click({
                let view = view.clone();
                let key = tab.pref_id().to_string();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        if !this.picker_collapsed.insert(key.clone()) {
                            this.picker_collapsed.remove(&key);
                        }
                        cx.notify();
                    });
                }
            })
            .child(widgets::section_title(
                format!("{} {arrow}", self.strings.get(tab.label_key())),
                cx,
            ))
            .into_any_element()];
        if collapsed {
            return out;
        }
        for (key, sub) in groups {
            out.push(widgets::muted(self.strings.get(key), cx).into_any_element());
            out.extend(
                sub.into_iter()
                    .map(|row| self.picker_row(row, checked, view.clone(), cx)),
            );
        }
        out
    }

    /// One picker row: checkbox + name (tooltip = Mod id) + previews.
    pub(crate) fn picker_row(
        &self,
        row: &ModRow,
        checked: &[String],
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let t = types(cx);
        let id = row.instance.clone();
        let (preview_buttons, preview_bodies) = self.preview_controls(
            &id,
            "pick",
            &row.mod_type,
            &row.effect_files,
            row.asset.as_deref(),
            row.payload_present,
            view.clone(),
            cx,
            false,
        );
        let click_view = view.clone();
        let click_id = id.clone();
        let click_on = !checked.iter().any(|c| c == &id);
        v_flex()
            .w_full()
            .child(
                h_flex()
                    .id(SharedString::from(format!("pick-row-{id}")))
                    .cursor_pointer()
                    .on_click({
                        let view = click_view.clone();
                        let id = click_id.clone();
                        move |_, _, cx| {
                            let on = click_on;
                            view.update(cx, |this, cx| {
                                this.toggle_picker(&id, on);
                                cx.notify();
                            });
                        }
                    })
                    .gap_2()
                    .items_center()
                    .child(
                        Checkbox::new(SharedString::from(format!("pick-{id}")))
                            .checked(checked.iter().any(|c| c == &id))
                            .accessibility_label(row.label.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("pick-name-{id}")))
                            .flex_1()
                            .min_w_0()
                            .tx(t.headline_md)
                            .truncate()
                            .tooltip({
                                let tip = id.clone();
                                move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
                            })
                            .child(row.label.clone()),
                    )
                    .children(preview_buttons),
            )
            .children(preview_bodies)
            .into_any_element()
    }

    /// Per-dest file rows on an installed card, grouped into R47 sections
    /// (Load DLLs / Include Files). E78: each dest renders once as
    /// checkbox + `source-base → dest` mapping + sync pill; the old Staging
    /// duplicate list is gone. Required dests stay locked on (E64).
    pub(crate) fn toggle_picker(&mut self, id: &str, on: bool) {
        tracing::debug!(action = "toggle-picker", source = id, on);
        if on {
            if !self.picker_checked.iter().any(|c| c == id) {
                self.picker_checked.push(id.to_string());
            }
        } else {
            self.picker_checked.retain(|c| c != id);
        }
    }

    /// Picker Select Visible: the header checkbox state applied to the expanded,
    /// filter-matching rows in section-major row order. Collapsed sections
    /// are out of scope, so a check/uncheck made before collapsing survives.
    pub(crate) fn toggle_select_visible_picker(&mut self, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "toggle-select-visible", on);
        let needle = self
            .picker_filter_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_lowercase();
        let Some(game) = self.selected.clone() else {
            return;
        };
        let rows = picker_rows(self.mods.get(&game).cloned().unwrap_or_default());
        let targets = picker_targets(&rows, &self.picker_collapsed, &needle);
        if on {
            for id in targets {
                if !self.picker_checked.iter().any(|c| c == &id) {
                    self.picker_checked.push(id);
                }
            }
        } else {
            self.picker_checked
                .retain(|c| !targets.iter().any(|t| t == c));
        }
        cx.notify();
    }

    /// Picker Install: hand the checked ids (check order) to the batch queue.
    /// Writes only through the per-instance E34 path (confirm card).
    /// An in-flight spawn (R70) appends; it does not start a second install.
    pub(crate) fn install_picked(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        tracing::debug!(action = "install-picked", game = self.selected.as_deref().unwrap_or("-"), count = ids.len());
        let current = self.install_current.as_ref().map(|(_, i)| i.as_str());
        let info = self.install_id_info();
        if merge_install_queue(&mut self.install_queue, current, &ids, &info) {
            self.continue_install_queue(cx);
        }
    }

    /// id → (mod_type, recipe requires). Seeds ids from held rows, overlays
    /// (mod_type, requires) from a transient catalog read (E85).
    /// Install-click only, never paint (Settings holds the rows, not games).
    pub(crate) fn install_id_info(&self) -> HashMap<String, (String, Vec<String>)> {
        let mut info = HashMap::new();
        if let Some(game) = &self.selected {
            if let Some(rows) = self.mods.get(game) {
                for r in rows {
                    info.insert(r.instance.clone(), (r.mod_type.clone(), Vec::new()));
                }
            }
        }
        if let Ok(list) = list_mods(&config_dir(), &data_dir()) {
            for i in &list.mods {
                info.entry(i.id.clone())
                    .and_modify(|e: &mut (String, Vec<String>)| {
                        e.0.clone_from(&i.mod_type);
                        e.1 = i.requires.to_vec();
                    })
                    .or_insert_with(|| (i.mod_type.clone(), i.requires.to_vec()));
            }
        }
        info
    }
}
