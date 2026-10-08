use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, WindowExt as _};
use gpui_kit::*;

use super::super::theme::{types, TypeStyled as _};
use super::super::{rt_block, widgets, Shell};

/// Cap for game/warning rows in a fan-out dialog; the rest fold into
/// one `+ N more` row (same 10-cap as the Details disclosure).
pub(crate) const FANOUT_LIST_CAP: usize = 10;

/// Shown rows plus the hidden count for the `+ N more` row.
pub(crate) fn cap_lines(lines: &[String], cap: usize) -> (Vec<String>, usize) {
    if lines.len() <= cap {
        return (lines.to_vec(), 0);
    }
    (lines[..cap].to_vec(), lines.len() - cap)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FanoutStyle {
    Primary,
    Ghost,
}

/// The disable dialog's scope radio. Opening the dialog selects no game
/// changes; Yes reads the live choice.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FanoutDisableChoice {
    #[default]
    CatalogOnly,
    Disable,
    Delete,
}

/// A dialog control (checkbox, radios) built per render: Root stores the
/// builder closure and re-runs it on notify, so the control reads live
/// Shell state through the view.
pub(crate) type FanoutControl = Box<dyn Fn(Entity<Shell>, &mut App) -> AnyElement + 'static>;

/// One fan-out dialog button: the stashed action runs when clicked. Yes
/// actions read the dialog's control state live; No only closes.
#[derive(Clone)]
pub(crate) enum FanoutAction {
    /// Remove Yes: the checkbox picks everywhere vs catalog-only.
    RemoveYes {
        uninstall: Vec<(String, Vec<String>)>,
        remove: Vec<String>,
    },
    /// Disable Yes: the radio picks the scope.
    DisableYes {
        id: String,
        games: Vec<String>,
    },
    /// Re-enable the recorded bulk games.
    RestoreInGames {
        id: String,
        games: Vec<String>,
    },
    /// Drop the bulk record; games stay as they are.
    KeepDisabled {
        id: String,
    },
    No,
}

#[derive(Clone)]
pub(crate) struct FanoutButton {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) style: FanoutStyle,
    pub(crate) action: FanoutAction,
}

/// Content for one Settings fan-out dialog: the question, game rows,
/// warning rows, an optional control (checkbox, radios), buttons.
/// Strings are copied before `open_dialog`: its builder runs later.
pub(crate) struct FanoutDialog {
    pub title: String,
    pub lines: Vec<String>,
    pub warns: Vec<String>,
    pub control: Option<FanoutControl>,
    pub buttons: Vec<FanoutButton>,
}

/// Open one fan-out dialog. The builder re-runs on notify, so the
/// control reads live Shell state through the view.
pub(crate) fn fanout_dialog(
    view: Entity<Shell>,
    window: &mut Window,
    cx: &mut App,
    content: FanoutDialog,
) {
    window.open_dialog(cx, move |dialog, _, cx| {
        let warn_color = cx.theme().warning;
        dialog.title(content.title.clone()).child(
            v_flex()
                .gap_2()
                .children(content.lines.iter().map(|l| widgets::muted(l.clone(), cx)))
                .children(content.warns.iter().map(|w| {
                    div()
                        .tx(types(cx).body_md)
                        .text_color(warn_color)
                        .child(w.clone())
                }))
                .children(content.control.as_ref().map(|c| c(view.clone(), cx)))
                .child(
                    h_flex().gap_2().children(
                        content
                            .buttons
                            .iter()
                            .cloned()
                            .map(|b| fanout_button(view.clone(), b, cx)),
                    ),
                ),
        )
    })
}

fn fanout_button(view: Entity<Shell>, b: FanoutButton, cx: &mut App) -> impl IntoElement {
    let btn = widgets::btn(b.id.clone(), cx).child(widgets::blabel(b.label.clone(), cx));
    let btn = match b.style {
        FanoutStyle::Primary => btn.primary(),
        FanoutStyle::Ghost => btn.ghost(),
    };
    btn.on_click(move |_, window, cx| {
        window.close_dialog(cx);
        if matches!(b.action, FanoutAction::No) {
            return;
        }
        let action = b.action.clone();
        view.update(cx, |this, cx| {
            this.fanout_dispatch(action, cx);
        });
    })
}

impl Shell {
    /// The library index as (game id, display name) pairs for scope scans.
    pub(crate) fn fanout_games(&self) -> Vec<(String, String)> {
        self.games
            .iter()
            .map(|g| (g.id.clone(), g.display_name().to_string()))
            .collect()
    }

    /// Synchronous scope scan for one mod (UI thread, like the catalog
    /// meta load): manifests plus required-by plus foreign checks.
    pub(crate) fn fanout_scope_sync(
        &self,
        mod_id: &str,
    ) -> tuxgt_core::Result<Vec<tuxgt_core::ScopeHit>> {
        let games = self.fanout_games();
        let id = mod_id.to_string();
        rt_block(async move {
            let data = tuxgt_core::data_dir();
            let pool = tuxgt_core::open_db_shared(&data).await?;
            tuxgt_core::install_scope(&pool, &tuxgt_core::config_dir(), &data, &games, &id).await
        })
    }

