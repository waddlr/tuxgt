use gpui_kit::component::input::Input;
use gpui_kit::component::{h_flex, v_flex, Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, FluentArgs};

use super::super::theme::types;
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    /// Unset-only Steam-AppID editor for the About `Steam ID` row on
    /// Heroic/manual games: input + Save/Search, plus search status and
    /// hits. Bare controls — the About key-value row owns the key.
    pub(crate) fn appid_editor(
        &self,
        game_id: &str,
        view: Entity<Self>,
        cx: &App,
    ) -> AnyElement {
        // No early return: the caller only paints this when unset.
        let searching = self.appid_searching;
        let searched = self.appid_searched;
        let hits = self.appid_hits.clone();
        let save_view = view.clone();
        let search_view = view.clone();
        let hits_view = view.clone();
        let save_game = game_id.to_string();
        let search_game = save_game.clone();
        let hits_game = save_game.clone();
        let show_empty = !searching && searched && hits.is_empty();
        let show_hits = !searching && !hits.is_empty();
        let controls = v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div().flex_1().min_w_0().child(Styled::h(
                            Input::new(&self.appid_input)
                                .xsmall()
                                .text_size(types(cx).body_md.size)
                                .cleanable(true),
                            types(cx).control_h,
                        )),
                    )
                    .child(
                        widgets::btn("appid-save", cx)
                            .child(widgets::blabel(self.strings.get("gui-action-save"), cx))
                            .on_click(move |_, window, cx| {
                                let value =
                                    save_view.read(cx).appid_input.read(cx).value().to_string();
                                let game = save_game.clone();
                                save_view.update(cx, |this, cx| {
                                    this.save_appid_ui(&game, &value, window, cx);
                                });
                            }),
                    )
                    .child(
                        widgets::btn("appid-search", cx)
                            .child(widgets::blabel(self.strings.get("gui-action-search"), cx))
                            .on_click(move |_, _, cx| {
                                let query = search_view
                                    .read(cx)
                                    .appid_input
                                    .read(cx)
                                    .value()
                                    .to_string();
                                let game = search_game.clone();
                                search_view.update(cx, |this, cx| {
                                    this.search_appid_ui(&game, &query, cx);
                                });
                            }),
                    ),
            )
            .when(searching, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-status-appid-searching"),
                    cx,
                ))
            })
            .when(show_empty, |this| {
                this.child(widgets::muted(
                    self.strings.get("gui-empty-appid-search"),
                    cx,
                ))
            })
            .when(show_hits, |this| {
                this.child(v_flex().gap_1().children(hits.iter().map(|(appid, name)| {
                    let hits_view = hits_view.clone();
                    let hits_game = hits_game.clone();
                    let hit_appid = appid.clone();
                    let hit_name = name.clone();
                    widgets::btn(SharedString::from(format!("appid-hit-{appid}")), cx)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(widgets::blabel(hit_name, cx)),
                        )
                        .child(widgets::muted(hit_appid.clone(), cx))
                        .on_click(move |_, window, cx| {
                            let game = hits_game.clone();
                            let appid = hit_appid.clone();
                            hits_view.update(cx, |this, cx| {
                                this.save_appid_ui(&game, &appid, window, cx);
                            });
                        })
                })))
            });
        controls.into_any_element()
    }

    /// Steam name-search for the Heroic/manual AppID row (unset only). The
    /// query rides the existing AppID input, falling back to the selected
    /// game's title when it is blank; no new field. Blank titles never
    /// spawn — status explains, no network. Otherwise the blocking
    /// store-search call lives in `background_spawn` (settings.rs
    /// secret-save precedent) and completion is dropped when `selected`
    /// moved on, like `save_appid_ui`.
    pub(crate) fn search_appid_ui(&mut self, game: &str, query: &str, cx: &mut Context<Self>) {
        let q = query.trim().to_string();
        let q = if q.is_empty() {
            self.selected_game()
                .map(|g| g.display_name().to_string())
                .unwrap_or_default()
        } else {
            q
        };
        if q.is_empty() {
            self.status = self.strings.get("gui-empty-appid-search");
            cx.notify();
            return;
        }
        let game = game.to_string();
        tracing::debug!(action = "search-appid", game = game.as_str());
        self.appid_searching = true;
        self.appid_searched = true;
        self.appid_hits = Default::default();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let found = tuxgt_core::search_steam_by_name(&q)?;
                    if found.is_empty() {
                        if let Some(short) = tuxgt_core::shorten_store_query(&q) {
                            return tuxgt_core::search_steam_by_name(&short);
                        }
                    }
                    Ok(found)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(game.as_str()) {
                    return;
                }
                match result {
                    Ok(found) => {
                        tracing::debug!(action = "search-appid", game = game.as_str(), hits = found.len(), outcome = "done");
                        this.appid_searching = false;
                        this.appid_hits = found
                            .into_iter()
                            .map(|hit| (hit.appid.to_string(), hit.name))
                            .collect::<Vec<_>>()
                            .into_boxed_slice();
                    }
                    Err(e) => {
                        this.appid_searching = false;
                        this.appid_searched = false;
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-appid", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// E44: store (or clear, on empty) the Steam-AppID overlay, then refresh
    /// metadata + hero art. Core validation errors land in the status line.
    pub(crate) fn save_appid_ui(
        &mut self,
        game: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let game = game.to_string();
        tracing::debug!(action = "save-appid", game = game.as_str(), cleared = value.trim().is_empty());
        let owned = game.clone();
        let value = value.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let trimmed = value.trim();
                        let appid = if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed)
                        };
                        tuxgt_core::set_steam_appid(&pool, &owned, appid).await?;
                        tuxgt_core::steam_appid_of(&pool, &owned).await
                    })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.selected.as_deref() != Some(game.as_str()) {
                    return;
                }
                match result {
                    Ok(Some(appid)) => {
                        let mut args = FluentArgs::new();
                        args.set("appid", appid.clone());
                        this.appid_stored = Some(appid);
                        // Held library rows are scan-loaded: keep the
                        // overlay on the selected row in sync so
                        // `resolved_appid` never serves a stale value.
                        if let Some(g) = this.games.iter_mut().find(|g| g.id == game) {
                            g.steam_appid = this.appid_stored.clone();
                        }
                        this.status = this.strings.get_args("gui-status-appid-set", Some(&args));
                        this.appid_hits = Default::default();
                        this.appid_searched = false;
                        let input = this.appid_input.clone();
                        input.update(cx, |inp, cx| {
                            inp.set_value(String::new(), window, cx);
                        });
                        // An AppID change moves the game on SteamGridDB: bust
                        // the AppID-dependent art and refresh metadata,
                        // which re-renders on completion.
                        tuxgt_core::bust_game_art_for_appid(&tuxgt_core::data_dir(), &game);
                        this.reload_metadata(true, cx);
                    }
                    Ok(None) => {
                        this.appid_stored = None;
                        if let Some(g) = this.games.iter_mut().find(|g| g.id == game) {
                            g.steam_appid = None;
                        }
                        this.appid_hits = Default::default();
                        this.appid_searched = false;
                        this.status = this.strings.get("gui-status-appid-cleared");
                        let input = this.appid_input.clone();
                        input.update(cx, |inp, cx| {
                            inp.set_value(String::new(), window, cx);
                        });
                        tuxgt_core::bust_game_art_for_appid(&tuxgt_core::data_dir(), &game);
                        this.reload_metadata(true, cx);
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-appid", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
