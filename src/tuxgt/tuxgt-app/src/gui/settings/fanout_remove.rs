use std::collections::HashMap;

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{h_flex, WindowExt as _};
use gpui_kit::*;
use tuxgt_core::{
    config_dir, data_dir, open_db_shared, remove_mod, uninstall_in_games, FluentArgs,
};

use super::super::{load_catalog_meta, rt_block, widgets, Shell};
use super::fanout_ui::{fanout_dialog, FanoutAction, FanoutControl, FanoutDialog, FanoutReport};

/// The also-remove checkbox: controlled by `Shell::fanout_remove_games`,
/// rebuilt live on every dialog render.
fn remove_games_control(label: String) -> FanoutControl {
    Box::new(move |view: Entity<Shell>, cx: &mut App| {
        let checked = view.read(cx).fanout_remove_games;
        h_flex()
            .gap_2()
            .child(
                Checkbox::new("fanout-remove-games-check")
                    .checked(checked)
                    .accessibility_label(label.clone())
                    .on_change(move |val, _, cx| {
                        let on = *val;
                        view.update(cx, |this, cx| {
                            this.fanout_remove_games = on;
                            cx.notify();
                        });
                    }),
            )
            .child(widgets::blabel(label.clone(), cx))
            .into_any_element()
    })
}

impl Shell {
    /// Trash on one user-Mod card: silent when the mod is installed
    /// nowhere, else the remove question with an also-remove checkbox
    /// (checked: the settled everywhere default) and Yes/No.
    pub(crate) fn confirm_remove_instance_ui(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = match self.fanout_scope_sync(&id) {
            Ok(hits) => hits,
            Err(e) => {
                let mut args = FluentArgs::new();
                args.set("error", e.to_string());
                self.status = self.strings.get_args("gui-status-err-remove", Some(&args));
                cx.notify();
                return;
            }
        };
        if scope.is_empty() {
            self.remove_instance_ui(id, cx);
            return;
        }
        self.fanout_remove_games = true;
        let name = self.fanout_mod_label(&id);
        let title = self.fanout_message(
            "gui-note-fanout-remove-one",
            "gui-note-fanout-remove-many",
            name,
            scope.len(),
        );
        let names: Vec<String> = scope.iter().map(|h| h.name.clone()).collect();
        let lines = self.fanout_game_lines(names);
        let warns = self.fanout_cap_warns(&self.fanout_warn_lines(&scope, false));
        let check = self.fanout_message(
            "gui-note-fanout-check-one",
            "gui-note-fanout-check-many",
            String::new(),
            scope.len(),
        );
        let games: Vec<String> = scope.iter().map(|h| h.game.clone()).collect();
        let buttons = self.fanout_yes_no_buttons(
            "fanout-remove-yes",
            "fanout-remove-no",
            FanoutAction::RemoveYes {
                uninstall: vec![(id.clone(), games)],
                remove: vec![id],
            },
        );
        fanout_dialog(
            cx.entity(),
            window,
            cx,
            FanoutDialog {
                title,
                lines,
                warns,
                control: Some(remove_games_control(check)),
                buttons,
            },
        );
    }

    /// Remove-visible confirm with the aggregated install scope: one
    /// dialog for every listed mod, or the plain confirm when no listed
    /// mod is installed anywhere.
    pub(crate) fn confirm_remove_visible_scope(
        &mut self,
        ids: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut scoped: Vec<(String, Vec<tuxgt_core::ScopeHit>)> = Vec::new();
        for id in &ids {
            match self.fanout_scope_sync(id) {
                Ok(hits) if !hits.is_empty() => scoped.push((id.clone(), hits)),
                Ok(_) => {}
                Err(e) => {
                    let mut args = FluentArgs::new();
                    args.set("error", e.to_string());
                    self.status = self.strings.get_args("gui-status-err-remove", Some(&args));
                    cx.notify();
                    return;
                }
            }
        }
        if scoped.is_empty() {
            self.confirm_remove_visible_plain(ids, window, cx);
            return;
        }
        self.fanout_remove_games = true;
        let title = self.strings.get("gui-note-fanout-visible");
        let mut lines = Vec::new();
        let mut warns = Vec::new();
        let mut uninstall = Vec::new();
        for (id, hits) in &scoped {
            let mut args = FluentArgs::new();
            args.set("name", self.fanout_mod_label(id));
            args.set("n", hits.len().to_string());
            lines.push(
                self.strings
                    .get_args("gui-note-fanout-mod-games", Some(&args)),
            );
            warns.extend(self.fanout_warn_lines(hits, false));
            uninstall.push((id.clone(), hits.iter().map(|h| h.game.clone()).collect()));
        }
        let capped = self.fanout_game_lines(lines);
        let warns = self.fanout_cap_warns(&warns);
        let check = self.strings.get("gui-note-fanout-check-visible");
        let buttons = self.fanout_yes_no_buttons(
            "fanout-remove-visible-yes",
            "fanout-remove-visible-no",
            FanoutAction::RemoveYes {
                uninstall,
                remove: ids,
            },
        );
        fanout_dialog(
            cx.entity(),
            window,
            cx,
            FanoutDialog {
                title,
                lines: capped,
                warns,
                control: Some(remove_games_control(check)),
                buttons,
            },
        );
    }

