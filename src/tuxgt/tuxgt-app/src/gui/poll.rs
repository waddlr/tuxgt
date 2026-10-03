use gpui_kit::*;

use tuxgt_core::{
    check_app_update, config_dir, data_dir, find_mod, open_db_shared, AppUpdateStatus, FluentArgs,
};

use super::notice::NoticeKind;
use super::*;

/// E104: one catalog poll outcome. `available` = Mod ids whose catalog
/// check says Available; `installed_zero` = the subset with no installs
/// (Settings-side cards); `per_game` = (game, stale-count) pairs for
/// per-game cards. Unknown never appears here. `app` rides the same run
/// (no new timer): the self-update verdict, or `None` when the check
/// itself failed to run.
#[derive(Default)]
pub(crate) struct PollReport {
    pub(crate) available: std::collections::HashSet<String>,
    pub(crate) installed_zero: std::collections::HashSet<String>,
    pub(crate) per_game: Vec<(String, usize)>,
    pub(crate) app: Option<AppUpdateStatus>,
}

/// E104: run `check_catalog_update` for every cache row with a recorded
/// version (non-empty sha), then compare every manifest provenance
/// locally. At most one network hit per Mod; tab paint never hits the
/// network.
async fn poll_catalog(
    pool: &tuxgt_core::SqlitePool,
    data: &std::path::Path,
    cfg: &std::path::Path,
) -> tuxgt_core::Result<PollReport> {
    use tuxgt_core::CatalogStatus;
    let rows = tuxgt_core::mod_cache_rows(pool).await?;
    let mut available = std::collections::HashSet::new();
    let mut installed_zero = std::collections::HashSet::new();
    for r in &rows {
        if r.asset_sha256.is_empty() {
            continue;
        }
        match tuxgt_core::check_catalog_update(pool, data, cfg, &r.id).await? {
            CatalogStatus::Available { .. } => {
                available.insert(r.id.clone());
                if r.installed == 0 {
                    installed_zero.insert(r.id.clone());
                }
            }
            CatalogStatus::UpToDate
            | CatalogStatus::Unknown { .. }
            | CatalogStatus::NeedsFiles { .. } => {}
        }
    }
    // Per-game staleness is a local compare: manifest provenance sha vs
    // the cache row sha. No network here. `check_catalog_update` only
    // writes last_check/status, so reuse the pre-check snapshot.
    let sha_of: std::collections::HashMap<&str, &str> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.asset_sha256.as_str()))
        .collect();
    let games = tuxgt_core::list_games(pool, None, None, None).await?;
    let mut per_game = Vec::new();
    for g in &games {
        let manifests = tuxgt_core::game_manifests(data, &g.id).unwrap_or_default();
        let stale = super::poll_attention::stale_instance_count(
            manifests
                .iter()
                .map(|m| (m.instance.as_str(), m.provenance.asset_sha256.as_str())),
            &sha_of,
        );
        if stale > 0 {
            per_game.push((g.id.clone(), stale));
        }
    }
    // Self-update rides the same run (no new timer): one latest-release
    // hit next to the catalog hits. The check is infallible (failures are
    // Unknown), so it cannot fail the poll.
    let app = check_app_update().await;
    Ok(PollReport {
        available,
        installed_zero,
        per_game,
        app: Some(app),
    })
}

impl Shell {
    pub(crate) fn maybe_poll_updates(&mut self, cx: &mut Context<Self>) {
        self.maybe_poll_updates_active(true, cx);
    }

