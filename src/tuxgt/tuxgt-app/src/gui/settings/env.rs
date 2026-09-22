use gpui_kit::component::v_flex;
use gpui_kit::*;

use super::super::widgets;
use super::super::{EnvPage, Shell};

impl Shell {
    pub(crate) fn settings_env(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        v_flex()
            .id("settings-env")
            .w_full()
            .flex_shrink_0()
            .gap_3()
            .child(widgets::placeholder_note(
                self.strings.get("gui-note-settings-env"),
                cx,
            ))
            .child(self.env_groups(EnvPage::Global, view, cx))
    }
}
