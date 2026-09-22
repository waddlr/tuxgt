use gpui_kit::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeId {
    TuxDark,
    TuxDarkBlue,
    TuxLight,
    TuxLightBlue,
    CatppuccinMocha,
    Nord,
    Dracula,
    GruvboxDark,
    TokyoNight,
    BreezeDark,
}

impl ThemeId {
    pub const ALL: &[ThemeId] = &[
        ThemeId::TuxDark,
        ThemeId::TuxDarkBlue,
        ThemeId::TuxLight,
        ThemeId::TuxLightBlue,
        ThemeId::CatppuccinMocha,
        ThemeId::Nord,
        ThemeId::Dracula,
        ThemeId::GruvboxDark,
        ThemeId::TokyoNight,
        ThemeId::BreezeDark,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ThemeId::TuxDark => "tuxgt-dark",
            ThemeId::TuxDarkBlue => "tuxgt-dark-blue",
            ThemeId::TuxLight => "tuxgt-light",
            ThemeId::TuxLightBlue => "tuxgt-light-blue",
            ThemeId::CatppuccinMocha => "catppuccin-mocha",
            ThemeId::Nord => "nord",
            ThemeId::Dracula => "dracula",
            ThemeId::GruvboxDark => "gruvbox-dark",
            ThemeId::TokyoNight => "tokyo-night",
            ThemeId::BreezeDark => "breeze-dark",
        }
    }

    pub fn label_id(self) -> &'static str {
        match self {
            ThemeId::TuxDark => "gui-theme-tuxgt-dark",
            ThemeId::TuxDarkBlue => "gui-theme-tuxgt-dark-blue",
            ThemeId::TuxLight => "gui-theme-tuxgt-light",
            ThemeId::TuxLightBlue => "gui-theme-tuxgt-light-blue",
            ThemeId::CatppuccinMocha => "gui-theme-catppuccin",
            ThemeId::Nord => "gui-theme-nord",
            ThemeId::Dracula => "gui-theme-dracula",
            ThemeId::GruvboxDark => "gui-theme-gruvbox",
            ThemeId::TokyoNight => "gui-theme-tokyo",
            ThemeId::BreezeDark => "gui-theme-breeze",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "tuxgt-dark" => ThemeId::TuxDark,
            "tuxgt-dark-blue" | "stitch" => ThemeId::TuxDarkBlue,
            "tuxgt-light" => ThemeId::TuxLight,
            "tuxgt-light-blue" => ThemeId::TuxLightBlue,
            "catppuccin-mocha" => ThemeId::CatppuccinMocha,
            "nord" => ThemeId::Nord,
            "dracula" => ThemeId::Dracula,
            "gruvbox-dark" => ThemeId::GruvboxDark,
            "tokyo-night" => ThemeId::TokyoNight,
            "breeze-dark" => ThemeId::BreezeDark,
            _ => ThemeId::TuxDark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThemeId;

    #[test]
    fn parse_stitch_is_dark_blue() {
        assert_eq!(ThemeId::parse("stitch"), ThemeId::TuxDarkBlue);
    }

    #[test]
    fn parse_unknown_is_tux_dark() {
        assert_eq!(ThemeId::parse(""), ThemeId::TuxDark);
        assert_eq!(ThemeId::parse("nope"), ThemeId::TuxDark);
    }

    #[test]
    fn parse_tuxgt_dark_and_all_order() {
        assert_eq!(ThemeId::parse("tuxgt-dark"), ThemeId::TuxDark);
        assert_eq!(ThemeId::ALL[0], ThemeId::TuxDark);
        assert_eq!(ThemeId::TuxDark.id(), "tuxgt-dark");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontScale {
    Compact,
    Default,
    Large,
}

impl FontScale {
    pub const ALL: &[FontScale] = &[FontScale::Compact, FontScale::Default, FontScale::Large];

    pub fn id(self) -> &'static str {
        match self {
            FontScale::Compact => "compact",
            FontScale::Default => "default",
            FontScale::Large => "large",
        }
    }

    pub fn label_id(self) -> &'static str {
        match self {
            FontScale::Compact => "gui-scale-compact",
            FontScale::Default => "gui-scale-default",
            FontScale::Large => "gui-scale-large",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "compact" => FontScale::Compact,
            "large" => FontScale::Large,
            _ => FontScale::Default,
        }
    }

    pub fn ui(self) -> Pixels {
        match self {
            FontScale::Compact => px(13.),
            FontScale::Default => px(15.),
            FontScale::Large => px(17.),
        }
    }

    pub fn mono(self) -> Pixels {
        match self {
            FontScale::Compact => px(11.),
            FontScale::Default => px(13.),
            FontScale::Large => px(15.),
        }
    }
}

impl FontScale {
    /// GUI.md §3 scale multiplier: compact 0.875, default 1.0, large 1.125.
    pub fn multiplier(self) -> f32 {
        match self {
            FontScale::Compact => 0.875,
            FontScale::Default => 1.0,
            FontScale::Large => 1.125,
        }
    }

    /// GUI.md §3 control height: 24 / 28 / 32.
    pub fn control_h(self) -> Pixels {
        match self {
            FontScale::Compact => px(24.),
            FontScale::Default => px(28.),
            FontScale::Large => px(32.),
        }
    }

    /// Table/list row height floor. Sidebar rows size to content instead.
    pub fn row_h(self) -> Pixels {
        match self {
            FontScale::Compact => px(28.),
            FontScale::Default => px(32.),
            FontScale::Large => px(36.),
        }
    }
}
