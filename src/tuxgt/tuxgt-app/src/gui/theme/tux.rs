use super::palettes::Pal;

fn dark(primary: u32, on_primary: u32, wash: u32, phover: u32) -> Pal {
    Pal {
        bg: 0x08090b,
        pane: 0x0b0c0f,
        // E139: blue-gray, not brown-gray; selected darker/subtler than
        // elevated. Stack order stays pane < card < selected.
        card: 0x15181d,
        inset: 0x050608,
        elevated: 0x3c4048,
        selected: 0x2a3038,
        wash,
        hairline: 0x2a2e36,
        fg: 0xd0d5db,
        muted: 0x8b939e,
        outline: 0x2a2e36,
        primary,
        on_primary,
        primary_hover: phover,
        mint: 0x6a8ea3,
        mauve: 0xb48ead,
        danger: 0xc45c68,
        warning: 0xe5c07b,
        gold: 0xcfb53b,
    }
}

fn light(primary: u32, on_primary: u32, phover: u32) -> Pal {
    Pal {
        bg: 0xc2cad2,
        pane: 0xb6bec6,
        card: 0xb6bec6,
        inset: 0xaab3bb,
        elevated: 0x8d97a1,
        selected: 0x8d97a1,
        wash: 0xa8b8c4,
        hairline: 0x9aa3ab,
        fg: 0x262828,
        muted: 0x545a5c,
        outline: 0x9aa3ab,
        primary,
        on_primary,
        primary_hover: phover,
        mint: 0x6a8ea3,
        mauve: 0xb48ead,
        danger: 0xb86870,
        warning: 0xe5c07b,
        gold: 0xcfb53b,
    }
}

pub(crate) fn tux_dark() -> Pal {
    dark(0xe0ae4a, 0x1a1206, 0x0e1014, 0xe8bc5e)
}

pub(crate) fn tux_dark_blue() -> Pal {
    dark(0x7ec8f0, 0x0a1820, 0x0c1822, 0x8cd0f4)
}

pub(crate) fn tux_light() -> Pal {
    light(0xc4a45c, 0x1a1408, 0xb4944e)
}

pub(crate) fn tux_light_blue() -> Pal {
    light(0x5a9ec4, 0x0c1820, 0x4c8eb6)
}

#[cfg(test)]
mod tests {
    use super::super::id::ThemeId;
    use super::super::palettes::pal;

    #[test]
    fn tux_primaries_and_stack() {
        assert_eq!(pal(ThemeId::TuxDark).primary, 0xe0ae4a);
        assert_eq!(pal(ThemeId::TuxDarkBlue).primary, 0x7ec8f0);
        assert_eq!(pal(ThemeId::TuxDark).bg, 0x08090b);
        assert_eq!(pal(ThemeId::TuxLight).fg, 0x262828);
        assert_eq!(pal(ThemeId::TuxLight).muted, 0x545a5c);
        // E139: blue-gray card/selected on both dark themes.
        assert_eq!(pal(ThemeId::TuxDark).card, 0x15181d);
        assert_eq!(pal(ThemeId::TuxDarkBlue).card, 0x15181d);
        assert_eq!(pal(ThemeId::TuxDark).selected, 0x2a3038);
        assert_eq!(pal(ThemeId::TuxDark).fg, 0xd0d5db);
    }

    /// Primary hover must differ from primary on every theme, or primary
    /// buttons show no hover change (kit reads `button_primary_hover`).
    #[test]
    fn every_theme_hovers_primary() {
        for id in ThemeId::ALL {
            let p = pal(*id);
            assert_ne!(p.primary_hover, p.primary, "{id:?} primary has no hover delta");
        }
    }

    /// E139: dark stack order (pane < card < selected) and blue tint
    /// (blue channel at or above red) on card/selected.
    #[test]
    fn dark_stack_order_and_blue_tint() {
        for id in [ThemeId::TuxDark, ThemeId::TuxDarkBlue] {
            let p = pal(id);
            assert!(p.pane < p.card, "card must sit above pane");
            assert!(p.card < p.selected, "selected must sit above card");
            for well in [p.card, p.selected] {
                let (r, g, b) = (well >> 16 & 0xff, well >> 8 & 0xff, well & 0xff);
                assert!(b >= r && g >= r, "well {well:06x} must be blue-gray");
            }
        }
    }
}
