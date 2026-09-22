use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::Shell;

impl Shell {
    /// E106: host identity on Settings → General, under Appearance. The same
    /// probes the retired status bar painted (`gui/sys.rs`): identity, not
    /// meters, and no timers.
    pub(crate) fn host_box(&self, cx: &App) -> impl IntoElement {
        let t = types(cx);
        let value = |s: &str| {
            div()
                .tx(t.body_md)
                .text_color(cx.theme().foreground)
                .text_right()
                .min_w_0()
                .child(s.to_string())
        };
        widgets::section_card("host-id", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-host"),
                cx,
            ))
            .child(widgets::labeled_row(
                "host-os",
                self.strings.get("gui-host-os"),
                None,
                value(&self.sys.os),
                cx,
            ))
            .child(widgets::row_hairline(cx))
            .child(widgets::labeled_row(
                "host-de",
                self.strings.get("gui-host-de"),
                None,
                value(&self.sys.desktop),
                cx,
            ))
            .child(widgets::row_hairline(cx))
            .child(widgets::labeled_row(
                "host-cpu",
                self.strings.get("gui-host-cpu"),
                None,
                value(&self.sys.cpu),
                cx,
            ))
            .child(widgets::row_hairline(cx))
            .child(widgets::labeled_row(
                "host-ram",
                self.strings.get("gui-host-ram"),
                None,
                value(&self.sys.ram),
                cx,
            ))
            .child(widgets::row_hairline(cx))
            .child(widgets::labeled_row(
                "host-gpu",
                self.strings.get("gui-host-gpu"),
                None,
                value(&self.sys.gpu),
                cx,
            ))
    }
}
