use super::*;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared};

use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn wrappers_section(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        widgets::section_card("wrappers", cx)
            .child(widgets::section_title(
                self.strings.get("gui-section-wrappers"),
                cx,
            ))
            .child(self.wrapper_row(
                game_id,
                "gamemode",
                "gui-wrapper-gamemode",
                "gui-wrapper-gamemode-help",
                view.clone(),
                cx,
            ))
            .child(self.wrapper_row(
                game_id,
                "gamescope",
                "gui-wrapper-gamescope",
                "gui-wrapper-gamescope-help",
                view.clone(),
                cx,
            ))
            .child(self.wrapper_row(
                game_id,
                "mangohud",
                "gui-wrapper-mangohud",
                "gui-wrapper-mangohud-help",
                view,
                cx,
            ))
            .when(
                self.is_client_game(game_id)
                    && self.handle.get(game_id).copied().unwrap_or(false)
                    && self.wrappers.is_empty(),
                |this| {
                    this.child(widgets::muted(
                        self.strings.get("gui-note-wrappers-need-apply"),
                        cx,
                    ))
                },
            )
    }

    pub(crate) fn wrapper_row(
        &self,
        game_id: &str,
        id: &'static str,
        label_id: &'static str,
        help_id: &'static str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let on = self.wrappers.iter().any(|w| w == id);
        let game = game_id.to_string();
        let switch = Switch::new(SharedString::from(format!("wr-{id}")))
            .checked(on)
            .xsmall()
            .on_click(move |val, _, cx| {
                let on = *val;
                let game = game.clone();
                view.update(cx, |this, cx| this.toggle_wrapper_ui(&game, id, on, cx));
            });
        widgets::labeled_row(
            id,
            self.strings.get(label_id),
            None,
            switch.tooltip(self.strings.get(help_id)),
            cx,
        )
    }

    /// E44: toggle one overlay wrapper, then refresh the stored set and the
    /// launch preview (`launch --print` reads the same rows).
    pub(crate) fn toggle_wrapper_ui(
        &mut self,
        game: &str,
        wrapper: &'static str,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "toggle-wrapper", game, wrapper, on);
        let game = game.to_string();
        let owned = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(&pool, &data_dir(), &host, &owned, async {
                            if on {
                                tuxgt_core::set_wrapper(&pool, &owned, wrapper).await
                            } else {
                                tuxgt_core::unset_wrapper(&pool, &owned, wrapper).await
                            }
                        })
                        .await?;
                        tuxgt_core::game_wrappers(&pool, &owned).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(game.as_str()) {
                    return;
                }
                let outcome = if result.is_ok() { "wrapper-toggled" } else { "error" };
                tracing::debug!(action = "toggle-wrapper", game = game.as_str(), outcome);
                match result {
                    Ok(rows) => {
                        this.wrappers = rows.into_boxed_slice();
                        this.session_payload
                            .insert(game.clone(), super::load_payload(&game));
                        // E94: core may have just auto-restored (wrappers off
                        // with nothing else needing a channel), so the
                        // auto-switch below reads core truth, not the old arm.
                        this.reload_armed_state(&game);
                        this.reload_launch_state(cx);
                        let hook = this.handle.get(&game).copied().unwrap_or(false);
                        if hook && this.wrappers.iter().any(|w| tuxgt_core::is_argv_wrapper(w)) {
                            this.select_launch_mode_ui(LaunchMode::Apply, cx);
                        }
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
