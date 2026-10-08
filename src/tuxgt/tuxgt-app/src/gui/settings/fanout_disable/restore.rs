use std::collections::{BTreeSet, HashMap};

use gpui_kit::*;
use tuxgt_core::{
    clear_bulk_disabled, config_dir, data_dir, open_db_shared, read_bulk_disabled,
    set_enabled_in_games, unrecord_bulk_games, FluentArgs,
};

use super::super::super::{load_catalog_meta, rt_block, Shell};
use super::super::fanout_ui::{
    fanout_dialog, FanoutAction, FanoutButton, FanoutDialog, FanoutReport, FanoutStyle,
};
impl Shell {
    /// Offered-toggle on with a recorded bulk set: flip the catalog
    /// entry, then offer to re-enable exactly the recorded pairs.
    /// Fully stale records clear silently.
    pub(crate) fn confirm_restore_ui(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let recorded = match read_bulk_disabled(&config_dir()) {
            Ok(map) => map.get(&id).cloned().unwrap_or_default(),
            Err(e) => {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        };
        if recorded.is_empty() {
            self.flip_offered_ui(id, true, cx);
            return;
        }
        let scope = match self.fanout_scope_sync(&id) {
            Ok(hits) => hits,
            Err(e) => {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        };
        if !self.flip_offered_ui(id.clone(), true, cx) {
            return;
        }
        let plan = tuxgt_core::plan_bulk_restore(&data_dir(), &id, &recorded);
        if plan.restore.is_empty() {
            let _ = clear_bulk_disabled(&config_dir(), &id);
            return;
        }
        let names: HashMap<String, String> = self.fanout_games().into_iter().collect();
        let name_of = |g: &str| names.get(g).cloned().unwrap_or_else(|| g.to_string());
        let mut lines: Vec<String> = plan.restore.iter().map(|g| name_of(g)).collect();
        for (game, reason) in &plan.skipped {
            let key = match reason {
                tuxgt_core::SkipReason::Uninstalled => "gui-note-fanout-skip-uninstalled",
                tuxgt_core::SkipReason::Enabled => "gui-note-fanout-skip-enabled",
            };
            lines.push(format!("{} — {}", name_of(game), self.strings.get(key)));
        }
        let lines = self.fanout_game_lines(lines);
        let restore_set: BTreeSet<String> = plan.restore.iter().cloned().collect();
        let hits: Vec<tuxgt_core::ScopeHit> = scope
            .into_iter()
            .filter(|h| restore_set.contains(&h.game))
            .collect();
        let warns = self.fanout_cap_warns(&self.fanout_warn_lines(&hits, true));
        let title = self.fanout_message(
            "gui-note-reenable-one",
            "gui-note-reenable-many",
            self.fanout_mod_label(&id),
            plan.restore.len(),
        );
        fanout_dialog(
            cx.entity(),
            window,
            cx,
            FanoutDialog {
                title,
                lines,
                warns,
                control: None,
                buttons: vec![
                    FanoutButton {
                        id: "fanout-restore-apply".to_string(),
                        label: self.strings.get("gui-action-reenable-in-games"),
                        style: FanoutStyle::Primary,
                        action: FanoutAction::RestoreInGames {
                            id: id.clone(),
                            games: plan.restore.clone(),
                        },
                    },
                    FanoutButton {
                        id: "fanout-restore-keep".to_string(),
                        label: self.strings.get("gui-action-keep-disabled"),
                        style: FanoutStyle::Ghost,
                        action: FanoutAction::KeepDisabled { id },
                    },
                ],
            },
        );
    }

    /// Re-enable the recorded bulk games (the dialog consent is the
    /// foreign-overwrite `yes`); unrecord what lands.
    pub(crate) fn fanout_restore_ui(
        &mut self,
        id: String,
        games: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            action = "fanout-restore",
            source = id.as_str(),
            count = games.len()
        );
        let transfer = self.hide_state.installing();
        let id_bg = id.clone();
        let names: HashMap<String, String> = self.fanout_games().into_iter().collect();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let cfg = config_dir();
                        let pool = open_db_shared(&data).await?;
                        let pairs: Vec<(String, String)> = games
                            .iter()
                            .map(|g| {
                                (
                                    g.clone(),
                                    names.get(g).cloned().unwrap_or_else(|| g.clone()),
                                )
                            })
                            .collect();
                        let mut report = FanoutReport::default();
                        for r in
                            set_enabled_in_games(&pool, &data, &pairs, &id_bg, true, true).await
                        {
                            report.attempted.push(r.game.clone());
                            match r.result {
                                Ok(()) => {
                                    report.succeeded.push((r.game, id_bg.clone()));
                                }
                                Err(e) => {
                                    report.failures.push(format!("{} — {e}", r.name));
                                }
                            }
                        }
                        let mut done: BTreeSet<String> =
                            report.succeeded.iter().map(|(g, _)| g.clone()).collect();
                        // Prune pairs that went stale since the dialog (or
                        // landed above, now enabled) so the record never
                        // rots; still-disabled failures stay recorded.
                        match read_bulk_disabled(&cfg) {
                            Ok(map) => {
                                let recorded = map.get(&id_bg).cloned().unwrap_or_default();
                                let plan = tuxgt_core::plan_bulk_restore(&data, &id_bg, &recorded);
                                done.extend(plan.skipped.iter().map(|(g, _)| g.clone()));
                            }
                            Err(e) => {
                                report.failures.push(format!("bulk memory — {e}"));
                            }
                        }
                        if let Err(e) = unrecord_bulk_games(&cfg, &id_bg, &done) {
                            report.failures.push(format!("bulk memory — {e}"));
                        }
                        Ok::<_, tuxgt_core::Error>((super::super::load_instances(), report))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((rows, report)) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        this.catalog_meta = load_catalog_meta();
                        let mut games = report.attempted.clone();
                        games.sort();
                        games.dedup();
                        for game in &games {
                            this.refresh_mods(game, cx);
                        }
                        this.status = this.fanout_restore_status(&id, &report);
                    }
                    Err(e) => {
                        this.status = format!("{e}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Status for a finished restore run.
    fn fanout_restore_status(&self, id: &str, report: &FanoutReport) -> String {
        if report.failures.is_empty() {
            let mut args = FluentArgs::new();
            args.set("id", id.to_string());
            args.set("n", report.succeeded.len().to_string());
            return self
                .strings
                .get_args("gui-status-reenabled-in-games", Some(&args));
        }
        let mut args = FluentArgs::new();
        args.set("done", report.succeeded.len().to_string());
        args.set("n", report.attempted.len().to_string());
        args.set("err", report.failures.first().cloned().unwrap_or_default());
        self.strings
            .get_args("gui-status-fanout-partial", Some(&args))
    }

    /// Keep-disabled choice: drop the bulk record; games stay as they are.
    pub(crate) fn fanout_keep_disabled_ui(&mut self, id: String, cx: &mut Context<Self>) {
        if let Err(e) = clear_bulk_disabled(&config_dir(), &id) {
            self.status = format!("{e}");
        }
        cx.notify();
    }
}
