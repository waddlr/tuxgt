use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, IconName, Sizable as _};
use gpui_kit::gpui::StatefulInteractiveElement as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// E138: drag flag for the product bar. Kit `TitleBar` always appends its
/// own `WindowControls` (Close hover hardcoded `danger`), so the product
/// bar owns drag/zoom/menu itself and paints its own buttons at kit size.
pub(crate) struct BarDrag {
    pub(crate) should_move: bool,
}

/// A titlebar move starts only from an armed drag with Left still held. A
/// release outside the titlebar (shadow zone, off-window) never disarms, so
/// buttonless motion after a top-edge resize must not grab the window.
pub(crate) fn should_start_move(armed: bool, pressed: Option<MouseButton>) -> bool {
    armed && pressed == Some(MouseButton::Left)
}

/// T25: chrome colors, read once per paint at the top of `titlebar()`.
/// `fg` is the default icon color; back/fwd/bell override it per site
/// via struct-update syntax.
#[derive(Clone, Copy)]
pub(crate) struct ChromeColors {
    pub(crate) fg: Hsla,
    pub(crate) hover: Hsla,
    pub(crate) hover_fg: Hsla,
    pub(crate) active: Hsla,
}

/// E138: one titlebar button. 34px cell like kit `ControlIcon`; hover is a
/// 24px muted circle behind the icon on every button — window and product
/// alike, close included, never `danger`. `selected` pins the circle on
/// (bell panel open); `disabled` idles the icon with no hover or click.
#[allow(clippy::too_many_arguments)]
pub(crate) fn chrome_btn(
    id: &'static str,
    group: &'static str,
    icon: IconName,
    colors: ChromeColors,
    tip: impl Into<SharedString>,
    disabled: bool,
    selected: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let group = SharedString::from(group);
    let tip: SharedString = tip.into();
    div()
        .id(id)
        .group(group.clone())
        .w(px(34.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .when(!tip.is_empty(), |this| {
            this.tooltip({
                let tip = tip.clone();
                move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
            })
        })
        // E138: the down-guard and double-click stop stay armed even when
        // disabled — otherwise a click-drag on a dimmed nav button starts
        // a window move, and a double-click zooms through the button.
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_double_click(|_, _, cx| {
            cx.stop_propagation();
        })
        .when(!disabled, |this| {
            this.on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_click(window, cx);
            })
        })
        .child(
            div()
                .id(group.clone())
                .size(px(24.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors.fg)
                .when(selected, |this| {
                    this.bg(colors.hover).text_color(colors.hover_fg)
                })
                .when(!disabled, |this| {
                    this.group_hover(group, |this| {
                        this.bg(colors.hover).text_color(colors.hover_fg)
                    })
                    .active(|s| s.bg(colors.active).text_color(colors.hover_fg))
                })
                .child(Icon::new(icon).small()),
        )
}

#[cfg(test)]
mod tests {
    use super::should_start_move;
    use gpui_kit::MouseButton;

    #[test]
    fn move_needs_armed_held_left() {
        assert!(should_start_move(true, Some(MouseButton::Left)));
        // Stale arm after a release the titlebar never saw: buttonless
        // motion (e.g. after a top-edge resize) must not grab the window.
        assert!(!should_start_move(true, None));
        assert!(!should_start_move(true, Some(MouseButton::Right)));
        assert!(!should_start_move(false, Some(MouseButton::Left)));
    }
}
