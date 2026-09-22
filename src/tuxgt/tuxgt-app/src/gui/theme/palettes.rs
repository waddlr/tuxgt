use gpui_kit::component::theme::{Theme, ThemeColor, ThemeMode, ThemeTokens};
use gpui_kit::*;

use super::brand::Brand;
use super::id::{FontScale, ThemeId};
use super::ty::TypeTokens;

pub(crate) struct Pal {
    pub(crate) bg: u32,
    pub(crate) pane: u32,
    pub(crate) card: u32,
    pub(crate) inset: u32,
    pub(crate) elevated: u32,
    pub(crate) selected: u32,
    pub(crate) wash: u32,
    pub(crate) hairline: u32,
    pub(crate) fg: u32,
    pub(crate) muted: u32,
    pub(crate) outline: u32,
    pub(crate) primary: u32,
    pub(crate) on_primary: u32,
    pub(crate) primary_hover: u32,
    pub(crate) mint: u32,
    pub(crate) mauve: u32,
    pub(crate) danger: u32,
    pub(crate) warning: u32,
    pub(crate) gold: u32,
}

pub(crate) fn pal(id: ThemeId) -> Pal {
    match id {
        ThemeId::TuxDark => super::tux::tux_dark(),
        ThemeId::TuxDarkBlue => super::tux::tux_dark_blue(),
        ThemeId::TuxLight => super::tux::tux_light(),
        ThemeId::TuxLightBlue => super::tux::tux_light_blue(),
        ThemeId::CatppuccinMocha => super::community::catppuccin(),
        ThemeId::Nord => super::community::nord(),
        ThemeId::Dracula => super::community::dracula(),
        ThemeId::GruvboxDark => super::community::gruvbox(),
        ThemeId::TokyoNight => super::community::tokyo(),
        ThemeId::BreezeDark => super::community::breeze(),
    }
}

pub(crate) fn hx(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub(crate) fn paint(c: &mut ThemeColor, p: &Pal) {
    let bg = hx(p.bg);
    let pane = hx(p.pane);
    let card = hx(p.card);
    let inset = hx(p.inset);
    let elevated = hx(p.elevated);
    let selected = hx(p.selected);
    let hair = hx(p.hairline);
    let fg = hx(p.fg);
    let muted = hx(p.muted);
    let outline = hx(p.outline);
    let primary = hx(p.primary);
    let on_p = hx(p.on_primary);
    let phover = hx(p.primary_hover);
    let mint = hx(p.mint);
    let danger = hx(p.danger);
    let warning = hx(p.warning);

    c.background = bg;
    c.foreground = fg;
    c.border = hair;
    c.window_border = hair;
    c.title_bar = pane;
    c.title_bar_border = hair;
    c.status_bar = inset;
    c.status_bar_border = hair;
    c.sidebar = pane;
    c.sidebar_foreground = fg;
    c.sidebar_border = hair;
    c.sidebar_accent = selected;
    c.sidebar_accent_foreground = fg;
    c.sidebar_primary = primary;
    c.sidebar_primary_foreground = on_p;
    c.primary = primary;
    c.primary_foreground = on_p;
    c.primary_hover = phover;
    c.primary_active = phover;
    c.button_primary = primary;
    c.button_primary_foreground = on_p;
    c.button_primary_hover = phover;
    c.button_primary_active = phover;
    c.secondary = card;
    c.secondary_foreground = fg;
    c.secondary_hover = elevated;
    c.secondary_active = elevated;
    c.button_secondary = card;
    c.button_secondary_foreground = fg;
    c.button_secondary_hover = elevated;
    c.button_secondary_active = elevated;
    c.muted = card;
    c.muted_foreground = muted;
    c.accent = primary;
    c.accent_foreground = on_p;
    c.popover = elevated;
    c.popover_foreground = fg;
    c.list = pane;
    c.list_even = bg;
    c.list_hover = card;
    c.list_active = selected;
    c.list_active_border = primary;
    c.list_head = pane;
    c.input = hair;
    c.caret = primary;
    c.selection = primary;
    c.ring = primary;
    c.link = primary;
    c.link_hover = phover;
    c.link_active = phover;
    c.tab = pane;
    c.tab_active = card;
    c.tab_active_foreground = primary;
    c.tab_bar = pane;
    c.tab_bar_segmented = inset;
    c.tab_foreground = muted;
    c.danger = danger;
    c.danger_foreground = hx(0xffffff);
    c.danger_hover = danger;
    c.danger_active = danger;
    c.button_danger = danger;
    c.button_danger_foreground = hx(0xffffff);
    c.button_danger_hover = danger;
    c.button_danger_active = danger;
    c.warning = warning;
    c.warning_foreground = bg;
    c.warning_hover = warning;
    c.warning_active = warning;
    c.success = mint;
    c.success_foreground = bg;
    c.success_hover = mint;
    c.success_active = mint;
    c.info = primary;
    c.info_foreground = on_p;
    c.switch = outline;
    c.switch_thumb = fg;
    c.scrollbar = bg;
    c.scrollbar_thumb = hair;
    c.scrollbar_thumb_hover = outline;
    c.group_box = card;
    c.group_box_foreground = fg;
    c.table = pane;
    c.table_even = bg;
    c.table_hover = card;
    c.table_head = pane;
    c.table_row_border = hair;
    c.overlay = bg;
    c.accordion = card;
    c.progress_bar = primary;
    c.cyan = primary;
    c.green = mint;
    c.red = danger;
    c.yellow = warning;
    c.magenta = hx(p.mauve);
    c.blue = primary;
}

pub fn apply(cx: &mut App, window: Option<&mut Window>, id: ThemeId, scale: FontScale) {
    let mode = match id {
        ThemeId::TuxLight | ThemeId::TuxLightBlue => ThemeMode::Light,
        _ => ThemeMode::Dark,
    };
    Theme::change(mode, None, cx);
    let p = pal(id);
    // 0.6.4 has no Theme::update.
    {
        let theme = Theme::global_mut(cx);
        paint(&mut theme.colors, &p);
        theme.tokens = ThemeTokens::from(&theme.colors);
        // Families stay on the kit defaults: Theme::change above probed
        // .SystemUIFont + mono to installed system fonts. Capture the
        // probed mono for cx-free `tx` label tokens.
        super::ty::set_mono_family(theme.mono_font_family.clone());
        theme.font_size = scale.ui();
        theme.mono_font_size = scale.mono();
        theme.radius = px(8.);
        theme.radius_lg = px(8.);
        theme.shadow = true;
        theme.notification.placement = gpui::Anchor::TopRight;
        theme.notification.margins.top = px(50.);
    }
    cx.set_global(Brand::from_pal(&p));
    cx.set_global(TypeTokens::of(scale));
    Theme::sync_base(cx);
    if let Some(window) = window {
        window.refresh();
    }
}
