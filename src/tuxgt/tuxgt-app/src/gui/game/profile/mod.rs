mod actions;
mod dialog;
mod gate;
mod modal;
mod rearm;
mod section;
mod start;
mod stop;

pub(crate) use dialog::ProfileDialog;
pub(crate) use start::{default_new_start, NewStart, HERO_BODY};

use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, preview_switch, switch_profile, FluentArgs};

use super::super::widgets;
use super::super::{rt_block, ClientStopOp, Note, Shell};

impl Shell {
    /// Hero display only: which profile Play will use. Selection lives
    /// in the General Profile card (`profile_section`).
    pub(crate) fn profile_pill(&self, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        let name = self
            .profiles
            .as_ref()
            .and_then(|s| s.active_profile())
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "…".to_string());
        let mut args = FluentArgs::new();
        args.set("name", name);
        h_flex().id(HERO_BODY[1]).child(widgets::pill(
            self.strings.get_args("gui-profile-pill", Some(&args)),
            b.muted_foreground,
            b.border,
            cx,
        ))
    }

    pub(crate) fn profile_use_ui(&mut self, target: String, cx: &mut Context<Self>) {
        let Some(game) = self.selected.clone() else {
            return;
        };
        let name = self
            .profiles
            .as_ref()
            .and_then(|s| s.by_id(&target))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| target.clone());
        self.spawn_preview(game, target, name, cx);
    }

    pub(crate) fn spawn_preview(
        &mut self,
        game: String,
        target: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let target_bg = target.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        preview_switch(&pool, &data_dir(), &game_bg, &target_bg).await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(preview) if preview.file_changes.is_empty() && !preview.drops_needs => {
                    this.switch_go(game, target, name, true, true, false, cx);
                }
                Ok(preview) if !preview.file_changes.is_empty() => {
                    this.open_files_dialog(
                        preview.file_changes,
                        {
                            let game = game.clone();
                            let target = target.clone();
                            let name = name.clone();
                            move |this, cx| {
                                this.switch_go(
                                    game.clone(),
                                    target.clone(),
                                    name.clone(),
                                    true,
                                    false,
                                    false,
                                    cx,
                                )
                            }
                        },
                        window,
                        cx,
                    );
                }
                Ok(preview) if preview.drops_needs => {
                    this.open_restore_dialog(
                        {
                            let game = game.clone();
                            let target = target.clone();
                            let name = name.clone();
                            move |this, cx| {
                                this.switch_go(
                                    game.clone(),
                                    target.clone(),
                                    name.clone(),
                                    true,
                                    true,
                                    false,
                                    cx,
                                )
                            }
                        },
                        window,
                        cx,
                    );
                }
                Ok(_) => this.switch_go(game, target, name, true, true, false, cx),
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn switch_go(
        &mut self,
        game: String,
        target: String,
        name: String,
        yes_files: bool,
        yes_restore: bool,
        yes_client_stop: bool,
        cx: &mut Context<Self>,
    ) {
        let transfer = self.hide_state.installing();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let game_bg = game.clone();
            let target_bg = target.clone();
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        switch_profile(
                            &pool,
                            &data_dir(),
                            &game_bg,
                            &target_bg,
                            yes_files,
                            yes_restore,
                            yes_client_stop,
                        )
                        .await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(out) => this.after_profile_switch(
                    &game,
                    &name,
                    out.restore_deferred,
                    out.rearm_deferred,
                    cx,
                ),
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-files:") && !yes_files =>
                {
                    let dests = msg
                        .split_once(':')
                        .map(|(_, rest)| rest)
                        .unwrap_or(&msg)
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    this.open_files_dialog(
                        dests,
                        {
                            let game = game.clone();
                            let target = target.clone();
                            let name = name.clone();
                            let yes_restore = yes_restore;
                            move |this, cx| {
                                this.switch_go(
                                    game.clone(),
                                    target.clone(),
                                    name.clone(),
                                    true,
                                    yes_restore,
                                    false,
                                    cx,
                                )
                            }
                        },
                        window,
                        cx,
                    );
                }
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-restore:") && !yes_restore =>
                {
                    this.open_restore_dialog(
                        {
                            let game = game.clone();
                            let target = target.clone();
                            let name = name.clone();
                            move |this, cx| {
                                this.switch_go(
                                    game.clone(),
                                    target.clone(),
                                    name.clone(),
                                    true,
                                    true,
                                    false,
                                    cx,
                                )
                            }
                        },
                        window,
                        cx,
                    );
                }
                Err(tuxgt_core::Error::NeedConfirm(msg))
                    if msg.starts_with("profile-client-stop:") && !yes_client_stop =>
                {
                    let op = ClientStopOp::ProfileSwitch {
                        target: target.clone(),
                        name: name.clone(),
                        yes_files,
                        yes_restore,
                    };
                    if !this.park_client_stop(&game, op, true, cx) {
                        this.profile_switch_stop_ui(
                            game.clone(),
                            target.clone(),
                            name.clone(),
                            yes_files,
                            yes_restore,
                            cx,
                        );
                    }
                }
                Err(e) => {
                    this.status = format!("{e}");
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn after_profile_switch(
        &mut self,
        game: &str,
        name: &str,
        restore_deferred: bool,
        rearm_deferred: bool,
        cx: &mut Context<Self>,
    ) {
        self.reload_profiles(cx);
        self.refresh_mods(game, cx);
        self.reload_knob_counts(cx);
        self.reload_game_env(cx);
        self.reload_armed_state(game);
        let mut args = FluentArgs::new();
        args.set("name", name.to_string());
        self.pending_note = Some(Note::Ok(
            self.strings.get_args("gui-profile-using", Some(&args)),
        ));
        if restore_deferred {
            self.queue_profile_restore(game, cx);
        }
        if rearm_deferred {
            self.queue_profile_rearm(game, cx);
        }
        cx.notify();
    }
}
