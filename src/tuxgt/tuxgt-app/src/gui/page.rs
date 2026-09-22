use std::time::Duration;

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

    /// Bounds are written during layout, so the first frame of a cold window
    /// still sees a zero viewport. Retry once so the pack window can cull;
    /// a later real size clears the flag.
    pub(crate) fn arm_pack_viewport(&mut self, cx: &mut Context<Self>) {
        if self.page_scroll.bounds().size.height > px(0.) {
            self.pack_viewport_armed = false;
            return;
        }
        let waiting = self.nav == Nav::Settings
            && self.settings_tab == SettingsTab::Mods
            && self.mods_tab == SettingsModsTab::Reshade;
        if !waiting || self.pack_viewport_armed {
            return;
        }
        self.pack_viewport_armed = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(32))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
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
        // Settings → Mods already replaces its tab body with the password
        // card. Every other page shows the same card in place of the body.
        let on_mods = self.nav == Nav::Settings && self.settings_tab == SettingsTab::Mods;
        if self.pending_archive_password.is_some() && !on_mods {
            return self.archive_password_box(view, cx).into_any_element();
        }
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
