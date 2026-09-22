use std::collections::HashMap;

use super::*;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, LoadConflict};

use super::super::theme::types;
use super::super::widgets;
use super::super::{
    load_conflicts_id, mods_section_id, ModRow, PendingConfirm, SettingsModsTab, Shell,
};

mod residency;

impl Shell {
    pub(crate) fn mods_tab(&self, game_id: &str, view: Entity<Self>, cx: &App) -> AnyElement {
        // `gui.mod-config-edit` takeover: an open Staged editor replaces the
        // whole tab content (add-form pattern) — the page IS the editor.
        if self.open_staged_for(game_id).is_some() {
            return v_flex()
                .id("mods")
                .gap_2()
                .child(self.config_page(view, cx))
                .into_any_element();
        }
        let t = types(cx);
        let needle = self
            .installed_filter_input
            .read(cx)
            .value()
            .to_string()
            .trim()
            .to_lowercase();
        let listed: &[ModRow] = self
            .mods
            .get(game_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        // Mods-tab sections: one per Settings-Mods tab, officials pinned
        // first. The GUI paints this order and writes it back on a reorder;
        // the engine order is the stored `load_order`.
        let conflicts: &[LoadConflict] = self
            .mod_conflicts
            .get(game_id)
            .map(|v| &v[..])
            .unwrap_or(&[]);
        let sections: Vec<(SettingsModsTab, Vec<&ModRow>, Vec<SectionConflict>)> =
            SettingsModsTab::ALL
                .iter()
                .map(|tab| {
                    let rows: Vec<&ModRow> = section_rows(listed, *tab)
                        .into_iter()
                        .filter(|r| Shell::mod_matches_needle(&r.label, &r.instance, &needle))
                        .collect();
                    (*tab, rows, section_conflicts(listed, *tab, conflicts))
                })
                .collect();
        // Uninstall-visible scope: expanded sections only (picker parity — a
        // collapsed section paints its header only). `has_rows` still tracks
        // every filter-passing row so collapsing all sections is not empty.
        let visible_ids: Vec<String> = installed_targets(&sections, &self.installed_collapsed);
        let has_rows = sections.iter().any(|(_, rows, _)| !rows.is_empty());
        // Picker open / empty list paints no cards: skip the O(stage) group
        // build entirely on those frames.
        let cards_visible = !self.mods_picker_open && !visible_ids.is_empty();
        let stage_by_instance: HashMap<&str, Vec<&super::StageRow>> = if cards_visible {
            let mut map: HashMap<&str, Vec<&super::StageRow>> = HashMap::new();
            if let Some(stage) = self.mod_stage.get(game_id) {
                for s in stage {
                    map.entry(s.instance.as_str()).or_default().push(s);
                }
            }
            map
        } else {
            HashMap::new()
        };
        let actions = vec![
            widgets::SectionAction::new(
                "mods-install-open",
                self.strings.get("gui-action-install"),
                {
                    let open_view = view.clone();
                    move |_, _, cx| {
                        open_view.update(cx, |this, cx| {
                            this.mods_picker_open = true;
                            this.picker_checked.clear();
                            this.scroll_page_top();
                            cx.notify();
                        });
                    }
                },
            )
            .primary(),
            widgets::SectionAction::new(
                "mods-resync-game",
                self.strings.get("gui-mod-action-resync-all"),
                {
                    let game_id = game_id.to_string();
                    let resync_view = view.clone();
                    move |_, _, cx| {
                        resync_view.update(cx, |this, cx| {
                            this.resync_game_ui(&game_id, cx);
                        });
                    }
                },
            )
            .outline(),
            widgets::SectionAction::destroy(
                "mods-uninstall-visible",
                self.strings.get("gui-action-uninstall-visible"),
                {
                    let uninstall_view = view.clone();
                    let ids = visible_ids.clone();
                    move |_, _, cx| {
                        uninstall_view.update(cx, |this, cx| {
                            this.uninstall_visible_ui(ids.clone(), cx);
                        });
                    }
                },
            )
            .disabled(visible_ids.is_empty()),
        ];
        v_flex()
            .id("mods")
            .gap_2()
            .when(!self.mods_picker_open, |this| {
                // Single row: installed filter left, Install / Force
                // re-sync all / Uninstall visible right. The picker covers
                // this row while open.
                this.child(
                    h_flex()
                        .id("mods-actions")
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(
                            div().flex_1().min_w_0().child(Styled::h(
                                Input::new(&self.installed_filter_input)
                                    .xsmall()
                                    .text_size(t.body_md.size)
                                    .cleanable(true),
                                t.control_h,
                            )),
                        )
                        .child(widgets::action_cluster(
                            "mods-actions",
                            actions,
                            self.page_scroll.bounds().size.width < px(widgets::HEADER_WRAP_W),
                            cx,
                        )),
                )
            })
            // Install confirms (Overwrite/Requires) paint inline here; the
            // ClientStop confirm is a shell-root modal (all game tabs).
            .when(
                self.pending_confirm.as_ref().is_some_and(|p| {
                    p.game() == game_id && !matches!(p, PendingConfirm::ClientStop { .. })
                }),
                |this| this.child(self.confirm_card(view.clone(), cx)),
            )
            .when(self.mods_picker_open, |this| {
                this.child(self.mods_picker_panel(game_id, view.clone(), cx))
            })
            .when(!self.mods_picker_open && !has_rows, |this| {
                let empty_key = if needle.is_empty() {
                    "gui-empty-mods-installed"
                } else {
                    "gui-empty-mods-filter"
                };
                this.child(widgets::muted(self.strings.get(empty_key), cx))
            })
            .when(!self.mods_picker_open, |this| {
                this.children(sections.iter().filter(|(_, rows, _)| !rows.is_empty()).map(
                    |(tab, rows, groups)| {
                        // Clickable header (picker parity): a collapsed section
                        // paints its header only.
                        let collapsed = self.installed_collapsed.contains(tab.pref_id());
                        let arrow = if collapsed { "▸" } else { "▾" };
                        let header_view = view.clone();
                        let key = tab.pref_id().to_string();
                        let mut col = v_flex()
                            .id(mods_section_id(*tab))
                            .w_full()
                            .gap_2()
                            .child(
                                h_flex()
                                    .id(SharedString::from(format!(
                                        "mods-section-{}-header",
                                        tab.pref_id()
                                    )))
                                    .w_full()
                                    .gap_1()
                                    .items_center()
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        header_view.update(cx, |this, cx| {
                                            if !this.installed_collapsed.insert(key.clone()) {
                                                this.installed_collapsed.remove(&key);
                                            }
                                            cx.notify();
                                        });
                                    })
                                    .child(widgets::section_title(
                                        format!("{} {arrow}", self.strings.get(tab.label_key())),
                                        cx,
                                    )),
                            );
                        if collapsed {
                            return col.into_any_element();
                        }
                        col = col
                            .when_some(
                                self.conflicts_block(game_id, *tab, groups, view.clone(), cx),
                                |this, block| this.child(block),
                            )
                            .children(rows.iter().enumerate().map(|(i, row)| {
                                let stage: &[&super::StageRow] = stage_by_instance
                                    .get(row.instance.as_str())
                                    .map(|v| v.as_slice())
                                    .unwrap_or(&[]);
                                // Officials stay pinned above user rows, so
                                // that boundary is the one refused swap.
                                let can_up = i > 0 && rows[i - 1].official == row.official;
                                let can_down =
                                    i + 1 < rows.len() && rows[i + 1].official == row.official;
                                self.mod_card(
                                    game_id,
                                    row,
                                    stage,
                                    can_up,
                                    can_down,
                                    view.clone(),
                                    cx,
                                )
                            }));
                        col.into_any_element()
                    },
                ))
            })
            .into_any_element()
    }

    /// One section's Load-conflicts block: groups whose contenders all sit in
    /// this section and share officialness (officials are pinned, so a mixed
    /// group has no reachable winner). `None` paints nothing.
    pub(crate) fn conflicts_block(
        &self,
        game_id: &str,
        tab: SettingsModsTab,
        groups: &[SectionConflict],
        view: Entity<Self>,
        cx: &App,
    ) -> Option<AnyElement> {
        if groups.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .id(load_conflicts_id(tab))
                .gap_1()
                .child(widgets::muted(
                    self.strings.get("gui-section-load-conflicts"),
                    cx,
                ))
                .children(groups.iter().enumerate().map(|(gi, c)| {
                    let winner = c.instances.last().cloned().unwrap_or_default();
                    let mut wargs = FluentArgs::new();
                    wargs.set("instance", winner.clone());
                    let winner_text = self
                        .strings
                        .get_args("gui-load-conflict-winner", Some(&wargs));
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(widgets::muted(
                            format!("{} · {winner_text}", dest_short(&c.dest)),
                            cx,
                        ))
                        .children(c.instances.iter().filter(|i| **i != winner).map(|rival| {
                            let view = view.clone();
                            let game_id = game_id.to_string();
                            let rival = rival.clone();
                            let group = c.instances.clone();
                            widgets::btn(SharedString::from(format!("make-win-{gi}-{rival}")), cx)
                                .secondary()
                                .child(widgets::blabel(self.strings.get("gui-action-make-win"), cx))
                                .on_click(move |_, _, cx| {
                                    let view = view.clone();
                                    let game_id = game_id.clone();
                                    let rival = rival.clone();
                                    let group = group.clone();
                                    view.update(cx, |this, cx| {
                                        this.make_win_ui(&game_id, &rival, &group, cx);
                                    });
                                })
                        }))
                        .into_any_element()
                }))
                .into_any_element(),
        )
    }
}
