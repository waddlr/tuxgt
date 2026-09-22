use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, IconName};
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, FluentArgs};

use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn redetect_row(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let game = game_id.to_string();
        if self.redetect_confirm {
            let doomed: Vec<(String, String)> = self
                .detect
                .iter()
                .filter_map(|s| s.override_.clone().map(|v| (s.key.to_string(), v)))
                .collect();
            let confirm_view = view.clone();
            let confirm_game = game.clone();
            let cancel_view = view.clone();
            v_flex()
                .id("redetect-confirm")
                .gap_1()
                .child(widgets::muted(
                    self.strings.get("gui-note-redetect-confirm"),
                    cx,
                ))
                .children(
                    doomed
                        .into_iter()
                        .map(|(k, v)| widgets::mono(format!("{k}  {v}"), cx)),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            widgets::btn("redetect-yes", cx)
                                .danger()
                                .child(widgets::blabel(
                                    self.strings.get("gui-action-confirm-wipe"),
                                    cx,
                                ))
                                .on_click(move |_, _, cx| {
                                    confirm_view.update(cx, |this, cx| {
                                        this.run_redetect_ui(&confirm_game, cx);
                                    });
                                }),
                        )
                        .child(
                            widgets::btn("redetect-no", cx)
                                .ghost()
                                .child(widgets::blabel(self.strings.get("gui-action-cancel"), cx))
                                .on_click(move |_, _, cx| {
                                    cancel_view.update(cx, |this, cx| {
                                        this.redetect_confirm = false;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .into_any_element()
        } else {
            widgets::btn("redetect", cx)
                .secondary()
                .child(widgets::bicon(IconName::RotateCw))
                .child(widgets::blabel(
                    self.strings.get("gui-action-force-redetect"),
                    cx,
                ))
                .tooltip(self.strings.get("gui-tip-redetect"))
                .on_click(move |_, _, cx| {
                    let click_game = game.clone();
                    view.update(cx, |_this, cx| {
                        cx.spawn(async move |this, cx| {
                            let bg_game = click_game.clone();
                            let snap = cx
                                .background_spawn(async move {
                                    rt_block(async {
                                        let pool = open_db_shared(&data_dir()).await?;
                                        Ok(tuxgt_core::detection_snapshot(&pool, &bg_game).await?)
                                    })
                                })
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                match snap {
                                    Ok(snap) => {
                                        if this.selected.as_deref() != Some(click_game.as_str()) {
                                            return;
                                        }
                                        this.detect = snap.into_boxed_slice();
                                        if this.detect.iter().any(|s| s.override_.is_some()) {
                                            this.redetect_confirm = true;
                                            this.override_edit = None;
                                        } else {
                                            this.run_redetect_ui(&click_game, cx);
                                        }
                                    }
                                    Err(e) => {
                                        let mut args = FluentArgs::new();
                                        args.set("error", e.to_string());
                                        this.status = this
                                            .strings
                                            .get_args("gui-status-err-redetect", Some(&args));
                                    }
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    });
                })
                .into_any_element()
        }
    }

    pub(crate) fn save_override_ui(
        &mut self,
        game: &str,
        field: &'static str,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let game = game.to_string();
        let owned = game.clone();
        let value = value.filter(|v| !v.is_empty());
        self.override_edit = None;
        tracing::debug!(action = "save-override", game = game.as_str(), field, value = ?value);
        self.redetect_confirm = false;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let dir = data_dir();
                        let pool = open_db_shared(&dir).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(
                            &pool,
                            &dir,
                            &host,
                            &owned,
                            tuxgt_core::set_override(&pool, &owned, field, value.as_deref()),
                        )
                        .await?;
                        let snap = tuxgt_core::detection_snapshot(&pool, &owned).await?;
                        Ok(snap)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(snap) => {
                        // Game-scoped: other pages reload on entry. The
                        // platform/api change also feeds the stored base.
                        if this.selected.as_deref() == Some(game.as_str())
                            && this.nav == super::Nav::Game
                        {
                            this.detect = snap.into_boxed_slice();
                            this.detect_for = Some(game.clone());
                            this.reload_selected_row();
                            this.rebuild_base();
                        }
                        let mut args = FluentArgs::new();
                        args.set("field", field);
                        this.status = this
                            .strings
                            .get_args("gui-status-override-set", Some(&args));
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-err-override", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// R62 Launch-tab extra-exe list editor. The primary exe is a read-only
    /// snapshot (its editors stay the override rows above); extras add/remove
    /// through the core writer, then the same `sync_session` sequence as
    /// `set_override`. A refused add surfaces the core error naming the
    /// owning game with nothing written. Add/remove are non-destructive
    /// correlator keys, so no E34 confirm.
    pub(crate) fn run_redetect_ui(&mut self, game: &str, cx: &mut Context<Self>) {
        let game = game.to_string();
        let owned = game.clone();
        tracing::debug!(action = "run-redetect", game = game.as_str());
        self.redetect_confirm = false;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let dir = data_dir();
                        let pool = open_db_shared(&dir).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(
                            &pool,
                            &dir,
                            &host,
                            &owned,
                            tuxgt_core::detect_one(
                                &pool,
                                &owned,
                                tuxgt_core::DetectOpts {
                                    force: true,
                                    yes: true,
                                },
                            ),
                        )
                        .await?;
                        let snap = tuxgt_core::detection_snapshot(&pool, &owned).await?;
                        Ok(snap)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(snap) => {
                        // Game-scoped: other pages reload on entry. The
                        // redetect can change platform/api, which feeds the
                        // stored base.
                        if this.selected.as_deref() == Some(game.as_str())
                            && this.nav == super::Nav::Game
                        {
                            this.detect = snap.into_boxed_slice();
                            this.detect_for = Some(game.clone());
                            this.reload_selected_row();
                            this.rebuild_base();
                        }
                        let mut args = FluentArgs::new();
                        args.set("game", game.clone());
                        this.status = this.strings.get_args("gui-status-redetected", Some(&args));
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-err-redetect", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn pick_override_path(
    view: Entity<Shell>,
    game: String,
    field: &'static str,
    dir: bool,
    cx: &mut App,
) {
    let mut args = FluentArgs::new();
    args.set("field", field);
    let prompt = view
        .read(cx)
        .strings
        .get_args("gui-prompt-override", Some(&args));
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: !dir,
        directories: dir,
        multiple: false,
        prompt: Some(prompt.into()),
    });
    cx.spawn(async move |cx| {
        let picked = rx.await.ok().and_then(|r| r.ok()).flatten();
        let Some(paths) = picked else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        let _ = cx.update(|cx| {
            view.update(cx, |this, cx| {
                this.save_override_ui(&game, field, Some(path.to_string_lossy().into_owned()), cx);
            });
        });
    })
    .detach();
}