    /// E104: `maybe_poll_updates` with an explicit active gate.
    pub(crate) fn maybe_poll_updates_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            return;
        }
        // R12: a hidden window is minimal background work. The catalog poll
        // is the app's periodic network work, so it waits for a Show; the
        // next open reads the same due-ness and runs it then.
        if self.hide_state.is_hidden() {
            return;
        }
        if self.update_polling {
            return;
        }
        if self
            .update_last_poll
            .is_some_and(|t| t.elapsed().as_secs() < Self::UPDATE_POLL_SECS)
        {
            return;
        }
        self.update_polling = true;
        let poll_started = std::time::Instant::now();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let cfg = config_dir();
                        poll_catalog(&pool, &data, &cfg).await
                    })
                })
                .await;
            let _ = this.update(cx, move |this, cx| {
                this.update_polling = false;
                let again = match result {
                    Ok(report) => this.apply_poll_report(report, poll_started, cx),
                    Err(e) => {
                        tracing::warn!(error = %e, "catalog poll failed");
                        // A recount failed during this attempt. An older
                        // stamp must not retry, or the next poll never stops.
                        super::poll_attention::recount_failed_since(
                            &this.game_attention_settled,
                            poll_started,
                        )
                    }
                };
                if again {
                    // A recount failed after this poll started. Do not
                    // freeze the 2h timer on that snapshot.
                    this.update_last_poll = None;
                    this.maybe_poll_updates(cx);
                    cx.notify();
                    return;
                }
                this.update_last_poll = Some(std::time::Instant::now());
                // Re-arm one 2h one-shot. The timer NEVER starts a run:
                // it clears due-ness so the next active paint
                // (`poll_on_focus_return`) starts it. Unfocused ticks
                // thus wait for focus return instead of polling in the
                // background. The generation guard keeps one re-arm:
                // a stale timer (e.g. from before a Settings Update
                // refresh) never clears a newer run's due-ness.
                this.update_poll_gen = this.update_poll_gen.wrapping_add(1);
                let gen = this.update_poll_gen;
                cx.spawn(async move |view_cx, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(Self::UPDATE_POLL_SECS))
                        .await;
                    let _ = view_cx.update(cx, |this: &mut Shell, _cx| {
                        if this.update_poll_gen == gen {
                            this.update_last_poll = None;
                        }
                    });
                })
                .detach();
                cx.notify();
            });
        })
        .detach();
    }

    /// E104: fold one poll report into Attention + page badges. Unknown is
    /// never stored or carded — it stays on the page card only.
    pub(crate) fn apply_poll_report(
        &mut self,
        report: PollReport,
        poll_started: std::time::Instant,
        cx: &mut Context<Self>,
    ) -> bool {
        let (per_game, again) = super::poll_attention::per_game_respecting_settle(
            &report.per_game,
            &self.game_attention_settled,
            poll_started,
        );
        tracing::info!(
            available = report.available.len(),
            games = report.per_game.len(),
            "catalog poll done"
        );
        // Catalog-level cards: Available + installed == 0.
        for id in &report.available {
            if report.installed_zero.contains(id) {
                let label = self.catalog_label(id);
                let mut args = FluentArgs::new();
                args.set("label", label);
                let text = self
                    .strings
                    .get_args("gui-notice-update-catalog", Some(&args));
                let key = format!("catalog:{id}");
                self.emit_attention(&key, NoticeKind::Warn, text, cx);
            }
        }
        // Per-game cards: one per game with ≥1 stale instance.
        // A recount newer than this poll replaces that game's row.
        for (game, n) in &per_game {
            let display = self.game_display(game);
            let mut args = FluentArgs::new();
            args.set("display", display);
            args.set("count", n.to_string());
            let text = self.strings.get_args("gui-notice-update-game", Some(&args));
            let key = format!("game:{game}");
            self.emit_attention(&key, NoticeKind::Warn, text, cx);
        }
        // Drop cards whose condition cleared (UpToDate / gone).
        let mut keep = std::collections::HashSet::new();
        for id in &report.available {
            if report.installed_zero.contains(id) {
                keep.insert(format!("catalog:{id}"));
            }
        }
        for (game, _) in &per_game {
            keep.insert(format!("game:{game}"));
        }
        // Self-update ride-along: state + `app:` card (Unknown passes the
        // card through; Updating/Updated are terminal for the process).
        if let Some(app) = report.app {
            self.apply_app_poll(app, &mut keep, cx);
        }
        self.notices.retain_attention(&keep);
        // Page badges refresh without extra GitHub hits: the poll already
        // recorded catalog statuses; cards compare locally below.
        self.catalog_updates = report.available;
        // Catalog rows are Settings-held: refresh them only when resident
        // (the tab reloads on entry otherwise).
        if !self.instances.is_empty() {
            self.instances = load_instances().into_boxed_slice();
            self.catalog_meta = super::catalog_meta::load_catalog_meta();
        }
        // The poll already knows staleness for every game — including games
        // whose condition just cleared — so drop all per-card verdicts
        // regardless of tab. Otherwise a stale cached verdict survives
        // off-tab (entry re-checks only missing keys) and masks the poll
        // badge behind it. Re-checks are silent, so nothing shifts.
        self.invalidate_all_updates();
        if let Some(id) = self.selected.clone() {
            if self.game_tab == GameTab::Mods {
                self.refresh_mods(&id, cx);
            }
        }
        cx.notify();
        again
    }

    /// E104: catalog display label for one Mod id: resident rows first,
    /// else a transient catalog read, else the id. Action paths only, never
    /// paint.
    pub(crate) fn catalog_label(&self, id: &str) -> String {
        if let Some(i) = self.instances.iter().find(|i| i.id == id) {
            return i.label.clone();
        }
        find_mod(&config_dir(), &data_dir(), id)
            .map(|m| m.label)
            .unwrap_or_else(|_| id.to_string())
    }

    /// E104: display name for one game id (falls back to the id). Empty
    /// names count as missing (`display_name` filters them), so a blank
    /// row never paints a blank Attention title.
    pub(crate) fn game_display(&self, id: &str) -> String {
        index_row(&self.index, id)
            .map(|g| g.display_name().to_string())
            .unwrap_or_else(|| id.to_string())
    }

    /// E104: focus return runs the poll when due (one Instant check).
    /// `gui.mod-config-edit`: an external Save while unfocused refills the
    /// affected preview key and re-reads stage pills when the Mods tab is
    /// showing — no stage hashing on other pages. The in-app editor closes
    /// on external open, so the level comes from `config_external_level`.
    pub(crate) fn poll_on_focus_return(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.config_external_open {
            self.config_external_open = false;
            if let Some(level) = self.config_external_level.clone() {
                // Refill, never bust: names survive a content edit (an
                // external delete still refreshes via the walk).
                self.fill_file_preview(level.mod_id());
            }
            self.config_external_level = None;
            if self.nav == Nav::Game && self.game_tab == GameTab::Mods {
                if let Some(gid) = self.selected.clone() {
                    self.refresh_mod_extra(&gid, cx, true);
                }
            }
            cx.notify();
        }
        self.maybe_poll_updates_active(active, cx);
    }

    /// E104: follow one Attention card. `catalog:<id>` → Settings Mods;
    /// `game:<id>` → that game's Mods tab; `app:<tag>` → Settings General.
    /// Closes the sidecar first so the target page paints without the panel
    /// over it.
    pub(crate) fn follow_attention(&mut self, key: &str, cx: &mut Context<Self>) {
        // Guard before any push or mutation: a blocked nav leaves history
        // untouched and the sidecar open under the discard modal.
        let target: Option<ConfigNavPending> = if let Some(game) = key.strip_prefix("game:") {
            Some(ConfigNavPending::Place(Place::Game {
                id: game.to_string(),
                tab: GameTab::Mods,
            }))
        } else if key.starts_with("catalog:") {
            Some(ConfigNavPending::Place(Place::Settings {
                tab: SettingsTab::Mods,
            }))
        } else if key.starts_with("app:") {
            Some(ConfigNavPending::Place(Place::Settings {
                tab: SettingsTab::General,
            }))
        } else {
            None
        };
        if let Some(target) = target {
            if !self.try_leave_config(target, cx) {
                return;
            }
        }
        self.sidecar_open = false;
        if let Some(game) = key.strip_prefix("game:") {
            // Push the current place first (select_game pattern) so Back
            // returns here.
            let place = Place::Game {
                id: game.to_string(),
                tab: GameTab::Mods,
            };
            let _ = self.push_if_new(place);
            self.show_game(game.to_string(), GameTab::Mods, cx);
        } else if key.starts_with("catalog:") {
            if self.nav != Nav::Settings || self.settings_tab != SettingsTab::Mods {
                self.drop_preview_disclosure();
            }
            self.settings_tab = SettingsTab::Mods;
            self.persist_settings_tab();
            if self.nav == Nav::Settings {
                self.load_settings_tab(cx);
            } else {
                self.enter_settings(cx);
            }
        } else if key.starts_with("app:") {
            if self.nav != Nav::Settings || self.settings_tab != SettingsTab::General {
                self.drop_preview_disclosure();
            }
            self.settings_tab = SettingsTab::General;
            self.persist_settings_tab();
            if self.nav == Nav::Settings {
                self.load_settings_tab(cx);
            } else {
                self.enter_settings(cx);
            }
        }
        cx.notify();
    }

    /// E101: bell toggle. Opening focuses the shell so Escape reaches the
    /// root key listener; the sidecar is not a place and never pushes.
    pub(crate) fn host_health_note(&self) -> Option<Note> {
        let warn = tuxgt_core::required_host_warning(&self.host_install)?;
        let mut args = FluentArgs::new();
        args.set("count", warn.count.to_string());
        args.set("path", warn.example.display().to_string());
        let id = if warn.missing {
            "gui-warn-host-install-missing"
        } else {
            "gui-warn-host-install-modified"
        };
        Some(Note::Warn(self.strings.get_args(id, Some(&args))))
    }

    /// E70: push the host-health warning when present, merging with an
    /// already-pending warning (Heroic restart) into one toast per op.
    pub(crate) fn note_host_health(&mut self) {
        let Some(Note::Warn(health)) = self.host_health_note() else {
            return;
        };
        match self.pending_note.take() {
            None => {
                self.pending_note = Some(Note::Warn(health));
            }
            Some(Note::Warn(prev)) => {
                self.pending_note = Some(Note::Warn(format!("{prev} · {health}")));
            }
            other => {
                // A non-warning note (e.g. an error) wins; drift persists and
                // warns again on the next op.
                self.pending_note = other;
            }
        }
    }
}
