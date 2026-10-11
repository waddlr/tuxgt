mod restore;

use std::collections::{BTreeSet, HashMap};

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use tuxgt_core::{
    config_dir, data_dir, open_db_shared, record_bulk_disabled, set_enabled_in_games,
    set_mod_offered, uninstall_in_games, FluentArgs,
};

use super::super::{load_catalog_meta, rt_block, widgets, Shell};
use super::fanout_ui::{
    fanout_dialog, FanoutAction, FanoutControl, FanoutControlState, FanoutDialog,
    FanoutDisableChoice, FanoutReport,
};

/// The scope radios: glyph + label rows, click to pick (the kit has no
/// Radio component). Controlled by its own entity (reading Shell here
/// would panic inside `Shell::render`), rebuilt live on every dialog
/// render; the Shell view only repaints on change.
fn disable_choice_control(
    labels: [String; 3],
    control: Entity<FanoutControlState>,
) -> FanoutControl {
    Box::new(move |view: Entity<Shell>, cx: &mut App| {
        let current = control
            .read(cx)
            .disable_or(FanoutDisableChoice::CatalogOnly);
        let rows = [
            (
                FanoutDisableChoice::CatalogOnly,
                "fanout-disable-radio-keep",
                labels[0].clone(),
            ),
            (
                FanoutDisableChoice::Disable,
                "fanout-disable-radio-disable",
                labels[1].clone(),
            ),
            (
                FanoutDisableChoice::Delete,
                "fanout-disable-radio-delete",
                labels[2].clone(),
            ),
        ];
        v_flex()
            .gap_1()
            .children(rows.into_iter().map(|(choice, id, label)| {
                let glyph = if current == choice { "●" } else { "○" };
                let row_view = view.clone();
                let writer = control.clone();
                h_flex()
                    .id(SharedString::from(id))
                    .gap_2()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        writer.update(cx, |state, _| {
                            *state = FanoutControlState::Disable(choice);
                        });
                        row_view.update(cx, |_, cx| cx.notify());
                    })
                    .child(widgets::mono(glyph, cx))
                    .child(widgets::blabel(label, cx))
            }))
            .into_any_element()
    })
}

impl Shell {
    /// The catalog offered-flag flip, no game fan-out. Silent paths and
    /// the Catalog-only choices land here. True when the flip landed.
    pub(crate) fn flip_offered_ui(&mut self, id: String, on: bool, cx: &mut Context<Self>) -> bool {
        let ok = match set_mod_offered(&config_dir(), &id, on, &data_dir()) {
            Ok(()) => {
                if let Some(row) = self.instances.iter_mut().find(|r| r.id == id) {
                    row.enabled = on;
                }
                self.refresh_selected_mods();
                let mut args = FluentArgs::new();
                args.set("instance", id);
                let key = if on {
                    "gui-status-instance-enabled"
                } else {
                    "gui-status-instance-disabled"
                };
                self.status = self.strings.get_args(key, Some(&args));
                true
            }
            Err(e) => {
                self.status = format!("{e}");
                false
            }
        };
        cx.notify();
        ok
    }

