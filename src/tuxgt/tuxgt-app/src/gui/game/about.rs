use super::*;
use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use tuxgt_core::{data_dir, open_db_shared, set_hidden_override, FluentArgs};

use super::super::theme::{types, TypeStyled as _};
use super::super::widgets;
use super::super::{rt_block, Shell};

impl Shell {
    /// E57: resolved Steam AppID for the hero chip and the Info links —
    /// stored overlay wins, else the Steam game segment (`identity.md`).
    pub(crate) fn resolved_appid(&self, g: &tuxgt_core::GameRow) -> Option<String> {
        self.appid_stored
            .clone()
            .or_else(|| g.resolved_appid().map(str::to_string))
    }

    /// E57: ProtonDB tier chip. Opens the site when we can resolve an appid;
    /// inert otherwise.
    pub(crate) fn protondb_chip(
        &self,
        tier: &str,
        appid: Option<String>,
        cx: &App,
    ) -> impl IntoElement {
        let b = cx.theme();
        let mut args = FluentArgs::new();
        args.set(
            "tier",
            widgets::id_label(widgets::ValKind::Tier, tier, &self.strings),
        );
        let chip = widgets::pill(
            self.strings.get_args("gui-pill-protondb", Some(&args)),
            widgets::protondb_color(tier, cx),
            b.border,
            cx,
        );
        match appid {
            Some(appid) => div()
                .id("hero-protondb")
                .cursor_pointer()
                .on_click(move |_, _, _| open_url(&protondb_url(&appid)))
                .child(chip)
                .into_any_element(),
            None => chip.into_any_element(),
        }
    }

    pub(crate) fn set_hidden_ui(&mut self, game_id: &str, on: bool, cx: &mut Context<Self>) {
        tracing::debug!(action = "set-hidden", game = game_id, on);
        // Hidden narrows at paint, so no base recompute and no full reload:
        // flip the flag on the held entries only.
        match rt_block(async {
            let pool = open_db_shared(&data_dir()).await?;
            set_hidden_override(&pool, game_id, Some(on)).await
        }) {
            Ok(()) => {
                if let Some(e) = self.index.iter_mut().find(|e| e.id == game_id) {
                    e.hidden = on;
                }
                if let Some(g) = self.games.iter_mut().find(|g| g.id == game_id) {
                    g.hidden = on;
                }
            }
            Err(e) => self.status = format!("{e}"),
        }
        cx.notify();
    }

    pub(crate) fn general_tab(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let game_id = self.selected.clone().unwrap_or_default();
        let mut right = v_flex().id("general-r1-right").gap_3();
        if self.is_client_game(&game_id) {
            right = right.child(self.launch_mode_section(view.clone(), cx));
        }
        right = right
            .child(self.wrappers_section(&game_id, view.clone(), cx))
            .child(self.prefix_body(view.clone(), cx));
        v_flex()
            .id("general")
            .gap_3()
            .child(widgets::cols(
                "general-r1",
                220.,
                true,
                vec![
                    (6, self.about_section(view.clone(), cx).into_any_element()),
                    (4, right.into_any_element()),
                ],
            ))
            .child(self.general_advanced(view, cx))
    }

    pub(crate) fn about_section(&self, view: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(g) = self.selected_game() else {
            return div().into_any_element();
        };
        let appid = self.resolved_appid(g);
        let summary = self.proton.get(&g.id).cloned().unwrap_or_default();
        let runner = g
            .proton
            .clone()
            .filter(|s| !s.is_empty())
            .filter(|_| matches!(g.platform.as_deref(), Some("proton" | "wine")));
        // No build/version row: Steam exposes only a numeric buildid and
        // Heroic's merged build channel is usually a build id too - neither
        // is the game version users recognize. A real version row needs its
        // own channel (TASKS: about-game-version).
        widgets::section_card("about", cx)
            .h_full()
            .child(self.about_header(g, view.clone(), cx))
            .children(self.about_id_rows(g, appid.as_deref(), view.clone(), cx))
            .child(self.about_protondb_row(appid.as_deref(), &summary.tier, cx))
            .when_some(runner, |this, value| {
                this.child(self.about_kv(
                    "about-runner",
                    self.strings.get("gui-detect-field-proton"),
                    value,
                    cx,
                ))
            })
            .children(self.about_launch_rows(cx))
            .child(self.about_path_row(g, cx))
            .child(self.hide_row(g, view, cx))
            .into_any_element()
    }

