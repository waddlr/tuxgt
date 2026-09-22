use gpui_kit::component::accordion::Accordion;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{v_flex, Sizable as _};
use gpui_kit::*;

use super::super::widgets;
use super::super::Shell;

impl Shell {
    pub(crate) fn hide_row(
        &self,
        g: &tuxgt_core::GameRow,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let game_id = g.id.clone();
        widgets::labeled_row(
            "about-hidden",
            self.strings.get("gui-label-hide-library"),
            None,
            Switch::new("game-hidden")
                .checked(g.hidden)
                .xsmall()
                .on_click({
                    let view = view.clone();
                    move |on, _, cx| {
                        let on = *on;
                        let game_id = game_id.clone();
                        view.update(cx, |this, cx| this.set_hidden_ui(&game_id, on, cx));
                    }
                }),
            cx,
        )
    }

    pub(crate) fn general_advanced(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let open = self.general_advanced;
        Accordion::new("general-advanced")
            .item(|item| {
                item.title(widgets::section_title(
                    self.strings.get("gui-section-advanced"),
                    cx,
                ))
                .open(open)
                .child(
                    v_flex()
                        .gap_3()
                        .child(self.extra_exes_section(view.clone(), cx))
                        .child(self.detect_section(view.clone(), cx)),
                )
            })
            .on_toggle_click({
                let view = view.clone();
                move |open: &[usize], _, cx| {
                    view.update(cx, |this, cx| {
                        this.general_advanced = !open.is_empty();
                        cx.notify();
                    });
                }
            })
    }
}
