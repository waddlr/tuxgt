use gpui_kit::*;

use super::id::FontScale;

/// GUI.md §3 resolved type token: absolute px size + line height.
///
/// Weight is part of the token (headlines 600, body 400, labels 500);
/// families are always the system fonts (theme UI font, probed mono).
/// GPUI exposes no letter-spacing API, so the table's tracking column
/// has no runtime effect.
#[derive(Clone, Copy, Debug)]
pub struct TypeToken {
    pub size: Pixels,
    pub line: Pixels,
    pub weight: FontWeight,
    pub mono: bool,
}

/// GUI.md §3 resolved type scale: six tokens plus control/row heights.
/// Shipped default is Default (this ladder). Compact/Large stay selectable.
#[derive(Clone, Copy, Debug)]
pub struct TypeTokens {
    pub headline_lg: TypeToken,
    pub headline_md: TypeToken,
    pub headline_sm: TypeToken,
    pub body_md: TypeToken,
    pub label_lg: TypeToken,
    pub label_sm: TypeToken,
    pub control_h: Pixels,
    pub row_h: Pixels,
}

impl Global for TypeTokens {}

impl TypeTokens {
    pub fn of(scale: FontScale) -> Self {
        let m = scale.multiplier();
        // headline-lg / body-md / label-sm are table-exact per scale;
        // every other token rounds its default value the same way.
        let ((hlg_s, hlg_l), (bmd_s, bmd_l), (lsm_s, lsm_l)) = match scale {
            FontScale::Compact => ((16., 21.), (11., 16.), (10., 12.)),
            FontScale::Default => ((18., 24.), (13., 18.), (11., 14.)),
            FontScale::Large => ((20., 27.), (15., 20.), (12., 16.)),
        };
        let scaled = |size: f32, line: f32| -> (Pixels, Pixels) {
            (px((size * m).round()), px((line * m).round()))
        };
        let inter = |size: f32, line: f32, weight: FontWeight| -> TypeToken {
            let (size, line) = scaled(size, line);
            TypeToken {
                size,
                line,
                weight,
                mono: false,
            }
        };
        let mono = |size: f32, line: f32| -> TypeToken {
            let (size, line) = scaled(size, line);
            TypeToken {
                size,
                line,
                weight: FontWeight::MEDIUM,
                mono: true,
            }
        };
        Self {
            headline_lg: TypeToken {
                size: px(hlg_s),
                line: px(hlg_l),
                weight: FontWeight::SEMIBOLD,
                mono: false,
            },
            headline_md: inter(16., 20., FontWeight::SEMIBOLD),
            headline_sm: inter(15., 18., FontWeight::SEMIBOLD),
            body_md: TypeToken {
                size: px(bmd_s),
                line: px(bmd_l),
                weight: FontWeight::NORMAL,
                mono: false,
            },
            label_lg: mono(14., 18.),
            label_sm: TypeToken {
                size: px(lsm_s),
                line: px(lsm_l),
                weight: FontWeight::MEDIUM,
                mono: true,
            },
            control_h: scale.control_h(),
            row_h: scale.row_h(),
        }
    }
}

/// Resolved §3 tokens for the active scale (default scale before `apply`).
pub fn types(cx: &App) -> TypeTokens {
    if cx.has_global::<TypeTokens>() {
        *cx.global::<TypeTokens>()
    } else {
        TypeTokens::of(FontScale::Default)
    }
}

/// Apply a resolved §3 token: absolute size + line height + weight, with
/// the system monospace family for label tokens. A later `font_weight`
/// still overrides, so conditional emphasis keeps working on top of a token.
pub trait TypeStyled: Styled {
    fn tx(self, tok: TypeToken) -> Self {
        let this = self
            .text_size(tok.size)
            .line_height(tok.line)
            .font_weight(tok.weight);
        if tok.mono {
            this.font_family(mono_family())
        } else {
            this
        }
    }
}

impl<T: Styled> TypeStyled for T {}

/// System monospace family, captured from the probed theme in `apply`.
///
/// `tx` has no `App` access, so the family the kit's `Theme::change` probe
/// resolved (an installed family, else the virtual `.SystemUIFont`) is
/// cached here once. Falls back to the virtual system font when `apply`
/// has not run (headless tests).
static MONO_FAMILY: std::sync::OnceLock<SharedString> = std::sync::OnceLock::new();

pub fn set_mono_family(family: SharedString) {
    let _ = MONO_FAMILY.set(family);
}

fn mono_family() -> SharedString {
    MONO_FAMILY
        .get()
        .cloned()
        .unwrap_or_else(|| ".SystemUIFont".into())
}
#[cfg(test)]
mod tests {
    use super::{FontScale, TypeTokens};
    use gpui_kit::px;

    #[test]
    fn default_ladder_is_former_large() {
        let t = TypeTokens::of(FontScale::Default);
        assert_eq!(t.headline_lg.size, px(18.));
        assert_eq!(t.headline_lg.line, px(24.));
        assert_eq!(t.body_md.size, px(13.));
        assert_eq!(t.body_md.line, px(18.));
        assert_eq!(t.label_sm.size, px(11.));
        assert_eq!(t.label_sm.line, px(14.));
        assert_eq!(t.control_h, px(28.));
        assert_eq!(t.row_h, px(32.));
        assert_eq!(t.headline_md.size, px(16.));
        assert_eq!(t.label_lg.size, px(14.));
    }

    #[test]
    fn parse_unknown_is_default() {
        assert!(matches!(FontScale::parse(""), FontScale::Default));
        assert!(matches!(FontScale::parse("large"), FontScale::Large));
        assert!(matches!(FontScale::parse("compact"), FontScale::Compact));
    }
}