    /// Read-only About key-value shell: key left, value right, hairline
    /// below. Regular body ink, never muted; the row tooltip carries the
    /// full value. Long values truncate against the key.
    pub(crate) fn about_kv_row(
        &self,
        id: &'static str,
        title: String,
        value_el: AnyElement,
        tip: SharedString,
        cx: &App,
    ) -> impl IntoElement {
        v_flex()
            .id(id)
            .w_full()
            .gap_1()
            .px_1()
            .py_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex_shrink_0()
                            .tx(types(cx).label_lg)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .justify_end()
                            .child(value_el),
                    ),
            )
            .child(widgets::row_hairline(cx))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
    }

    /// Read-only About key-value with a plain truncated value.
    pub(crate) fn about_kv(
        &self,
        id: &'static str,
        title: String,
        value: String,
        cx: &App,
    ) -> impl IntoElement {
        let tip = SharedString::from(value.clone());
        let value_el = div()
            .min_w_0()
            .tx(types(cx).body_md)
            .truncate()
            .child(SharedString::from(value))
            .into_any_element();
        self.about_kv_row(id, title, value_el, tip, cx)
    }

    pub(crate) fn about_protondb_row(
        &self,
        appid: Option<&str>,
        tier: &str,
        cx: &App,
    ) -> impl IntoElement {
        let title = self.strings.get("gui-sort-protondb");
        let value = widgets::id_label(widgets::ValKind::Tier, tier, &self.strings);
        let tip = SharedString::from(value.clone());
        let value_el = match appid {
            Some(appid) => self
                .url_btn("about-protondb-link", value, protondb_url(appid), cx)
                .into_any_element(),
            None => div()
                .min_w_0()
                .tx(types(cx).body_md)
                .truncate()
                .child(SharedString::from(value))
                .into_any_element(),
        };
        self.about_kv_row("about-protondb", title, value_el, tip, cx)
            .into_any_element()
    }

    /// Cached store launch config as read-only About rows (Steam launch
    /// options are filled live from the client config at load). Only
    /// present values paint; a game with no stored config shows one
    /// `Launch Config | (none)` row instead of nothing.
    /// Env paints as its own `KEY=VALUE` list, the rest as single lines.
    pub(crate) fn about_launch_rows(&self, cx: &App) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if let Some(cfg) = self.launch_cfg.as_ref() {
            for (id, key, v) in [
                (
                    "about-launch-options",
                    "gui-label-launch-options",
                    &cfg.launch_options,
                ),
                (
                    "about-launch-wrapper",
                    "gui-label-launch-wrapper",
                    &cfg.wrapper,
                ),
            ] {
                if let Some(value) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    rows.push(
                        self.about_kv(id, self.strings.get(key), value.to_string(), cx)
                            .into_any_element(),
                    );
                }
            }
            let lines = cfg.env_lines();
            if !lines.is_empty() {
                rows.push(self.about_env_row(lines, cx).into_any_element());
            }
        }
        if rows.is_empty() {
            rows.push(
                self.about_kv(
                    "about-launch-none",
                    self.strings.get("gui-label-launch-config"),
                    "(none)".to_string(),
                    cx,
                )
                .into_any_element(),
            );
        }
        rows
    }

    /// Launch Env as a sorted `KEY=VALUE` list under its key: three lines
    /// visible with the list's own scroll, hairline below like every row.
    pub(crate) fn about_env_row(&self, lines: Vec<String>, cx: &App) -> impl IntoElement {
        let cap = types(cx).body_md.line * 3. + px(4.);
        v_flex()
            .id("about-launch-env")
            .w_full()
            .gap_1()
            .px_1()
            .py_1()
            .child(
                div()
                    .tx(types(cx).label_lg)
                    .truncate()
                    .child(self.strings.get("gui-label-launch-env")),
            )
            .child(
                div()
                    .id("about-launch-env-list")
                    .w_full()
                    .max_h(cap)
                    .overflow_y_scroll()
                    .track_scroll(&self.about_env_scroll)
                    .on_scroll_wheel(widgets::chain_inner(self.about_env_scroll.clone()))
                    .children(
                        lines
                            .into_iter()
                            .map(|l| widgets::mono(l, cx).into_any_element()),
                    ),
            )
            .child(widgets::row_hairline(cx))
    }

    pub(crate) fn about_header(
        &self,
        g: &tuxgt_core::GameRow,
        view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let id = g.id.clone();
        let tip = id.clone();
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(div().flex_1().min_w_0().child(widgets::section_title(
                self.strings.get("gui-section-about"),
                cx,
            )))
            .child(
                widgets::copy_btn(
                    "copy-tuxgt-id",
                    self.strings.get("gui-action-copy-tuxgt-id"),
                    cx,
                )
                .tooltip(tip)
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        let id = id.clone();
                        view.update(cx, |this, cx| {
                            this.copy_text(id, "gui-status-copied", cx);
                        });
                    }
                }),
            )
    }

    /// One `Steam ID` field: the manager row id as plain text above it, then
    /// a single key-value row that swaps its value cell — store link + Mop
    /// clear button when the overlay is set, the input + Save/Search editor
    /// when unset. Steam-manager rows paint the link only.
    pub(crate) fn about_id_rows(
        &self,
        g: &tuxgt_core::GameRow,
        appid: Option<&str>,
        view: Entity<Self>,
        cx: &App,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if g.manager != "steam" {
            if let Ok(gid) = tuxgt_core::GameId::parse(&g.id) {
                let mut args = FluentArgs::new();
                args.set(
                    "manager",
                    widgets::id_label(widgets::ValKind::Manager, &g.manager, &self.strings),
                );
                rows.push(
                    self.about_kv(
                        "about-manager-id",
                        self.strings
                            .get_args("gui-about-manager-id-key", Some(&args)),
                        gid.game.clone(),
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        if g.manager == "steam" {
            if let Some(appid) = appid {
                let url = steam_store_url(appid);
                rows.push(
                    self.about_kv_row(
                        "about-steam-id",
                        self.strings.get("gui-about-steam-id-key"),
                        self.url_btn("about-steam-id-link", appid.to_string(), url.clone(), cx)
                            .into_any_element(),
                        SharedString::from(url),
                        cx,
                    )
                    .into_any_element(),
                );
            }
            return rows;
        }
        let key = self.strings.get("gui-about-steam-id-key");
        if let Some(appid) = appid {
            let url = steam_store_url(appid);
            let link = self
                .url_btn("about-steam-id-link", appid.to_string(), url.clone(), cx)
                .into_any_element();
            let clear_view = view.clone();
            let clear_game = g.id.clone();
            let tip = self.strings.get("gui-action-clear");
            rows.push(
                self.about_kv_row(
                    "about-steam-id",
                    key,
                    h_flex()
                        .gap_1()
                        .items_center()
                        .justify_end()
                        .child(link)
                        .child(
                            widgets::btn("appid-clear", cx)
                                .secondary()
                                .child(Icon::new(FullIconName::Mop).small())
                                .tooltip(tip)
                                .on_click(move |_, window, cx| {
                                    let game = clear_game.clone();
                                    clear_view.update(cx, |this, cx| {
                                        this.save_appid_ui(&game, "", window, cx);
                                    });
                                }),
                        )
                        .into_any_element(),
                    SharedString::from(url),
                    cx,
                )
                .into_any_element(),
            );
        } else {
            rows.push(
                self.about_kv_row(
                    "about-steam-id",
                    key,
                    self.appid_editor(&g.id, view.clone(), cx),
                    SharedString::from(self.strings.get("gui-note-steam-appid")),
                    cx,
                )
                .into_any_element(),
            );
        }
        rows
    }

    pub(crate) fn about_path_row(&self, g: &tuxgt_core::GameRow, cx: &App) -> impl IntoElement {
        // The launcher log lives at `<managed>/runtime/tuxgt-launcher.log`,
        // so the managed dir (not top-level `games/`) is one click away.
        let managed = tuxgt_core::GameId::parse(&g.id)
            .ok()
            .map(|gid| tuxgt_core::game_dir(&data_dir(), &gid).display().to_string());
        h_flex()
            .id("about-paths")
            .gap_1()
            .flex_wrap()
            .child(self.path_btn(
                "about-exe",
                "gui-path-exe",
                widgets::OpenKind::File,
                g.exe_path.as_deref(),
                cx,
            ))
            .child(self.path_btn(
                "about-install",
                "gui-path-install",
                widgets::OpenKind::Folder,
                g.install_dir.as_deref(),
                cx,
            ))
            .child(self.path_btn(
                "about-prefix",
                "gui-path-prefix",
                widgets::OpenKind::Folder,
                g.prefix_path.as_deref(),
                cx,
            ))
            .child(self.path_btn(
                "about-managed",
                "gui-path-managed",
                widgets::OpenKind::Folder,
                managed.as_deref(),
                cx,
            ))
    }

    pub(crate) fn path_btn(
        &self,
        id: &'static str,
        label: &'static str,
        kind: widgets::OpenKind,
        path: Option<&str>,
        cx: &App,
    ) -> impl IntoElement {
        let label = self.strings.get(label);
        let target = path.filter(|s| !s.is_empty()).map(str::to_string);
        let mut btn = widgets::open_btn(id, kind, label, target.clone(), cx);
        if let Some(tip) = target {
            btn = btn.tooltip(tip);
        }
        btn
    }

    pub(crate) fn url_btn(
        &self,
        id: &'static str,
        label: String,
        url: String,
        cx: &App,
    ) -> impl IntoElement {
        widgets::open_btn(id, widgets::OpenKind::Link, label, Some(url.clone()), cx).tooltip(url)
    }
}
