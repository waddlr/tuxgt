use gpui_kit::component::theme::ThemeMode;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

use super::super::theme::{brand, hsla_hex, overlay_ink, types, TypeStyled as _};

pub fn hairline_card(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let b = cx.theme();
    div()
        .id(id)
        .rounded(px(4.))
        .border_1()
        .border_color(b.border)
        .bg(b.group_box)
}

pub fn pill(
    label: impl Into<SharedString>,
    well: Hsla,
    border: Hsla,
    cx: &App,
) -> impl IntoElement {
    let well = pill_shade(well, cx);
    div()
        .h(px(18.))
        .px_1p5()
        .rounded(px(2.))
        .border_1()
        .border_color(border)
        .bg(well)
        .text_color(overlay_ink(hsla_hex(well)).0)
        .tx(types(cx).label_sm)
        .flex()
        .items_center()
        .child(label.into())
}

/// E141: mode-aware pill wells. Dark mode pulls mid/light wells down to a
/// dark shade, light mode pulls mid/dark wells up — hue and saturation
/// untouched, so gray stays gray (deemphasis) and colors stay colored,
/// and every well pairs with its `overlay_ink` at high contrast.
pub(crate) fn pill_shade(well: Hsla, cx: &App) -> Hsla {
    pill_shade_for(well, matches!(cx.theme().mode, ThemeMode::Dark))
}

/// Pure shade rule behind `pill_shade` (E141 test target).
fn pill_shade_for(well: Hsla, dark: bool) -> Hsla {
    let target = if dark {
        // Upper edge 0.70: silver (0.65) must darken — it reads light
        // ink off overlay_ink but pairs under 4.5. Warning (0.69) darkens
        // with it; platinum (0.76) already pairs dark ink at ~9:1.
        if well.l > 0.10 && well.l < 0.70 {
            Some(0.13)
        } else {
            None
        }
    } else if well.l > 0.30 && well.l < 0.92 {
        Some(0.87)
    } else {
        None
    };
    match target {
        Some(l) => Hsla {
            h: well.h,
            s: well.s,
            l,
            a: well.a,
        },
        None => well,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::theme::{contrast_ratio, hsla_hex, overlay_ink};
    use super::pill_shade_for;
    use gpui_kit::{black, rgb, white, Hsla};

    /// E141: mid wells normalize into the mode shade band with hue
    /// intact; settled dark/light wells pass through untouched.
    /// T25: the shade targets are bands, not pins — palette tweaks must
    /// not edit tests; the contrast test below is the real invariant.
    #[test]
    fn shade_rule_by_mode() {
        let gold: Hsla = rgb(0xe0ae4a).into();
        let dark_gold = pill_shade_for(gold, true);
        assert_eq!(
            (dark_gold.h, dark_gold.s, dark_gold.a),
            (gold.h, gold.s, gold.a)
        );
        assert!(
            (0.08..=0.20).contains(&dark_gold.l),
            "dark shade band, got {}",
            dark_gold.l
        );
        let danger: Hsla = rgb(0xc45c68).into();
        let light_danger = pill_shade_for(danger, false);
        assert!(
            (0.80..=0.92).contains(&light_danger.l),
            "light shade band, got {}",
            light_danger.l
        );
        // Settled wells pass through: near-black in dark, near-white in light.
        assert_eq!(pill_shade_for(black().into(), true), black().into());
        assert_eq!(pill_shade_for(white().into(), false), white().into());
        // Band edges stay put; just inside remaps into the dark band.
        let edge: Hsla = Hsla {
            h: 0.1,
            s: 0.5,
            l: 0.10,
            a: 1.0,
        };
        assert_eq!(pill_shade_for(edge, true).l, 0.10);
        let inner: Hsla = Hsla {
            h: 0.1,
            s: 0.5,
            l: 0.11,
            a: 1.0,
        };
        assert!(
            (0.08..=0.20).contains(&pill_shade_for(inner, true).l),
            "inner edge remaps into the dark shade band"
        );
    }

    /// E141 invariant: normalized wells pair with their `overlay_ink`
    /// at 4.5+ in both modes.
    #[test]
    fn normalized_wells_pair_with_their_ink() {
        for (well, dark) in [
            (0xe0ae4a, true),
            (0x6a8ea3, true),
            (0xc45c68, true),
            (0x8b939e, true),
            (0xa6a6a6, true),
            (0xe5c07b, true),
            (0xc45c68, false),
            (0x8b939e, false),
        ] {
            let shaded = pill_shade_for(rgb(well).into(), dark);
            let ink = hsla_hex(overlay_ink(hsla_hex(shaded)).0);
            assert!(
                contrast_ratio(hsla_hex(shaded), ink) >= 4.5,
                "well {well:06x} dark={dark}"
            );
        }
    }
}

pub fn accent_stripe(color: Hsla) -> impl IntoElement {
    div().w(px(3.)).h_full().bg(color).flex_shrink_0()
}

pub fn section_title(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div().tx(types(cx).headline_md).child(text.into())
}

pub fn muted(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .tx(types(cx).body_md)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn mono(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .tx(types(cx).label_lg)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn protondb_color(tier: &str, cx: &App) -> Hsla {
    let b = brand(cx);
    match tier.to_ascii_lowercase().as_str() {
        "platinum" => b.platinum,
        "gold" => b.gold,
        "silver" => b.silver,
        "bronze" => b.bronze,
        "borked" => cx.theme().danger,
        "native" => cx.theme().success,
        _ => cx.theme().muted_foreground,
    }
}

/// Accent stripe for one Mod card.
pub fn mod_stripe(mod_type: &str, cx: &App) -> Hsla {
    match mod_type {
        "reshade" | "reshade_addon" | "effect" | "texture" => brand(cx).mauve,
        "optiscaler" => cx.theme().success,
        _ => cx.theme().primary,
    }
}
