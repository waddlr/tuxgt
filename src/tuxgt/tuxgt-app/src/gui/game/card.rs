use std::collections::{HashMap, HashSet};

use gpui_kit::component::accordion::Accordion;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, StageState};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{ModRow, Shell};

impl Shell {
    pub(crate) fn mod_card(
        &self,
        game_id: &str,
        row: &ModRow,
        stage: &[&super::StageRow],
        can_up: bool,
        can_down: bool,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let t = types(cx);
        let stripe = widgets::mod_stripe(&row.mod_type, cx);
        let game_id = game_id.to_string();
        let inst = row.instance.clone();
        // R50: the graph line below renders only for missing requires or slot
        // conflicts, so it is always the warning color when present.
        // R48: Mode names the injection mechanism.
        let mode_name = match row.adapter.as_str() {
            "preload" => self.strings.get("gui-adapter-mode-preload"),
            "install" => self.strings.get("gui-adapter-mode-install"),
            _ => row.adapter.clone(),
        };
        let mut mode_args = FluentArgs::new();
        mode_args.set("name", mode_name);
        let mut file_args = FluentArgs::new();
        file_args.set("count", row.files.to_string());
        let mode_pill = self.strings.get_args("gui-mod-mode", Some(&mode_args));
        // E90: the file count is the dest Accordion title, not a header chip.
        let files_title = self.strings.get_args("gui-chip-files", Some(&file_args));
        h_flex()
            .id(row.ids.card.clone())
            .overflow_hidden()
            .rounded(px(4.))
            .border_1()
            .border_color(b.border)
            .bg(b.group_box)
            .child(widgets::accent_stripe(stripe))
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
                                Switch::new(row.ids.switch.clone())
                                    .w(px(28.))
                                    .checked(row.enabled)
                                    .xsmall()
                                    .on_click({
                                        let view = view.clone();
                                        let game_id = game_id.clone();
                                        let inst = inst.clone();
                                        move |on, _, cx| {
                                            let on = *on;
                                            view.update(cx, |this, cx| {
                                                this.toggle_mod(&game_id, &inst, on, false, cx);
                                            });
                                        }
                                    }),
                            )
                            .child({
                                // The section header names the type, so the
                                // type pill is gone; its help rides the name.
                                let help = match row.mod_type.as_str() {
                                    "reshade" => self.strings.get("gui-mod-help-reshade"),
                                    "optiscaler" => self.strings.get("gui-mod-help-optiscaler"),
                                    "reshade_addon" | "effect" | "texture" => {
                                        self.strings.get("gui-mod-help-pack")
                                    }
                                    _ => self.strings.get("gui-mod-help-custom"),
                                };
                                div()
                                    .id(row.ids.name.clone())
                                    .min_w_0()
                                    .tx(t.headline_md)
                                    .truncate()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(help.clone()).build(window, cx)
                                    })
                                    .child(row.label.clone())
                            })
                            .child(widgets::pill(
                                mode_pill,
                                cx.theme().foreground,
                                b.border,
                                cx,
                            ))
                            .child(div().flex_1())
                            .child(Self::move_button(
                                row.ids.move_up.clone(),
                                IconName::ArrowUp,
                                self.strings.get("gui-action-move-up"),
                                !can_up,
                                &view,
                                &game_id,
                                &inst,
                                -1,
                                cx,
                            ))
                            .child(Self::move_button(
                                row.ids.move_down.clone(),
                                IconName::ArrowDown,
                                self.strings.get("gui-action-move-down"),
                                !can_down,
                                &view,
                                &game_id,
                                &inst,
                                1,
                                cx,
                            ))
                            .child(widgets::destroy_btn(
                                row.ids.uninstall.clone(),
                                self.strings.get("gui-action-uninstall"),
                                {
                                    let view = view.clone();
                                    let game_id = game_id.clone();
                                    let inst = inst.clone();
                                    move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.uninstall_mod_ui(&game_id, &inst, false, cx);
                                        });
                                    }
                                },
                                cx,
                            )),
                    )
                    .when(
                        row.installed && super::slot_capable(&row.mod_type),
                        |this| {
                            this.child(widgets::labeled_row(
                                row.ids.slot_row.clone(),
                                self.strings.get("gui-add-slot"),
                                None,
                                widgets::value_btn(
                                    row.ids.slot.clone(),
                                    row.slot.clone(),
                                    {
                                        let view = view.clone();
                                        let game_id = game_id.clone();
                                        let inst = inst.clone();
                                        move |menu, _, _| {
                                            let mut menu = menu;
                                            for s in Shell::ADD_SLOTS {
                                                menu = menu.item(PopupMenuItem::new(s).on_click({
                                                    let view = view.clone();
                                                    let game_id = game_id.clone();
                                                    let inst = inst.clone();
                                                    move |_, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            this.slot_change_ui(
                                                                &game_id, &inst, s, false, cx,
                                                            );
                                                        });
                                                    }
                                                }));
                                            }
                                            menu
                                        }
                                    },
                                    cx,
                                ),
                                cx,
                            ))
                        },
                    )
                    .when(!row.graph.is_empty(), |this| {
                        this.child(
                            div()
                                .tx(t.body_md)
                                .text_color(b.warning)
                                .child(row.graph.clone()),
                        )
                    })
                    .child(self.update_row(&game_id, &inst, view.clone(), cx))
                    .child({
                        // E90: dest rows (Load DLLs + Include Files) and env keep
                        // rows share one kit Accordion per card, default closed.
                        // Controlled: expansion lives in `mod_files_open` (the
                        // kit only notifies item indices). The title is the
                        // file count; per-card Force re-sync stays below.
                        let files_key = row.ids.files_key.clone();
                        let files_open = files_accordion_open(&self.mod_files_open, &files_key);
                        Accordion::new(row.ids.files_id.clone())
                            .item(|item| {
                                // Collapsed cards paint header only: `keep_rows`
                                // builds thousands of elements otherwise.
                                let item = item
                                    .title(widgets::mono(files_title.clone(), cx))
                                    .open(files_open);
                                if !files_open {
                                    return item;
                                }
                                let index: HashMap<&str, StageState> =
                                    stage.iter().map(|r| (r.file.as_str(), r.state)).collect();
                                // E99: Effects/Files previews are disclosures
                                // inside the accordion, never card-header
                                // buttons.
                                let (preview_buttons, preview_bodies) = self.preview_controls(
                                    &inst,
                                    "card",
                                    &row.mod_type,
                                    &row.effect_files,
                                    row.asset.as_deref(),
                                    row.payload_present,
                                    view.clone(),
                                    cx,
                                    false,
                                );
                                item.child(
                                    v_flex()
                                        .gap_1()
                                        .child(self.keep_rows(
                                            &game_id,
                                            &inst,
                                            &row.file_entries,
                                            &row.env_entries,
                                            &index,
                                            view.clone(),
                                            cx,
                                        ))
                                        .when(!preview_buttons.is_empty(), |this| {
                                            this.child(
                                                h_flex()
                                                    .flex_wrap()
                                                    .gap_1()
                                                    .children(preview_buttons),
                                            )
                                        })
                                        .children(preview_bodies)
                                        .child(self.stage_section(
                                            &game_id,
                                            &inst,
                                            view.clone(),
                                            cx,
                                        )),
                                )
                            })
                            .on_toggle_click({
                                let view = view.clone();
                                move |open: &[usize], _, cx| {
                                    view.update(cx, |this, cx| {
                                        apply_files_toggle(
                                            &mut this.mod_files_open,
                                            files_key.as_str().to_string(),
                                            open,
                                        );
                                        cx.notify();
                                    });
                                }
                            })
                    }),
            )
    }
}

pub(crate) fn files_accordion_key(game_id: &str, instance: &str) -> String {
    format!("{game_id}/{instance}")
}

/// E90 dest Accordion expansion: the single item (index 0) is open when its
/// key is in the set. Empty set = all collapsed (default closed).
pub(crate) fn files_accordion_open(open_cards: &HashSet<String>, key: &str) -> bool {
    open_cards.contains(key)
}

/// E90 dest Accordion toggle: the kit reports open item indices; with one
/// item per card, `[0]` = expanded, `[]` = collapsed.
pub(crate) fn apply_files_toggle(open_cards: &mut HashSet<String>, key: String, open: &[usize]) {
    if open.contains(&0) {
        open_cards.insert(key);
    } else {
        open_cards.remove(&key);
    }
}
