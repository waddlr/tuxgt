use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::super::theme::{types, TypeStyled as _};
use super::*;

pub fn chain_inner(
    handle: ScrollHandle,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
    move |event, window, cx| {
        let delta = event.delta.pixel_delta(window.line_height());
        if chain_room(handle.offset().y, handle.max_offset().y, delta.y, delta.x) {
            cx.stop_propagation();
        }
    }
}

/// Pure room check behind `chain_inner`: effective vertical intent is `dy`,
/// falling back to `dx` (shift-wheel reports on x; vertical-only boxes remap
/// it, same as the toolkit's own scroll listener).
pub fn chain_room(offset_y: Pixels, max_y: Pixels, dy: Pixels, dx: Pixels) -> bool {
    let v = if dy.is_zero() { dx } else { dy };
    if v.is_zero() || max_y <= px(0.) {
        return false;
    }
    if v < px(0.) {
        offset_y > -max_y
    } else {
        offset_y < px(0.)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValKind {
    Manager,
    Store,
    Platform,
    Api,
    Bitness,
    Engine,
    Tier,
    ModType,
    // R37: painted only by the gated `adapter_choice_row` (game/launch.rs).
    #[allow(dead_code)]
    Adapter,
}

/// `(stored id, Fluent key)` per kind. `gui-detect-val-*` **are** the
/// platform/api/bitness/engine labels; the other kinds have their own keys.
/// The id is what the filter/sqlx layer stores; the key is what is painted.
pub(crate) fn val_table(kind: ValKind) -> &'static [(&'static str, &'static str)] {
    let table: &[(&str, &str)] = match kind {
        ValKind::Manager => &[
            ("steam", "plugin-steam-label"),
            ("heroic", "plugin-heroic-label"),
            ("manual", "plugin-manual-label"),
        ],
        ValKind::Store => &[
            ("gog", "gui-store-gog"),
            ("epic", "gui-store-epic"),
            ("amazon", "gui-store-amazon"),
            ("standalone", "gui-store-standalone"),
        ],
        ValKind::Platform => &[
            ("native", "gui-detect-val-native"),
            ("proton", "gui-detect-val-proton"),
            ("wine", "gui-detect-val-wine"),
        ],
        ValKind::Api => &[
            ("dx9", "gui-detect-val-dx9"),
            ("dx10", "gui-detect-val-dx10"),
            ("dx11", "gui-detect-val-dx11"),
            ("dx12", "gui-detect-val-dx12"),
            ("vulkan", "gui-detect-val-vulkan"),
            ("opengl", "gui-detect-val-opengl"),
        ],
        ValKind::Bitness => &[("32", "gui-detect-val-32"), ("64", "gui-detect-val-64")],
        ValKind::Engine => &[
            ("unreal", "gui-detect-val-unreal"),
            ("unity", "gui-detect-val-unity"),
            ("re_engine", "gui-detect-val-re-engine"),
            ("creation", "gui-detect-val-creation"),
            ("blackspace", "gui-detect-val-blackspace"),
            ("mo2", "gui-detect-val-mo2"),
        ],
        ValKind::Tier => &[
            ("platinum", "gui-tier-platinum"),
            ("gold", "gui-tier-gold"),
            ("silver", "gui-tier-silver"),
            ("bronze", "gui-tier-bronze"),
            ("borked", "gui-tier-borked"),
            ("native", "gui-detect-val-native"),
        ],
        ValKind::ModType => &[
            ("reshade", "gui-modtype-reshade"),
            ("reshade_addon", "gui-modtype-reshade-addon"),
            ("optiscaler", "gui-modtype-optiscaler"),
            ("custom", "gui-modtype-custom"),
            ("effect", "gui-modtype-effect"),
            ("texture", "gui-modtype-texture"),
        ],
        ValKind::Adapter => &[
            ("preload", "gui-adapter-preload"),
            ("install", "gui-adapter-install"),
            ("proton_env", "gui-adapter-proton-env"),
        ],
    };
    table
}

pub(crate) fn val_key(kind: ValKind, id: &str) -> Option<&'static str> {
    val_table(kind)
        .iter()
        .find(|(value, _)| *value == id)
        .map(|(_, key)| *key)
}