    /// Run the stashed fan-out action from a dialog Yes button. The
    /// dialog's control state (checkbox, radio) is read live.
    pub(crate) fn fanout_dispatch(&mut self, action: FanoutAction, cx: &mut Context<Self>) {
        match action {
            FanoutAction::RemoveYes { uninstall, remove } => {
                if self.fanout_remove_games {
                    self.fanout_remove_everywhere_ui(uninstall, remove, cx);
                } else {
                    self.fanout_catalog_only_remove(remove, cx);
                }
            }
            FanoutAction::DisableYes { id, games } => match self.fanout_disable_choice {
                FanoutDisableChoice::CatalogOnly => {
                    self.flip_offered_ui(id, false, cx);
                }
                FanoutDisableChoice::Disable => self.fanout_disable_ui(id, games, false, cx),
                FanoutDisableChoice::Delete => self.fanout_disable_ui(id, games, true, cx),
            },
            FanoutAction::RestoreInGames { id, games } => self.fanout_restore_ui(id, games, cx),
            FanoutAction::KeepDisabled { id } => self.fanout_keep_disabled_ui(id, cx),
            FanoutAction::No => {}
        }
    }

    /// Catalog-only removal for a stashed id list: one background task.
    /// Concurrent single removes race on `instances` (last-writer-wins
    /// can resurrect a removed row) and clobber status.
    pub(crate) fn fanout_catalog_only_remove(&mut self, mods: Vec<String>, cx: &mut Context<Self>) {
        match mods.as_slice() {
            [id] => self.remove_instance_ui(id.clone(), cx),
            _ => self.remove_visible_mods_ui(mods, cx),
        }
    }
}

/// One fan-out run's outcome for refresh + status. Every attempted
/// game reports; failures never silently skip.
#[derive(Default)]
pub(crate) struct FanoutReport {
    pub attempted: Vec<String>,
    pub succeeded: Vec<(String, String)>,
    pub failures: Vec<String>,
    pub removed: usize,
    pub remove_failed: bool,
}

impl Shell {
    /// Warning rows capped like game rows, with one `+ N more` row.
    pub(crate) fn fanout_cap_warns(&self, warns: &[String]) -> Vec<String> {
        let (mut capped, hidden) = cap_lines(warns, FANOUT_LIST_CAP);
        if hidden > 0 {
            let mut more = tuxgt_core::FluentArgs::new();
            more.set("n", hidden.to_string());
            capped.push(self.strings.get_args("gui-note-fanout-more", Some(&more)));
        }
        capped
    }

    /// The shared Yes / No cluster. Yes carries the stashed scope and
    /// reads the dialog's control state live; No only closes.
    pub(crate) fn fanout_yes_no_buttons(
        &self,
        yes_id: &str,
        no_id: &str,
        yes: FanoutAction,
    ) -> Vec<FanoutButton> {
        vec![
            FanoutButton {
                id: yes_id.to_string(),
                label: self.strings.get("gui-action-yes"),
                style: FanoutStyle::Primary,
                action: yes,
            },
            FanoutButton {
                id: no_id.to_string(),
                label: self.strings.get("gui-action-no"),
                style: FanoutStyle::Ghost,
                action: FanoutAction::No,
            },
        ]
    }

    /// Catalog display label for a mod id, falling back to the id.
    pub(crate) fn fanout_mod_label(&self, id: &str) -> String {
        self.instances
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.label.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// One/many dialog message for a mod, by install count.
    pub(crate) fn fanout_message(&self, one: &str, many: &str, name: String, n: usize) -> String {
        let mut args = tuxgt_core::FluentArgs::new();
        args.set("name", name);
        if n <= 1 {
            self.strings.get_args(one, Some(&args))
        } else {
            args.set("n", n.to_string());
            self.strings.get_args(many, Some(&args))
        }
    }

    /// Game rows capped with one `+ N more` row.
    pub(crate) fn fanout_game_lines(&self, names: Vec<String>) -> Vec<String> {
        let (mut lines, hidden) = cap_lines(&names, FANOUT_LIST_CAP);
        if hidden > 0 {
            let mut more = tuxgt_core::FluentArgs::new();
            more.set("n", hidden.to_string());
            lines.push(self.strings.get_args("gui-note-fanout-more", Some(&more)));
        }
        lines
    }

    /// Warning rows for scope hits: foreign dests, required-by, missing
    /// game roots. Game order follows the scan (sorted by name).
    /// `for_enable` words the foreign list as replacements (re-enable
    /// overwrites with backups kept) instead of left-in-place.
    pub(crate) fn fanout_warn_lines(
        &self,
        hits: &[tuxgt_core::ScopeHit],
        for_enable: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for h in hits {
            if !h.foreign.is_empty() {
                let mut args = tuxgt_core::FluentArgs::new();
                args.set("game", h.name.clone());
                args.set("n", h.foreign.len().to_string());
                let key = if for_enable {
                    "gui-note-fanout-foreign-overwrite"
                } else {
                    "gui-note-fanout-foreign"
                };
                out.push(self.strings.get_args(key, Some(&args)));
            }
            if !h.required_by.is_empty() {
                let mut args = tuxgt_core::FluentArgs::new();
                args.set("game", h.name.clone());
                args.set("mods", h.required_by.join(", "));
                out.push(
                    self.strings
                        .get_args("gui-note-fanout-required", Some(&args)),
                );
            }
            if h.root_error.is_some() {
                let mut args = tuxgt_core::FluentArgs::new();
                args.set("game", h.name.clone());
                out.push(self.strings.get_args("gui-note-fanout-noroot", Some(&args)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::cap_lines;

    #[test]
    fn cap_lines_keeps_short_lists_whole() {
        let lines = vec!["a".to_string(), "b".to_string()];
        assert_eq!(cap_lines(&lines, 10), (lines, 0));
    }

    #[test]
    fn cap_lines_reports_hidden_count() {
        let lines: Vec<String> = (0..12).map(|i| format!("g{i}")).collect();
        let (shown, hidden) = cap_lines(&lines, 10);
        assert_eq!(shown.len(), 10);
        assert_eq!(shown[9], "g9");
        assert_eq!(hidden, 2);
    }
}
