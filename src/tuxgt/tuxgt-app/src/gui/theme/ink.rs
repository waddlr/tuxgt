use gpui_kit::*;

/// E108: WCAG relative luminance of a packed `0xRRGGBB` hex.
pub(crate) fn relative_luma(hex: u32) -> f32 {
    let chan = |c: u32| {
        let s = (c as f32) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = chan((hex >> 16) & 0xff);
    let g = chan((hex >> 8) & 0xff);
    let b = chan(hex & 0xff);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Packed `0xRRGGBB` from an `Hsla` (chip well → `overlay_ink`).
pub(crate) fn hsla_hex(c: Hsla) -> u32 {
    let rgb = c.to_rgb();
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    (byte(rgb.r) << 16) | (byte(rgb.g) << 8) | byte(rgb.b)
}

/// E108: title ink + mods-line ink for a wash hex. A light wash gets dark
/// ink, a dark wash light ink; the mods line is the same ink at 0.70 / 0.78.
pub(crate) fn overlay_ink(hex: u32) -> (Hsla, Hsla) {
    let (title, sub_a) = if relative_luma(hex) > 0.42 {
        (0x1c2128, 0.70)
    } else {
        (0xf4f6f8, 0.78)
    };
    let t: Hsla = rgb(title).into();
    (t, t.opacity(sub_a))
}

/// E140: WCAG contrast ratio between two packed hexes. Pill wells must
/// pair with their `overlay_ink` at 4.5+ — mid-tone wells (e.g. a text
/// color used as background) fail with either ink, so wells stay clearly
/// dark or clearly light. Test-only: no paint path calls it.
#[cfg(test)]
pub(crate) fn contrast_ratio(a_hex: u32, b_hex: u32) -> f32 {
    let (la, lb) = (relative_luma(a_hex), relative_luma(b_hex));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::{contrast_ratio, hsla_hex, overlay_ink, relative_luma};
    use gpui_kit::{rgb, Hsla};

    #[test]
    fn light_wash_takes_dark_ink() {
        assert!(relative_luma(0xe8d44d) > 0.42);
        let (title, sub) = overlay_ink(0xe8d44d);
        assert_eq!(title, rgb(0x1c2128).into());
        assert_eq!(sub.a, 0.70);
    }

    #[test]
    fn dark_wash_takes_light_ink() {
        assert!(relative_luma(0x0e1014) <= 0.42);
        assert_eq!(overlay_ink(0x0e1014).0, rgb(0xf4f6f8).into());
        assert_eq!(overlay_ink(0x1c1f26).0, rgb(0xf4f6f8).into());
        assert_eq!(overlay_ink(0x1c1f26).1.a, 0.78);
    }

    #[test]
    fn hsla_hex_round_trip() {
        assert_eq!(hsla_hex(rgb(0xe0ae4a).into()), 0xe0ae4a);
        let yellow: Hsla = rgb(0xe8d44d).into();
        assert_eq!(overlay_ink(hsla_hex(yellow)).0, rgb(0x1c2128).into());
    }

    /// E140: the fixed pill wells pair with their `overlay_ink` at 4.5+ —
    /// dark wells take light ink, light wells dark ink.
    #[test]
    fn pill_wells_pair_with_their_ink() {
        for well in [0x050608, 0x0b0c0f, 0xe0ae4a, 0xe5c07b] {
            let ink = hsla_hex(overlay_ink(well).0);
            assert!(
                contrast_ratio(well, ink) >= 4.5,
                "well {well:06x} ink {ink:06x}"
            );
        }
    }

    /// E140 lesson: `0x8b939e` (`muted_foreground`) as a well is unreadable
    /// because `overlay_ink` picks light ink for it (~2.8:1). Mid-tones sit
    /// on the wrong side of the threshold, so they are never wells.
    #[test]
    fn mid_tone_well_picks_unreadable_ink() {
        let well = 0x8b939e;
        let ink = hsla_hex(overlay_ink(well).0);
        assert_eq!(ink, 0xf4f6f8);
        assert!(contrast_ratio(well, ink) < 4.5);
    }
}
