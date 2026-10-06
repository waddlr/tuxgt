use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::Shell;

impl Shell {
    /// Warn-only WoW64 row (`gui.wow64-warning`): first child of the
    /// Launch Mode card when the loader found an active WoW64 channel on
    /// a 32-bit game. Silent (empty box) otherwise.
    pub(crate) fn wow64_row(&self, cx: &App) -> impl IntoElement {
        let Some(source) = self.wow64_warn else {
            return div().into_any_element();
        };
        let b = cx.theme();
        div()
            .id("wow64-warning")
            .tx(types(cx).body_md)
            .text_color(b.warning)
            .child(self.strings.get(source.fluent_id()))
            .into_any_element()
    }
}
