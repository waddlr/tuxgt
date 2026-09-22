use std::collections::{HashMap, HashSet};

use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{Disableable as _, IconName};
use gpui_kit::*;
use tuxgt_core::{
    data_dir, open_db_shared, set_load_order, set_mod_env_enabled, FluentArgs, LoadConflict,
};

use super::super::widgets;
use super::super::{rt_block, ModRow, SettingsModsTab, Shell};

impl Shell {
    pub(crate) fn move_button(
        id: SharedString,
        icon: IconName,
        tip: String,
        disabled: bool,
        view: &Entity<Self>,
        game_id: &str,
        inst: &str,
        delta: i32,
        cx: &App,
    ) -> AnyElement {
        let view = view.clone();
        let game_id = game_id.to_string();
        let inst = inst.to_string();
        widgets::btn(id, cx)
            .secondary()
            .child(widgets::bicon(icon))
            .tooltip(tip)
            .disabled(disabled)
            .on_click(move |_, _, cx| {
                let game_id = game_id.clone();
                let inst = inst.clone();
                view.update(cx, |this, cx| {
                    this.move_mod_ui(&game_id, &inst, delta, cx);
                });
            })
            .into_any_element()
    }

    /// Rewrite the per-game installed-mod order (first loses, last wins).
    /// Reorder never raises `NeedConfirm`: core re-applies install-adapter
    /// copies as `--yes` with backups, then prewires. `refresh_mods` only —
    /// staging bytes are untouched, so no resync.
    pub(crate) fn reorder_ui(&mut self, game: &str, order: Vec<String>, cx: &mut Context<Self>) {
        tracing::debug!(action = "reorder", game, instances = order.len());
        let game = game.to_string();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        set_load_order(&pool, &data, &game, &order).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = if result.is_ok() { "reordered" } else { "error" };
                tracing::debug!(action = "reorder", game = done.as_str(), outcome);
                match result {
                    Ok(_) => {
                        this.refresh_mods(&done, cx);
                        this.status = this.strings.get("gui-status-reordered");
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Swap one installed card with its section neighbor (`delta` −1 up, +1
    /// down). No section neighbor to swap with (edge, or the official pin
    /// boundary) is a no-op.
    pub(crate) fn move_mod_ui(
        &mut self,
        game: &str,
        instance: &str,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        let rows = self.mods.get(game).cloned().unwrap_or_default();
        tracing::debug!(action = "move-mod", game, instance, delta);
        let Some(order) = move_in_order(&rows, instance, delta) else {
            return;
        };
        self.reorder_ui(game, order, cx);
    }

    /// Move one rival to win a section-local conflict group: reinsert it
    /// directly after the last other member of the group inside its section.
    /// Only relative order inside the group decides the winner.
    pub(crate) fn make_win_ui(
        &mut self,
        game: &str,
        instance: &str,
        group: &[String],
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "make-win", game, instance);
        let rows = self.mods.get(game).cloned().unwrap_or_default();
        let Some(order) = make_win_order(&rows, instance, group) else {
            return;
        };
        self.reorder_ui(game, order, cx);
    }

    /// Toggle one manifest env row on an installed card (E74). Core rewrites
    /// the FileManifest only (no staging); the session syncs like knob/custom.
    pub(crate) fn toggle_env(
        &mut self,
        game: &str,
        instance: &str,
        key: &str,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "toggle-env", game, instance, key, on);
        let game = game.to_string();
        let instance = instance.to_string();
        let key = key.to_string();
        let inst_cb = instance.clone();
        let key_cb = key.clone();
        let done = game.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    rt_block(async {
                        let data = data_dir();
                        let pool = open_db_shared(&data).await?;
                        let host = tuxgt_core::PluginHost::load()?;
                        tuxgt_core::mutate_game(&pool, &data, &host, &game, async {
                            set_mod_env_enabled(&data, &game, &instance, &key, on)?;
                            Ok(())
                        })
                        .await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.selected.as_deref() != Some(done.as_str()) {
                    return;
                }
                let outcome = if result.is_ok() { "env-toggled" } else { "error" };
                tracing::debug!(action = "toggle-env", game = done.as_str(), instance = inst_cb.as_str(), key = key_cb.as_str(), outcome);
                match result {
                    Ok(()) => {
                        this.refresh_mods(&done, cx);
                        let mut args = FluentArgs::new();
                        args.set("instance", inst_cb.clone());
                        args.set("dest", key_cb.clone());
                        let key = if on {
                            "gui-status-file-kept"
                        } else {
                            "gui-status-file-omitted"
                        };
                        this.status = this.strings.get_args(key, Some(&args));
                    }
                    Err(e) => this.status = format!("{e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(crate) fn section_rows<'a>(rows: &'a [ModRow], tab: SettingsModsTab) -> Vec<&'a ModRow> {
    let mut out: Vec<&ModRow> = rows
        .iter()
        .filter(|r| r.installed && SettingsModsTab::for_type(&r.mod_type) == tab)
        .collect();
    out.sort_by(|a, b| {
        (!a.official, a.load_order, &a.instance).cmp(&(!b.official, b.load_order, &b.instance))
    });
    out
}
/// Installed Uninstall-visible scope: rows of the expanded sections, in paint
/// (section-major) order. A collapsed section paints its header only, so its
/// rows are out of scope (picker Select Visible parity).
pub(crate) fn installed_targets(
    sections: &[(SettingsModsTab, Vec<&ModRow>, Vec<SectionConflict>)],
    collapsed: &HashSet<String>,
) -> Vec<String> {
    sections
        .iter()
        .filter(|(tab, _, _)| !collapsed.contains(tab.pref_id()))
        .flat_map(|(_, rows, _)| rows.iter().map(|r| r.instance.clone()))
        .collect()
}

/// One section-local dest conflict: contested dest + contenders in ascending
/// load order (last wins).
pub(crate) struct SectionConflict {
    pub(crate) dest: String,
    pub(crate) instances: Vec<String>,
}

/// `load_conflicts` groups that resolve inside a single section: every
/// contender in `tab`'s section, all the same officialness (an official is
/// pinned first, so a mixed group has no reachable winner). A group with a
/// contender in another section is dropped whole — the engine winner there is
/// decided across sections, which neither this line nor Make-win can express.
pub(crate) fn section_conflicts(
    rows: &[ModRow],
    tab: SettingsModsTab,
    conflicts: &[LoadConflict],
) -> Vec<SectionConflict> {
    if conflicts.is_empty() {
        return Vec::new();
    }
    let members: HashMap<&str, bool> = section_rows(rows, tab)
        .into_iter()
        .map(|r| (r.instance.as_str(), r.official))
        .collect();
    conflicts
        .iter()
        .filter_map(|c| {
            let inside: Vec<String> = c
                .instances
                .iter()
                .filter(|i| members.contains_key(i.as_str()))
                .cloned()
                .collect();
            if inside.len() < 2 || inside.len() != c.instances.len() {
                return None;
            }
            let kind = *members.get(inside[0].as_str())?;
            if inside
                .iter()
                .any(|i| members.get(i.as_str()) != Some(&kind))
            {
                return None;
            }
            Some(SectionConflict {
                dest: c.dest.clone(),
                instances: inside,
            })
        })
        .collect()
}

/// Full per-game permutation: sections in tab order. Any reorder writes the
/// whole set (core validates it is exactly the installed ids).
pub(crate) fn order_with_section(
    rows: &[ModRow],
    tab: SettingsModsTab,
    section: &[String],
) -> Vec<String> {
    SettingsModsTab::ALL
        .iter()
        .flat_map(|t| {
            if *t == tab {
                section.to_vec()
            } else {
                section_rows(rows, *t)
                    .into_iter()
                    .map(|r| r.instance.clone())
                    .collect()
            }
        })
        .collect()
}

/// Section-local swap of `instance` with its neighbor (`delta` −1 up, +1
/// down) as a full permutation. `None` = no swap (edge, unknown instance, or
/// across the official pin boundary: officials stay first in their section).
pub(crate) fn move_in_order(rows: &[ModRow], instance: &str, delta: i32) -> Option<Vec<String>> {
    let row = rows
        .iter()
        .find(|r| r.instance == instance && r.installed)?;
    let tab = SettingsModsTab::for_type(&row.mod_type);
    let mut section: Vec<String> = section_rows(rows, tab)
        .into_iter()
        .map(|r| r.instance.clone())
        .collect();
    let pos = section.iter().position(|i| i == instance)?;
    let swap = pos as i32 + delta;
    if swap < 0 || swap as usize >= section.len() {
        return None;
    }
    let official = |id: &str| {
        rows.iter()
            .find(|r| r.instance == id)
            .is_some_and(|r| r.official)
    };
    if official(&section[pos]) != official(&section[swap as usize]) {
        return None;
    }
    section.swap(pos, swap as usize);
    Some(order_with_section(rows, tab, &section))
}

/// Section-local Make-win: reinsert `instance` directly after the last other
/// member of `group` inside its own section, as a full permutation.
pub(crate) fn make_win_order(
    rows: &[ModRow],
    instance: &str,
    group: &[String],
) -> Option<Vec<String>> {
    let row = rows
        .iter()
        .find(|r| r.instance == instance && r.installed)?;
    let tab = SettingsModsTab::for_type(&row.mod_type);
    let mut section: Vec<String> = section_rows(rows, tab)
        .into_iter()
        .map(|r| r.instance.clone())
        .collect();
    section.retain(|i| i != instance);
    let last = group
        .iter()
        .filter(|i| i.as_str() != instance)
        .max_by_key(|i| section.iter().position(|s| s == *i).unwrap_or(0))?;
    let pos = section.iter().position(|s| s == last)?;
    section.insert(pos + 1, instance.to_string());
    Some(order_with_section(rows, tab, &section))
}
