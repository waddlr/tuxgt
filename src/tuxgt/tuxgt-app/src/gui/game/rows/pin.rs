//! Game-card pin row: secondary button, parent copy line, copy sentence.
//! A copy never paints Update.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::super::super::Shell;

impl Shell {
    pub(crate) fn pin_update_cluster(
        &self,
        game_id: &str,
        instance: &str,
        show_button: bool,
        names: &[String],
        update: Option<AnyElement>,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        let controls = if show_button || update.is_some() {
            let mut row = h_flex().gap_2().flex_wrap().items_center();
            if show_button {
                row = row.child(self.pin_version_button(
                    view,
                    SharedString::from(format!("pin-{game_id}-{instance}")),
                    instance.to_string(),
                    Some(game_id.to_string()),
                    cx,
                ));
            }
            if let Some(update) = update {
                row = row.child(update);
            }
            Some(row.into_any_element())
        } else {
            None
        };
        if names.is_empty() {
            return controls.unwrap_or_else(|| div().into_any_element());
        }
        v_flex()
            .gap_1()
            .child(self.pin_copy_line(names, cx))
            .when_some(controls, |this, el| this.child(el))
            .into_any_element()
    }
}
