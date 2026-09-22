mod env;
mod scan;

use gpui_kit::*;

use tuxgt_core::{
    all_tools, data_dir, detection_snapshot, game_show, list_extra_exes, open_db_shared,
    FluentArgs, PluginHost, KEY_SOURCES,
};

use super::theme::{FontScale, ThemeId};

use super::*;

impl Shell {
    pub(crate) fn fetch_for_tab(&mut self, cx: &mut Context<Self>) {
        // Game page only: Library/Settings rescans must not re-inflate
        // game-scoped rows the transitions just dropped.
        if self.nav != Nav::Game {
            return;
        }
        self.reload_appid(cx);
        self.ensure_detection(cx);
        self.reload_launch_state(cx);
        self.reload_metadata(false, cx);
        // Env rows load on the Env tab; other tabs need just the hero counts.
        // Mods rows load only on the Mods tab.
        if self.game_tab == GameTab::Env {
            self.reload_game_env(cx);
        } else {
            self.reload_knob_counts(cx);
        }
        if self.game_tab == GameTab::Mods {
            if let Some(id) = self.selected.clone() {
                self.enter_mods_tab(&id);
                self.refresh_mod_extra(&id, cx, false);
            }
        }
    }

    pub(crate) fn reload_metadata(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let host = PluginHost::load()?;
                        let _ = game_show(&pool, &host, &owned, refresh).await?;
                        Ok(())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) {
                    return;
                }
                if let Err(e) = result {
                    this.status = format!("{e}");
                    cx.notify();
                    return;
                }
                // Selected game only: the Game page never holds
                // library-wide metadata. The single-entry maps assign only
                // on the Game page (a late completion must not wipe the
                // Library's full maps); art + base are page-safe everywhere.
                if let Some(g) = this.selected_game().cloned() {
                    if this.nav == Nav::Game {
                        let meta = load_metadata_for(&id, &g);
                        this.set_game_metadata(&id, meta);
                    }
                    // Fresh GridDB URLs may name new art: re-render.
                    this.spawn_art_render_one(&id, cx);
                    if refresh {
                        // Refreshed tiers/AWACY feed the sidebar protondb /
                        // tier-sort / awacy base on every page.
                        this.rebuild_base();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Detection snapshot for `selected`, unless it is already loaded for it.
    pub(crate) fn ensure_detection(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_none() || self.detect_for == self.selected {
            return;
        }
        self.reload_detection(cx);
    }

    /// E57: stored Steam-AppID overlay for `selected`. Loaded for every tab
    /// so the hero chip and the Info links resolve before the Launch editor.
    pub(crate) fn reload_appid(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.appid_stored = None;
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        tuxgt_core::steam_appid_of(&pool, &owned).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) || this.nav != Nav::Game {
                    return;
                }
                match result {
                    Ok(appid) => this.appid_stored = appid,
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// E44: Launch-tab per-game state — stored overlay wrappers, fetched for
    /// the selected row.
    pub(crate) fn reload_launch_state(&mut self, cx: &mut Context<Self>) {
        self.spawn_launch_state(false, cx);
    }

    /// Post-op refresh: same reload with a live store read, and a reload
    /// failure toasts instead of overwriting the op's own status report
    /// (the op already reported; the reload runs after it and must not
    /// replace its verdict).
    pub(crate) fn refresh_launch_state(&mut self, cx: &mut Context<Self>) {
        self.spawn_launch_state(true, cx);
    }

    fn spawn_launch_state(&mut self, post_op: bool, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.wrappers = Default::default();
            self.launch_cfg = None;
            self.about_env_scroll = ScrollHandle::new();
            return;
        };
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async { fetch_launch_state(&owned, post_op).await })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(id.as_str()) || this.nav != Nav::Game {
                    return;
                }
                match result {
                    Ok((wrappers, needs, cfg)) => {
                        this.wrappers = wrappers.into_boxed_slice();
                        this.launch_needs = needs;
                        this.launch_cfg = Some(cfg);
                    }
                    Err(e) if post_op => {
                        this.pending_note = Some(Note::Err(format!("{e}")));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Secret-manager set/unset per key source (B07.5). Blocking keyring
    /// reads run on the background thread; errors surface in status.
    pub(crate) fn reload_secrets(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    KEY_SOURCES
                        .iter()
                        .map(|s| {
                            tuxgt_core::secret_manager_get(s).map(|v| (s.to_string(), v.is_some()))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(rows) => this.secret_states = rows.into_iter().collect(),
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-secrets", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// E61: unpack tools for the Settings Host tools block. `all_tools()`
    /// shells out to four binaries, so it runs on the background thread once
    /// per Settings entry; the paint path only reads `self.tools`.
    pub(crate) fn reload_tools(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let tools = cx.background_spawn(async { all_tools() }).await;
            let _ = this.update(cx, |this, cx| {
                if this.nav != Nav::Settings {
                    return;
                }
                this.tools = tools.into_boxed_slice();
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn reload_detection(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            self.detect = Default::default();
            self.extras = Default::default();
            self.extra_exe_for = None;
            self.override_edit = None;
            self.redetect_confirm = false;
            return;
        };
        self.override_edit = None;
        self.redetect_confirm = false;
        let owned = id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let pool = open_db_shared(&data_dir()).await?;
                        let snap = detection_snapshot(&pool, &owned).await?;
                        let extras = list_extra_exes(&pool, &owned).await?;
                        Ok((snap, extras))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() == Some(id.as_str()) && this.nav == Nav::Game {
                    match result {
                        Ok((snap, extras)) => {
                            this.detect = snap.into_boxed_slice();
                            this.detect_for = Some(id.clone());
                            this.extras = extras.into_boxed_slice();
                            this.extra_exe_for = Some(id.clone());
                        }
                        Err(e) => {
                            let mut args = FluentArgs::new();
                            args.set("error", e.to_string());
                            this.status =
                                this.strings.get_args("gui-status-err-detect", Some(&args));
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn set_theme(&mut self, id: ThemeId, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-theme", theme = ?id);
        self.prefs.theme = id.id().into();
        self.prefs.save();
        theme::apply(cx, Some(window), id, self.prefs.scale());
        cx.notify();
    }

    pub(crate) fn set_scale(
        &mut self,
        scale: FontScale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "set-scale", scale = ?scale);
        self.prefs.font_scale = scale.id().into();
        self.prefs.save();
        theme::apply(cx, Some(window), self.prefs.theme_id(), scale);
        cx.notify();
    }

    pub(crate) fn set_debug_log(&mut self, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-debug-log", on);
        self.prefs.debug_log = on;
        self.prefs.save();
        crate::log::set_debug(on);
        cx.notify();
    }
}

/// Shared loader behind `reload_launch_state` / `refresh_launch_state`.
/// `live` re-reads the store file itself: scan-time rows go stale the
/// moment Apply/Restore changes the client config (Heroic has no row
/// fallback at all; a Steam row can hold pre-Apply options).
async fn fetch_launch_state(
    game_id: &str,
    live: bool,
) -> Result<
    (
        Vec<String>,
        tuxgt_core::LaunchNeeds,
        tuxgt_core::GameLaunchConfig,
    ),
    tuxgt_core::Error,
> {
    let pool = open_db_shared(&data_dir()).await?;
    let wrappers = tuxgt_core::game_wrappers(&pool, game_id).await?;
    let needs = tuxgt_core::game_launch_needs(&pool, &data_dir(), game_id).await?;
    let mut cfg = tuxgt_core::game_launch_config(&pool, game_id).await?;
    // Steam keeps launch options in its own client config,
    // never in our row: read them live for the reference.
    let blank = cfg
        .launch_options
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none();
    if live {
        // Post-op: the store file just changed, so the row (and the
        // blank-only fallback below) is pre-op truth. A missing live
        // entry keeps the row instead of blanking the reference.
        if let Ok(gid) = tuxgt_core::GameId::parse(game_id) {
            match gid.manager.as_str() {
                "steam" => {
                    if let Some(opts) =
                        tuxgt_core::provider::steam::steam_launch_options(&gid.game)
                    {
                        cfg.launch_options = Some(opts);
                    }
                }
                "heroic" => {
                    // Apply writes wrapperOptions (and env), never
                    // launcherArgs: sync the fields Apply touches.
                    if let Some(snap) =
                        tuxgt_core::provider::heroic::heroic_live_config(&gid.game)
                    {
                        if snap.launch_options.is_some() {
                            cfg.launch_options = snap.launch_options;
                        }
                        if snap.env.is_some() {
                            cfg.env = snap.env;
                        }
                        if snap.wrapper.is_some() {
                            cfg.wrapper = snap.wrapper;
                        }
                    }
                }
                _ => {}
            }
        }
    } else if blank {
        if let Ok(gid) = tuxgt_core::GameId::parse(game_id) {
            if gid.manager == "steam" {
                cfg.launch_options = tuxgt_core::provider::steam::steam_launch_options(&gid.game);
            }
        }
    }
    Ok((wrappers, needs, cfg))
}
