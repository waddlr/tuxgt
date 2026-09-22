use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{detect_ids, Shell};

pub(crate) fn detect_field_label(key: &str) -> Option<&'static str> {
    Some(match key {
        "platform" => "gui-detect-field-platform",
        "api" => "gui-detect-field-api",
        "extra_apis" => "gui-detect-field-extra-apis",
        "engine" => "gui-detect-field-engine",
        "bitness" => "gui-detect-field-bitness",
        "exe" => "gui-detect-field-exe",
        "prefix" => "gui-detect-field-prefix",
        "proton" => "gui-detect-field-proton",
        "build" => "gui-detect-field-build",
        "exe_version" => "gui-detect-field-exe-version",
        _ => return None,
    })
}

/// E60 value class for one detection field. `None` where the value is not a
/// display-map id (paths, proton builds, env, launch options).
pub(crate) fn detect_val_kind(key: &str) -> Option<widgets::ValKind> {
    Some(match key {
        "platform" => widgets::ValKind::Platform,
        "api" | "extra_apis" => widgets::ValKind::Api,
        "engine" => widgets::ValKind::Engine,
        "bitness" => widgets::ValKind::Bitness,
        _ => return None,
    })
}

/// E60: label a displayed snapshot value only when its field is a
/// display-map kind; everything else stays as stored.
pub(crate) fn map_detect_val(
    kind: Option<widgets::ValKind>,
    value: &str,
    strings: &tuxgt_core::Strings,
) -> String {
    match kind {
        Some(kind) => widgets::id_label(kind, value, strings),
        None => value.to_string(),
    }
}
impl Shell {
    pub(crate) fn detect_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let game_id = self.selected.clone().unwrap_or_default();
        widgets::section_card("detection", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-detection"),
                cx,
            ))
            .children(
                self.detect
                    .iter()
                    .map(|snap| self.detect_row(snap, game_id.as_str(), view.clone(), cx)),
            )
            .when(self.detect.is_empty(), |this| {
                this.child(widgets::placeholder_note(
                    self.strings.get("gui-empty-no-snapshot"),
                    cx,
                ))
            })
            .child(self.redetect_row(game_id.as_str(), view, cx))
    }

    pub(crate) fn detect_row(
        &self,
        snap: &tuxgt_core::DetectSnapshot,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let kind = detect_val_kind(snap.key);
        let effective = map_detect_val(kind, snap.effective().unwrap_or("—"), &self.strings);
        let source = if snap.override_.is_some() {
            self.strings.get("gui-source-override")
        } else if snap.detected.is_some() {
            self.strings.get("gui-source-detected")
        } else if snap.store.is_some() {
            self.strings.get("gui-source-store")
        } else {
            self.strings.get("gui-source-unset")
        };
        let title = detect_field_label(snap.key)
            .map(|k| self.strings.get(k))
            .unwrap_or_else(|| snap.key.to_string());
        let ids = detect_ids(snap.key);
        v_flex()
            .id(ids.row)
            .w_full()
            .gap_1()
            .px_1()
            .py_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .tx(types(cx).label_lg)
                            .min_w_0()
                            .truncate()
                            .child(title),
                    )
                    .child(div().flex_1())
                    .child(widgets::pill(
                        source,
                        cx.theme().muted_foreground,
                        b.border,
                        cx,
                    ))
                    .child(
                        self.detect_editor(snap, game_id, view, cx)
                            .into_any_element(),
                    ),
            )
            .child({
                let tip = SharedString::from(effective.clone());
                div()
                    .id(ids.eff)
                    .w_full()
                    .min_w_0()
                    .tx(types(cx).label_lg)
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(SharedString::from(effective))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            })
            .child(widgets::row_hairline(cx))
    }

    pub(crate) fn detect_editor(
        &self,
        snap: &tuxgt_core::DetectSnapshot,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        match snap.key {
            "platform" => self
                .detect_dropdown(
                    snap,
                    game_id,
                    &[
                        ("native", "gui-detect-val-native"),
                        ("proton", "gui-detect-val-proton"),
                        ("wine", "gui-detect-val-wine"),
                    ],
                    view,
                    cx,
                )
                .into_any_element(),
            "bitness" => self
                .detect_dropdown(
                    snap,
                    game_id,
                    &[("32", "gui-detect-val-32"), ("64", "gui-detect-val-64")],
                    view,
                    cx,
                )
                .into_any_element(),
            "api" => self
                .detect_dropdown(
                    snap,
                    game_id,
                    &[
                        ("dx9", "gui-detect-val-dx9"),
                        ("dx10", "gui-detect-val-dx10"),
                        ("dx11", "gui-detect-val-dx11"),
                        ("dx12", "gui-detect-val-dx12"),
                        ("vulkan", "gui-detect-val-vulkan"),
                        ("opengl", "gui-detect-val-opengl"),
                    ],
                    view,
                    cx,
                )
                .into_any_element(),
            "engine" => self
                .detect_dropdown(
                    snap,
                    game_id,
                    &[
                        ("unreal", "gui-detect-val-unreal"),
                        ("unity", "gui-detect-val-unity"),
                        ("re_engine", "gui-detect-val-re-engine"),
                        ("creation", "gui-detect-val-creation"),
                        ("blackspace", "gui-detect-val-blackspace"),
                    ],
                    view,
                    cx,
                )
                .into_any_element(),
            "exe" | "prefix" => self.detect_path(snap, game_id, view, cx).into_any_element(),
            _ => self.detect_text(snap, game_id, view, cx).into_any_element(),
        }
    }
}
