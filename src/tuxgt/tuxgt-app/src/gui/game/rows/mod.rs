mod update;

use std::collections::HashMap;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::StageState;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn keep_rows(
        &self,
        game_id: &str,
        instance: &str,
        rows: &[super::ModFileRow],
        env_rows: &[super::ModEnvRow],
        stage: &HashMap<&str, StageState>,
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
                "gui-mod-section-loaddll",
                view.clone(),
                cx,
            ))
            .child(self.file_section(
                game_id,
                instance,
                files.as_slice(),
                stage,
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
        title_id: &'static str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let out: Vec<AnyElement> = rows
            .iter()
            .map(|f| {
                self.file_row(game_id, instance, f, stage, view.clone(), cx)
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
    /// omitted or when no stage row exists yet).
    pub(crate) fn file_row(
        &self,
        game_id: &str,
        instance: &str,
        f: &super::ModFileRow,
        stage: &HashMap<&str, StageState>,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
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
        // Edit paints left of the sync pill.
        let pill: Option<AnyElement> = match (edit, stage_pill) {
            (None, None) => None,
            (e, p) => Some(h_flex().gap_1().items_center().children(e).children(p).into_any_element()),
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
