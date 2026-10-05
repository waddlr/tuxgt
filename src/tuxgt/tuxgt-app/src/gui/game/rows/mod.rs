mod update;

use std::collections::HashMap;

use gpui_kit::component::button::Toggle;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{FluentArgs, StageState};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;
use super::DestConflict;

impl Shell {
    pub(crate) fn keep_rows(
        &self,
        game_id: &str,
        instance: &str,
        rows: &[super::ModFileRow],
        env_rows: &[super::ModEnvRow],
        stage: &HashMap<&str, StageState>,
        marks: &[DestConflict],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let (dlls, files): (Vec<_>, Vec<_>) = rows.iter().partition(|f| f.loaddll);
        v_flex()
            .gap_1()
            .child(self.file_section(
                game_id,
                instance,
                dlls.as_slice(),
                stage,
                marks,
                "gui-mod-section-loaddll",
                view.clone(),
                cx,
            ))
            .child(self.file_section(
                game_id,
                instance,
                files.as_slice(),
                stage,
                marks,
                "gui-mod-section-include",
                view.clone(),
                cx,
            ))
            .child(self.env_section(game_id, instance, env_rows, view.clone(), cx))
    }

    /// One R47 file section: shared title token + merged rows, or muted
    /// `none` when the section has no dests.
    pub(crate) fn file_section(
        &self,
        game_id: &str,
        instance: &str,
        rows: &[&super::ModFileRow],
        stage: &HashMap<&str, StageState>,
        marks: &[DestConflict],
        title_id: &'static str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let out: Vec<AnyElement> = rows
            .iter()
            .map(|f| {
                self.file_row(game_id, instance, f, stage, marks, view.clone(), cx)
                    .into_any_element()
            })
            .collect();
        // `gui.mod-config-edit`: the editor is a full-page takeover at the
        // Mods tab root (`mods_tab`), never an inline card in this list.
        self.keep_section(title_id, rows.is_empty(), out.into_iter(), cx)
    }

    /// Shared `title + none-note + mapped rows` skeleton for the file and
    /// env keep sections.
    fn keep_section<R>(
        &self,
        title_id: &'static str,
        empty: bool,
        rows: impl IntoIterator<Item = R>,
        cx: &App,
    ) -> Div
    where
        R: IntoElement,
    {
        v_flex()
            .gap_1()
            .child(widgets::section_title(self.strings.get(title_id), cx))
            .when(empty, |this| {
                this.child(widgets::muted(self.strings.get("gui-mod-section-none"), cx))
            })
            .children(rows)
    }

