use gpui_kit::component::v_flex;
use gpui_kit::*;

use super::*;

impl Shell {
    pub(crate) fn enabled_n(&self, game_id: &str) -> usize {
        self.mod_counts.get(game_id).copied().unwrap_or(0)
    }

    pub(crate) fn scroll_page_top(&self) {
        self.page_scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(crate) fn page_chrome(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        v_flex()
            .id("page-chrome")
            .flex_none()
            .w_full()
            .flex_shrink_0()
            .px(px(14.))
            .pt_3()
            .gap_2()
            .child(match self.nav {
                Nav::Library => self.library_chrome(view, cx).into_any_element(),
                Nav::Game => self.game_chrome(view, cx).into_any_element(),
                Nav::Settings => self.settings_chrome(view, cx).into_any_element(),
            })
    }

    pub(crate) fn page_body(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        match self.nav {
            Nav::Library => self.library_scroll(view, cx).into_any_element(),
            Nav::Game => self.game_scroll(view, cx).into_any_element(),
            Nav::Settings => self.settings_body(view, cx).into_any_element(),
        }
    }

    pub(crate) fn page_scroller(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        // Slot (flex_1 + min_h_0 + overflow_hidden) is the only flex item.
        // Viewport is size_full of that slot. Content is flex_none so Taffy
        // cannot shrink it to the viewport (which makes scroll_max = 0).
        div()
            .id("page-slot")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(
                div()
                    .id("page")
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.page_scroll)
                    .child(
                        div()
                            .id("page-content")
                            .flex_none()
                            .w_full()
                            .px(px(14.))
                            .pb_3()
                            .child(self.page_body(view, cx)),
                    ),
            )
    }
}
