use gpui_kit::*;

use super::id::ThemeId;
use super::palettes::{hx, pal, Pal};

/// Roles kit `ThemeColor` does not have. Chrome colors live on `cx.theme()`.
#[derive(Clone, Copy, Debug)]
pub struct Brand {
    pub mauve: Hsla,
    pub platinum: Hsla,
    pub gold: Hsla,
    pub silver: Hsla,
    pub bronze: Hsla,
    pub wash: Hsla,
}

impl Brand {
    pub(crate) fn from_pal(p: &Pal) -> Self {
        Self {
            mauve: hx(p.mauve),
            platinum: hx(0xb5c0d0),
            gold: hx(p.gold),
            silver: hx(0xa6a6a6),
            bronze: hx(0xcd7f32),
            wash: hx(p.wash),
        }
    }
}

impl Global for Brand {}

pub fn brand(cx: &App) -> Brand {
    if cx.has_global::<Brand>() {
        *cx.global::<Brand>()
    } else {
        Brand::from_pal(&pal(ThemeId::TuxDark))
    }
}

#[cfg(test)]
mod tests {
    use super::super::id::ThemeId;
    use super::super::ink::hsla_hex;
    use super::super::palettes::pal;
    use super::Brand;

    #[test]
    fn from_pal_matches_hex_table() {
        let p = pal(ThemeId::TuxDark);
        let b = Brand::from_pal(&p);
        assert_eq!(hsla_hex(b.wash), p.wash);
        assert_eq!(hsla_hex(b.gold), p.gold);
    }
}