/// Display label for one stored id. Unknown id → the stored string, never
/// Title Case (`overview.md` Strings).
pub fn id_label(kind: ValKind, id: &str, strings: &tuxgt_core::Strings) -> String {
    match val_key(kind, id) {
        Some(key) => strings.get(key),
        None => id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{chain_room, id_label, val_table, ValKind};
    use gpui_kit::px;

    /// E60: every map entry paints the catalog label, never the key or the
    /// stored id; an id outside the map stays as stored.
    #[test]
    fn display_map_resolves_to_labels() {
        let s = tuxgt_core::Strings::en_us().expect("catalog");
        for kind in [
            ValKind::Manager,
            ValKind::Store,
            ValKind::Platform,
            ValKind::Bitness,
            ValKind::Engine,
            ValKind::Tier,
            ValKind::ModType,
            ValKind::Adapter,
        ] {
            for (id, key) in val_table(kind) {
                let label = id_label(kind, id, &s);
                assert_ne!(label, *key, "{key} missing from the catalog");
                assert_ne!(label, *id, "{id} is not relabeled by {key}");
            }
        }
        assert_eq!(id_label(ValKind::Manager, "steam", &s), "Steam");
        assert_eq!(id_label(ValKind::Api, "dx12", &s), "DX12");
        assert_eq!(id_label(ValKind::Engine, "re_engine", &s), "RE Engine");
        assert_eq!(id_label(ValKind::Bitness, "64", &s), "64-bit");
        assert_eq!(id_label(ValKind::ModType, "custom", &s), "Custom Mod");
        assert_eq!(id_label(ValKind::Store, "gog", &s), "GOG");
        assert_eq!(id_label(ValKind::Adapter, "proton_env", &s), "ProtonEnv");
        assert_eq!(id_label(ValKind::Tier, "borked", &s), "Borked");
        assert_eq!(id_label(ValKind::Api, "metal", &s), "metal");
    }

    #[test]
    fn chain_room_cases() {
        // No overflow: never room, either direction.
        assert!(!chain_room(px(0.), px(0.), px(-10.), px(0.)));
        assert!(!chain_room(px(0.), px(0.), px(10.), px(0.)));
        // Zero delta: nothing to claim.
        assert!(!chain_room(px(-5.), px(100.), px(0.), px(0.)));
        // At top: room downward only.
        assert!(chain_room(px(0.), px(100.), px(-10.), px(0.)));
        assert!(!chain_room(px(0.), px(100.), px(10.), px(0.)));
        // At bottom: room upward only.
        assert!(!chain_room(px(-100.), px(100.), px(-10.), px(0.)));
        assert!(chain_room(px(-100.), px(100.), px(10.), px(0.)));
        // Mid-list: room both ways.
        assert!(chain_room(px(-40.), px(100.), px(-10.), px(0.)));
        assert!(chain_room(px(-40.), px(100.), px(10.), px(0.)));
        // Shift-wheel reports on x: remapped to vertical like the toolkit.
        assert!(chain_room(px(0.), px(100.), px(0.), px(-10.)));
        assert!(!chain_room(px(0.), px(100.), px(0.), px(10.)));
        assert!(chain_room(px(-100.), px(100.), px(0.), px(10.)));
    }
}
/// Wrapping columns: an `h_flex` row of weighted cells that wrap when a
/// cell would shrink below `min_w` (N equal cells wrap below ~N*`min_w` +
/// gaps). `grow` is flex-grow per cell (60/40 → 6 and 4); `stretch`
/// cross-aligns stretched instead of start.
pub fn cols(
    id: &'static str,
    min_w: f32,
    stretch: bool,
    cells: Vec<(i32, AnyElement)>,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .when(stretch, |this| this.items_stretch())
        .when(!stretch, |this| this.items_start())
        .gap_3()
        .flex_wrap()
        .children(cells.into_iter().map(|(grow, child)| {
            v_flex()
                .flex_grow(grow as f32)
                .flex_shrink(1.)
                .flex_basis(px(0.))
                .min_w(px(min_w))
                .min_h_0()
                .child(child)
        }))
}

/// Open a path's location: directories as-is, files via their parent.
/// Never exec a PE.
pub fn open_location(path: &str) {
    let p = std::path::Path::new(path);
    let target = if p.is_file() {
        p.parent()
            .map(|d| d.as_os_str())
            .unwrap_or_else(|| p.as_os_str())
    } else {
        p.as_os_str()
    };
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
}

/// Property row: label (and optional help) left, control last at the far
/// right — `justify_between` across the row, label column capped and
/// truncated.
pub fn labeled_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    help: Option<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .gap_2()
        .items_center()
        .justify_between()
        .px_1()
        .py_1()
        .child(
            v_flex()
                .min_w_0()
                .max_w(px(420.))
                .child(div().tx(types(cx).label_lg).truncate().child(label.into()))
                .when_some(help, |this, h| this.child(muted(h, cx))),
        )
        .child(control)
}

/// Secondary Copy control — same height as Open. Callers attach `on_click`.
pub fn copy_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, cx: &App) -> Button {
    btn(id, cx)
        .secondary()
        .child(bicon(IconName::Copy))
        .child(blabel(label, cx))
}

pub fn row_hairline(cx: &App) -> impl IntoElement {
    div().w_full().h(px(1.)).bg(cx.theme().border)
}
