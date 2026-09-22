use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, ActiveTheme, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, FluentArgs};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    pub(crate) fn extra_exes_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let b = cx.theme();
        let game_id = self.selected.clone().unwrap_or_default();
        let primary = self
            .detect
            .iter()
            .find(|s| s.key == "exe")
            .and_then(|s| s.effective())
            .unwrap_or("—")
            .to_string();
        widgets::section_card("extra-exes", cx)
            .child({
                let tip = SharedString::from(self.strings.get("gui-tip-extra-exes"));
                div()
                    .id("extra-exes-title")
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(widgets::section_header(
                        "extra-exes-head",
                        self.strings.get("gui-section-extra-exes"),
                        self.page_scroll.bounds().size.width,
                        vec![widgets::SectionAction::new(
                            "extra-exe-add",
                            self.strings.get("gui-action-add"),
                            {
                                let add_view = view.clone();
                                let add_game = game_id.clone();
                                move |_, _, cx| {
                                    let value = add_view
                                        .read(cx)
                                        .extra_exe_input
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    add_view.update(cx, |this, cx| {
                                        this.add_extra_exe_ui(&add_game, value.trim(), cx);
                                    });
                                }
                            },
                        )],
                        cx,
                    ))
            })
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(widgets::pill(
                        self.strings.get("gui-label-primary-exe"),
                        cx.theme().muted_foreground,
                        b.border,
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .tx(types(cx).label_lg)
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(SharedString::from(primary)),
                    ),
            )
            .when(self.extras.is_empty(), |this| {
                this.child(widgets::placeholder_note(
                    self.strings.get("gui-empty-no-extra-exes"),
                    cx,
                ))
            })
            .children(
                self.extras
                    .iter()
                    .map(|exe| self.extra_exe_row(exe, game_id.as_str(), view.clone(), cx)),
            )
            .child(self.extra_exe_add_row(game_id.as_str(), view, cx))
    }

    pub(crate) fn extra_exe_row(
        &self,
        exe: &str,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let game = game_id.to_string();
        let doomed = exe.to_string();
        h_flex()
            .gap_1()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .tx(types(cx).label_lg)
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(SharedString::from(exe.to_string())),
            )
            .child(widgets::destroy_btn(
                SharedString::from(format!("extra-rm-{exe}")),
                self.strings.get("gui-action-remove"),
                {
                    let game = game.clone();
                    let doomed = doomed.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.remove_extra_exe_ui(&game, &doomed, cx);
                        });
                    }
                },
                cx,
            ))
    }

    /// R62 path input row: type a path and use the section header's Add, or
    /// pick one with Browse (which adds at once through the core writer).
    pub(crate) fn extra_exe_add_row(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let browse_game = game_id.to_string();
        h_flex()
            .gap_1()
            .items_center()
            .child(
                div().flex_1().min_w_0().child(Styled::h(
                    Input::new(&self.extra_exe_input)
                        .xsmall()
                        .text_size(types(cx).body_md.size)
                        .cleanable(true),
                    types(cx).control_h,
                )),
            )
            .child(
                widgets::btn("extra-exe-browse", cx)
                    .child(widgets::blabel(self.strings.get("gui-action-browse"), cx))
                    .on_click(move |_, _, cx| {
                        pick_extra_exe_path(view.clone(), browse_game.clone(), cx)
                    }),
            )
    }

    /// R62: add one extra correlator exe, then the same wired sequence as
    /// `set_override` → `sync_session`. Empty input is a no-op; the core
    /// writer canonicalizes, skips primary-equal adds, and refuses cross-game
    /// collisions naming the owner with nothing written.
    pub(crate) fn add_extra_exe_ui(&mut self, game: &str, exe: &str, cx: &mut Context<Self>) {
        let exe = exe.trim().to_string();
        if exe.is_empty() {
            return;
        }
        let game = game.to_string();
        tracing::debug!(action = "add-extra-exe", game = game.as_str(), exe = exe.as_str());
        let owned = game.clone();
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
                            tuxgt_core::add_extra_exe(&pool, &owned, &exe),
                        )
                        .await?;
                        let rows = tuxgt_core::list_extra_exes(&pool, &owned).await?;
                        Ok(rows)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(game.as_str()) || this.nav != super::Nav::Game {
                    return;
                }
                match result {
                    Ok(rows) => {
                        this.extras = rows.into_boxed_slice();
                        this.extra_exe_for = Some(game.clone());
                        this.session_payload
                            .insert(game.clone(), super::load_payload(&game));

                        this.status = this.strings.get("gui-status-extra-added");
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-extra", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// R62: remove one extra correlator exe, then `sync_session` like every
    /// other correlator writer. Unknown keys are a core no-op that still
    /// refreshes the list.
    pub(crate) fn remove_extra_exe_ui(&mut self, game: &str, exe: &str, cx: &mut Context<Self>) {
        let exe = exe.to_string();
        tracing::debug!(action = "remove-extra-exe", game, exe = exe.as_str());
        let game = game.to_string();
        let owned = game.clone();
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
                            tuxgt_core::remove_extra_exe(&pool, &owned, &exe),
                        )
                        .await?;
                        let rows = tuxgt_core::list_extra_exes(&pool, &owned).await?;
                        Ok(rows)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(game.as_str()) || this.nav != super::Nav::Game {
                    return;
                }
                match result {
                    Ok(rows) => {
                        this.extras = rows.into_boxed_slice();
                        this.extra_exe_for = Some(game.clone());
                        this.session_payload
                            .insert(game.clone(), super::load_payload(&game));

                        this.status = this.strings.get("gui-status-extra-removed");
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-extra", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn pick_extra_exe_path(view: Entity<Shell>, game: String, cx: &mut App) {
    let prompt = view.read(cx).strings.get("gui-prompt-extra-exe");
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
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
                this.add_extra_exe_ui(&game, &path.to_string_lossy(), cx);
            });
        });
    })
    .detach();
}