    /// E78 merged dest row, left → right: keep checkbox, mono
    /// `source-base → dest` mapping, sync pill for kept dests (no pill when
    /// omitted or when no stage row exists yet). A contested dest paints a
    /// conflict marker first in the pill cluster: warning-tinted when this
    /// card loses the file, success-tinted when it wins it.
    pub(crate) fn file_row(
        &self,
        game_id: &str,
        instance: &str,
        f: &super::ModFileRow,
        stage: &HashMap<&str, StageState>,
        marks: &[DestConflict],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let conflict: Option<AnyElement> = marks.iter().find(|m| m.dest == f.dest).map(|m| {
            let mut lines = Vec::new();
            if !m.loses_to.is_empty() {
                let mut args = FluentArgs::new();
                args.set("mods", m.loses_to.join(", "));
                lines.push(self.strings.get_args("gui-conflict-tip-loses", Some(&args)));
            }
            if !m.wins_over.is_empty() {
                let mut args = FluentArgs::new();
                args.set("mods", m.wins_over.join(", "));
                lines.push(self.strings.get_args("gui-conflict-tip-wins", Some(&args)));
            }
            let tip = lines.join("\n");
            div()
                .id(SharedString::from(format!("cm-{instance}-{}", f.dest)))
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .small()
                        .text_color(if m.winning { b.success } else { b.warning }),
                )
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .into_any_element()
        });
        let stage_pill: Option<AnyElement> = if f.enabled {
            stage.get(f.dest.as_str()).map(|s| {
                let (label, fg, tip) = match s {
                    StageState::InSync => (
                        self.strings.get("gui-mod-stage-insync"),
                        cx.theme().muted_foreground,
                        None,
                    ),
                    StageState::UserModified => (
                        self.strings.get("gui-mod-stage-outsync"),
                        b.warning,
                        Some(self.strings.get("gui-mod-stage-modified")),
                    ),
                    StageState::DepotNewer => (
                        self.strings.get("gui-mod-stage-outsync"),
                        b.warning,
                        Some(self.strings.get("gui-mod-stage-depot")),
                    ),
                    StageState::Unmanaged => (
                        self.strings.get("gui-mod-stage-outsync"),
                        b.warning,
                        Some(self.strings.get("gui-mod-stage-unmanaged")),
                    ),
                };
                let pill = div()
                    .id(SharedString::from(format!("sp-{}-{}", instance, f.dest)))
                    .child(widgets::pill(label, fg, b.border, cx));
                match tip {
                    Some(reason) => pill
                        .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                        .into_any_element(),
                    None => pill.into_any_element(),
                }
            })
        } else {
            None
        };
        let edit = self.staged_edit_button(game_id, instance, &f.dest, f.enabled, view.clone(), cx);
        let mode = self.file_mode_toggle(game_id, instance, f, view.clone());
        // Conflict marker, then Mode, Edit, and the sync pill.
        let pill: Option<AnyElement> =
            if conflict.is_none() && mode.is_none() && edit.is_none() && stage_pill.is_none() {
                None
            } else {
                Some(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .children(conflict)
                        .children(mode)
                        .children(edit)
                        .children(stage_pill)
                        .into_any_element(),
                )
            };
        let lock_tip = f
            .required
            .then(|| self.strings.get("gui-tip-file-required"));
        let stem = format!("{instance}-{}", f.dest);
        let on_toggle = {
            let game = game_id.to_string();
            let instance = instance.to_string();
            let dest = f.dest.clone();
            move |this: &mut Shell, on: bool, cx: &mut Context<Shell>| {
                this.toggle_file(&game, &instance, &dest, on, false, cx);
            }
        };
        self.keep_row(
            "f",
            &stem,
            f.enabled,
            f.required,
            lock_tip,
            super::file_mapping_label(&f.source, &f.dest),
            pill,
            view,
            on_toggle,
            cx,
        )
    }

    /// Shared keep-checkbox row skeleton for file rows (sync pill, required
    /// lock) and env rows (neither): 24px checkbox + flex-1 truncated
    /// clickable label + optional pill + hairline. `disabled` locks the
    /// checkbox and drops the label click; `lock_tip` is the lock tooltip.
    #[allow(clippy::too_many_arguments)]
    fn keep_row<F>(
        &self,
        id_prefix: &str,
        id_stem: &str,
        checked: bool,
        disabled: bool,
        lock_tip: Option<String>,
        label: String,
        pill: Option<AnyElement>,
        view: Entity<Self>,
        on_toggle: F,
        cx: &App,
    ) -> Div
    where
        F: Fn(&mut Shell, bool, &mut Context<Shell>) + Clone + 'static,
    {
        let t = types(cx);
        let click_on = !checked;
        let mut check = Checkbox::new(SharedString::from(format!("{id_prefix}ks-{id_stem}")))
            .checked(checked)
            .disabled(disabled)
            .on_change({
                let view = view.clone();
                let toggle = on_toggle.clone();
                move |on, _, cx| {
                    let on = *on;
                    view.update(cx, |this, cx| {
                        toggle(this, on, cx);
                    });
                }
            });
        if let Some(tip) = lock_tip {
            check = check.tooltip(tip);
        }
        let mut row = h_flex()
            .id(SharedString::from(format!("{id_prefix}k-{id_stem}")))
            .gap_2()
            .items_center()
            .py_1()
            .child(div().w(px(24.)).flex_none().child(check))
            .child(
                div()
                    .id(SharedString::from(format!("{id_prefix}l-{id_stem}")))
                    .flex_1()
                    .min_w_0()
                    .tx(t.label_lg)
                    .text_color(cx.theme().muted_foreground)
                    .when(!disabled, |this| {
                        let view = view.clone();
                        let toggle = on_toggle.clone();
                        this.cursor_pointer().on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                toggle(this, click_on, cx);
                            });
                        })
                    })
                    .truncate()
                    .child(label),
            );
        if let Some(pill) = pill {
            row = row.child(pill);
        }
        v_flex().child(row).child(widgets::row_hairline(cx))
    }

    /// Load / Include for one applicable `.dll`. Absent for other dests.
    /// On is LoadDLL. The row moves section on the next paint.
    fn file_mode_toggle(
        &self,
        game_id: &str,
        instance: &str,
        f: &super::ModFileRow,
        view: Entity<Self>,
    ) -> Option<AnyElement> {
        if !tuxgt_core::file_mode_applicable(&f.dest) {
            return None;
        }
        let game = game_id.to_string();
        let instance = instance.to_string();
        let dest = f.dest.clone();
        let load_label = self.strings.get("gui-mode-load");
        let load_tip = self.strings.get("gui-mode-loaddll");
        Some(
            div()
                .w(px(72.))
                .flex_none()
                .child(
                    Toggle::new(SharedString::from(format!("fm-{}-{}", instance, f.dest)))
                        .label(load_label)
                        .tooltip(load_tip)
                        .xsmall()
                        .checked(f.loaddll)
                        .on_click(move |next, _, cx| {
                            cx.stop_propagation();
                            let load = *next;
                            view.update(cx, |this, cx| {
                                this.set_file_load_ui(&game, &instance, &dest, load, cx);
                            });
                        }),
                )
                .into_any_element(),
        )
    }

    /// E74 env rows: `KEY=VALUE` + keep checkbox, no sync pill, no lock.
    pub(crate) fn env_section(
        &self,
        game_id: &str,
        instance: &str,
        rows: &[super::ModEnvRow],
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        self.keep_section(
            "gui-mod-section-env",
            rows.is_empty(),
            rows.iter()
                .map(|e| self.env_row(game_id, instance, e, view.clone(), cx)),
            cx,
        )
    }

    pub(crate) fn env_row(
        &self,
        game_id: &str,
        instance: &str,
        e: &super::ModEnvRow,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let stem = format!("{instance}-{}", e.key);
        let on_toggle = {
            let game = game_id.to_string();
            let instance = instance.to_string();
            let key = e.key.clone();
            move |this: &mut Shell, on: bool, cx: &mut Context<Shell>| {
                this.toggle_env(&game, &instance, &key, on, cx);
            }
        };
        self.keep_row(
            "e",
            &stem,
            e.enabled,
            false,
            None,
            format!("{}={}", e.key, e.value),
            None,
            view,
            on_toggle,
            cx,
        )
    }
}