    /// Offered-toggle off: silent when the mod is installed nowhere,
    /// else the disable question with scope radios (no game changes
    /// selected) and Yes/No.
    pub(crate) fn confirm_disable_ui(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = match self.fanout_scope_sync(&id) {
            Ok(hits) => hits,
            Err(e) => {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        };
        if scope.is_empty() {
            self.flip_offered_ui(id, false, cx);
            return;
        }
        let control = cx.new(|_| FanoutControlState::Disable(FanoutDisableChoice::CatalogOnly));
        let name = self.fanout_mod_label(&id);
        let title = self.fanout_message(
            "gui-note-fanout-disable-one",
            "gui-note-fanout-disable-many",
            name,
            scope.len(),
        );
        let off = self.strings.get("gui-note-fanout-off");
        let names: Vec<String> = scope
            .iter()
            .map(|h| {
                if h.enabled {
                    h.name.clone()
                } else {
                    format!("{} — {off}", h.name)
                }
            })
            .collect();
        let lines = self.fanout_game_lines(names);
        let warns = self.fanout_cap_warns(&self.fanout_warn_lines(&scope, false));
        let labels = [
            self.strings.get("gui-note-fanout-radio-keep"),
            self.strings.get("gui-note-fanout-radio-disable"),
            self.strings.get("gui-note-fanout-radio-delete"),
        ];
        let games: Vec<String> = scope.iter().map(|h| h.game.clone()).collect();
        let buttons = self.fanout_yes_no_buttons(
            "fanout-disable-yes",
            "fanout-disable-no",
            FanoutAction::DisableYes { id, games },
            control.clone(),
        );
        fanout_dialog(
            cx.entity(),
            window,
            cx,
            FanoutDialog {
                title,
                lines,
                warns,
                control: Some(disable_choice_control(labels, control)),
                buttons,
            },
        );
    }

    /// Per-game disable (`delete=false`, reversible) or uninstall
    /// (`delete=true`), then the catalog flip. Games first so a failure
    /// leaves the toggle in its pre-state.
    pub(crate) fn fanout_disable_ui(
        &mut self,
        id: String,
        games: Vec<String>,
        delete: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "fanout-disable", source = id.as_str(), delete);
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
                        if delete {
                            for r in uninstall_in_games(&pool, &data, &pairs, &id_bg).await {
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
                        } else {
                            // Already-off games are skipped outright (the
                            // dialog marks them): never run, never
                            // recorded, so a later restore cannot flip
                            // games the user disabled by hand.
                            let mut run = Vec::new();
                            for (game, name) in &pairs {
                                let off = tuxgt_core::game_manifests(&data, game)
                                    .ok()
                                    .and_then(|ms| ms.into_iter().find(|m| m.instance == id_bg))
                                    .is_some_and(|m| !m.enabled);
                                if !off {
                                    run.push((game.clone(), name.clone()));
                                }
                            }
                            let mut transitioned = BTreeSet::new();
                            for r in
                                set_enabled_in_games(&pool, &data, &run, &id_bg, false, false).await
                            {
                                report.attempted.push(r.game.clone());
                                match r.result {
                                    Ok(()) => {
                                        transitioned.insert(r.game.clone());
                                        report.succeeded.push((r.game, id_bg.clone()));
                                    }
                                    Err(e) => {
                                        report.failures.push(format!("{} — {e}", r.name));
                                    }
                                }
                            }
                            if let Err(e) = record_bulk_disabled(&cfg, &id_bg, &transitioned) {
                                report.failures.push(format!("bulk memory — {e}"));
                            }
                        }
                        // The toggle flips only when the fan-out fully
                        // landed: a failed game still runs the mod, so
                        // flipping would paint the exact surprise this
                        // dialog exists to prevent.
                        if report.failures.is_empty() {
                            if let Err(e) = set_mod_offered(&cfg, &id_bg, false, &data) {
                                report.failures.push(format!("catalog — {e}"));
                            }
                        }
                        Ok::<_, tuxgt_core::Error>((super::load_instances(), report, delete))
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok((rows, report, delete)) => {
                        if this.instances_showing() {
                            this.instances = rows.into_boxed_slice();
                        }
                        this.catalog_meta = load_catalog_meta();
                        if delete {
                            for (game, inst) in &report.succeeded {
                                this.invalidate_update(game, inst);
                            }
                        }
                        let mut games = report.attempted.clone();
                        games.sort();
                        games.dedup();
                        for game in &games {
                            this.refresh_mods(game, cx);
                        }
                        this.status = this.fanout_disable_status(&id, &report, delete);
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

    /// Status for a finished disable/delete run.
    fn fanout_disable_status(&self, id: &str, report: &FanoutReport, delete: bool) -> String {
        if report.failures.is_empty() && report.attempted.is_empty() {
            let mut args = FluentArgs::new();
            args.set("instance", id.to_string());
            return self
                .strings
                .get_args("gui-status-instance-disabled", Some(&args));
        }
        if report.failures.is_empty() {
            let mut args = FluentArgs::new();
            args.set("id", id.to_string());
            args.set("n", report.succeeded.len().to_string());
            let key = if delete {
                "gui-status-deleted-from-games"
            } else {
                "gui-status-disabled-in-games"
            };
            return self.strings.get_args(key, Some(&args));
        }
        let mut args = FluentArgs::new();
        args.set("done", report.succeeded.len().to_string());
        args.set("n", report.attempted.len().to_string());
        args.set("err", report.failures.first().cloned().unwrap_or_default());
        self.strings
            .get_args("gui-status-fanout-partial", Some(&args))
    }
}