    /// The plain remove-visible confirm: no listed mod is installed
    /// anywhere, so there is no scope to choose. Yes/No wording matches
    /// the fan-out dialogs.
    pub(crate) fn confirm_remove_visible_plain(
        &self,
        ids: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut args = FluentArgs::new();
        args.set("n", ids.len().to_string());
        let title = self
            .strings
            .get_args("gui-note-remove-visible", Some(&args));
        let confirm = self.strings.get("gui-action-yes");
        let cancel = self.strings.get("gui-action-no");
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            dialog.title(title.clone()).child(
                h_flex()
                    .gap_2()
                    .child(
                        widgets::btn("remove-visible-yes", cx)
                            .primary()
                            .child(widgets::blabel(confirm.clone(), cx))
                            .on_click({
                                let view = view.clone();
                                let ids = ids.clone();
                                move |_, window, cx| {
                                    window.close_dialog(cx);
                                    view.update(cx, |this, cx| {
                                        this.remove_visible_mods_ui(ids.clone(), cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        widgets::btn("remove-visible-no", cx)
                            .ghost()
                            .child(widgets::blabel(cancel.clone(), cx))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
        });
    }

    /// Uninstall each mod in its scoped games, then remove the recipes.
    /// Every game reports; failures never silently skip.
    pub(crate) fn fanout_remove_everywhere_ui(
        &mut self,
        uninstall: Vec<(String, Vec<String>)>,
        remove: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            action = "fanout-remove-everywhere",
            mods = remove.len(),
            games = uninstall.iter().map(|(_, g)| g.len()).sum::<usize>()
        );
        let transfer = self.hide_state.installing();
        let mods_count = remove.len();
        let names: HashMap<String, String> = self.fanout_games().into_iter().collect();
        cx.spawn(async move |this, cx| {
            let _transfer = transfer;
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let cfg = config_dir();
                        let pool = open_db_shared(&data).await?;
                        let mut report = FanoutReport::default();
                        for (mod_id, games) in &uninstall {
                            let pairs: Vec<(String, String)> = games
                                .iter()
                                .map(|g| {
                                    (
                                        g.clone(),
                                        names.get(g).cloned().unwrap_or_else(|| g.clone()),
                                    )
                                })
                                .collect();
                            for r in uninstall_in_games(&pool, &data, &pairs, mod_id).await {
                                report.attempted.push(r.game.clone());
                                match r.result {
                                    Ok(()) => {
                                        report.succeeded.push((r.game, mod_id.clone()));
                                    }
                                    Err(e) => {
                                        report.failures.push(format!("{} — {e}", r.name));
                                    }
                                }
                            }
                        }
                        for mod_id in &remove {
                            match remove_mod(&cfg, &data, mod_id) {
                                Ok(()) => report.removed += 1,
                                Err(e) => {
                                    report.remove_failed = true;
                                    report.failures.push(format!("{mod_id} — {e}"));
                                }
                            }
                        }
                        Ok::<_, tuxgt_core::Error>((super::load_instances(), report))
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
                        for (game, inst) in &report.succeeded {
                            this.invalidate_update(game, inst);
                        }
                        let mut games = report.attempted.clone();
                        games.sort();
                        games.dedup();
                        for game in &games {
                            this.refresh_mods(game, cx);
                        }
                        this.status = this.fanout_remove_status(&report, mods_count);
                    }
                    Err(e) => {
                        let mut args = FluentArgs::new();
                        args.set("error", e.to_string());
                        this.status = this.strings.get_args("gui-status-err-remove", Some(&args));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Status for a finished remove-everywhere run: full-ok names the
    /// scope, anything else names the first failure.
    fn fanout_remove_status(&self, report: &FanoutReport, mods: usize) -> String {
        if report.failures.is_empty() {
            if mods <= 1 {
                let mut args = FluentArgs::new();
                args.set(
                    "id",
                    report
                        .succeeded
                        .first()
                        .map(|(_, m)| m.clone())
                        .unwrap_or_default(),
                );
                args.set("n", report.succeeded.len().to_string());
                return self
                    .strings
                    .get_args("gui-status-removed-everywhere", Some(&args));
            }
            let mut args = FluentArgs::new();
            args.set("m", report.removed.to_string());
            args.set("g", report.succeeded.len().to_string());
            return self
                .strings
                .get_args("gui-status-removed-visible-everywhere", Some(&args));
        }
        if report.remove_failed && report.succeeded.len() == report.attempted.len() {
            let mut args = FluentArgs::new();
            args.set("error", report.failures.join("; "));
            return self.strings.get_args("gui-status-err-remove", Some(&args));
        }
        let mut args = FluentArgs::new();
        args.set("done", report.succeeded.len().to_string());
        args.set("n", report.attempted.len().to_string());
        args.set("err", report.failures.first().cloned().unwrap_or_default());
        self.strings
            .get_args("gui-status-fanout-partial", Some(&args))
    }
}
