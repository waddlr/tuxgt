mod card;
mod nav;

use std::path::PathBuf;

use gpui_kit::assets::IconName as FullIconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;

use tuxgt_core::{config_dir, data_dir, is_config_text, FluentArgs};

use super::{widgets, Shell};

/// Edit level for the inline config card (`gui.mod-config-edit`): the
/// shared Settings payload vs one game's staged copy. Instance ids are
/// catalog mod ids, so either level derives its preview-cache key as
/// `mod:<id>`.
#[derive(Clone, PartialEq)]
pub(crate) enum ConfigLevel {
    Global { id: String },
    Staged { game: String, instance: String },
}

impl ConfigLevel {
    pub(crate) fn mod_id(&self) -> &str {
        match self {
            ConfigLevel::Global { id } => id,
            ConfigLevel::Staged { instance, .. } => instance,
        }
    }
}

/// Open inline config editor (`None` = closed). One buffer at a time:
/// opening a second file replaces the first with a status note.
#[derive(Clone)]
pub(crate) struct ConfigEdit {
    pub(crate) level: ConfigLevel,
    pub(crate) rel: String,
    pub(crate) external_only: bool,
}

/// Files over this size open externally-only (never loaded into the
/// Textarea).
const CONFIG_INLINE_CAP: u64 = 1 << 20;

fn config_abs_path(level: &ConfigLevel, rel: &str) -> tuxgt_core::Result<PathBuf> {
    match level {
        ConfigLevel::Global { id } => {
            tuxgt_core::payload_config_path(&config_dir(), &data_dir(), id, rel)
        }
        ConfigLevel::Staged { game, instance } => {
            tuxgt_core::check_rel(rel)?;
            Ok(tuxgt_core::stage_dir(&data_dir(), game, instance).join(rel))
        }
    }
}

impl Shell {
    /// Mount the inline editor for one config file. A missing staged file
    /// mounts with an empty buffer (Save creates it); a too-large or
    /// non-UTF8 file mounts externally-only. A missing payload file errors
    /// instead of mounting: creating depot files is not an edit.
    pub(crate) fn open_config_ui(
        &mut self,
        level: ConfigLevel,
        rel: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(action = "open-config", rel = rel.as_str());
        // Re-opening the mounted file is a no-op: the buffer may hold
        // unsaved typing that a disk re-read would wipe.
        if let Some(open) = self.config_edit.as_ref() {
            if open.level == level && open.rel == rel {
                cx.notify();
                return;
            }
        }
        // Resolve before mutating: a failed open leaves the mounted editor
        // and its buffer untouched.
        let path = match config_abs_path(&level, &rel) {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        };
        if let Some(open) = self.config_edit.as_ref() {
            let mut args = FluentArgs::new();
            args.set("file", open.rel.clone());
            self.status = self
                .strings
                .get_args("gui-note-config-discarded", Some(&args));
        }
        let mut external_only = false;
        let text = match std::fs::metadata(&path) {
            Ok(m) if m.len() > CONFIG_INLINE_CAP => {
                external_only = true;
                String::new()
            }
            Ok(_) => match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => {
                    external_only = true;
                    String::new()
                }
            },
            Err(_) => String::new(),
        };
        if !external_only {
            self.config_input.update(cx, |inp, cx| {
                inp.set_value(text, window, cx);
            });
        }
        self.config_edit = Some(ConfigEdit {
            level,
            rel,
            external_only,
        });
        // Acting in place supersedes a parked navigate-away confirm.
        self.config_nav_pending = None;
        // Takeover paints at the top: land the page there on open.
        self.scroll_page_top();
        cx.notify();
    }

    /// Save the buffer over the mounted file. Global saves fan out to
    /// installed games in the background (non-force; per-game touches
    /// preserved with a status note). Staged saves flip the pill via the
    /// existing stage-cache refresh.
    pub(crate) fn save_config_ui(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.config_edit.clone() else {
            return;
        };
        if edit.external_only {
            return;
        }
        let text = self.config_input.read(cx).value().to_string();
        let path = match config_abs_path(&edit.level, &edit.rel) {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.status = format!("{e}");
                cx.notify();
                return;
            }
        }
        // `atomic_write` stays crate-internal; a same-dir create is enough
        // for hand-sized configs.
        if let Err(e) = std::fs::write(&path, text.as_bytes()) {
            self.status = format!("{e}");
            cx.notify();
            return;
        }
        tracing::debug!(action = "save-config", rel = edit.rel.as_str());
        match edit.level {
            ConfigLevel::Global { id } => {
                self.config_edit = None;
                self.config_nav_pending = None;
                // Refill, never bust: the open preview would paint the
                // error line on a missing key, and names survive a content
                // edit (an external delete still refreshes via the walk).
                self.fill_file_preview(&id);
                self.refresh_selected_mods();
                let id_bg = id.clone();
                cx.spawn(async move |this, cx| {
                    let outcome = cx
                        .background_spawn(async move {
                            tuxgt_core::push_global_edits_all(&data_dir(), &id_bg)
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        let mut updated = 0usize;
                        let mut preserved = 0usize;
                        let mut first_err: Option<String> = None;
                        for (game, result) in outcome {
                            match result {
                                Ok(rep) => {
                                    updated += rep.updated.len();
                                    preserved += rep.preserved.len();
                                }
                                Err(e) => {
                                    if first_err.is_none() {
                                        first_err = Some(format!("{game}: {e}"));
                                    }
                                }
                            }
                        }
                        let mut args = FluentArgs::new();
                        args.set("updated", updated.to_string());
                        args.set("preserved", preserved.to_string());
                        this.status = this
                            .strings
                            .get_args("gui-status-config-pushed", Some(&args));
                        if let Some(e) = first_err {
                            this.status = format!("{} — {e}", this.status);
                        }
                        if let Some(gid) = this.selected.clone() {
                            if this.mods.contains_key(&gid) {
                                this.refresh_mod_extra(&gid, cx, true);
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
                cx.notify();
            }
            ConfigLevel::Staged { game, .. } => {
                self.config_edit = None;
                self.config_nav_pending = None;
                self.refresh_mod_extra(&game, cx, true);
                cx.notify();
            }
        }
    }

    pub(crate) fn cancel_config_ui(&mut self, cx: &mut Context<Self>) {
        self.config_edit = None;
        self.config_nav_pending = None;
        cx.notify();
    }
    /// Staged-level Edit button for one file row (`None` unless the dest is
    /// an enabled allowlisted text config — omitted dests have no staged
    /// file, hand-dropped unmanaged files are not manifest dests).
    /// Icon-only (`file-pen-line`); the label survives as the tooltip.
    /// Mounts left of the sync pill at the call site.
    pub(crate) fn staged_edit_button(
        &self,
        game_id: &str,
        instance: &str,
        dest: &str,
        enabled: bool,
        view: Entity<Self>,
        cx: &App,
    ) -> Option<AnyElement> {
        if !(enabled && is_config_text(dest)) {
            return None;
        }
        let edit_game = game_id.to_string();
        let edit_instance = instance.to_string();
        let edit_dest = dest.to_string();
        let edit_view = view.clone();
        let tip = self.strings.get("gui-action-edit-config");
        Some(
            widgets::btn(SharedString::from(format!("ec-{instance}-{dest}")), cx)
                .secondary()
                .child(Icon::new(FullIconName::FilePenLine).small())
                .tooltip(tip)
                .on_click(move |_, window, cx| {
                    edit_view.update(cx, |this, cx| {
                        this.open_config_ui(
                            ConfigLevel::Staged {
                                game: edit_game.clone(),
                                instance: edit_instance.clone(),
                            },
                            edit_dest.clone(),
                            window,
                            cx,
                        );
                    });
                })
                .into_any_element(),
        )
    }

    /// Open Global editor id, if any: Settings boxes take over their tab
    /// content for it (add-form pattern — the page IS the editor).
    pub(crate) fn open_global_id(&self) -> Option<String> {
        match &self.config_edit.as_ref()?.level {
            ConfigLevel::Global { id } => Some(id.clone()),
            ConfigLevel::Staged { .. } => None,
        }
    }

    /// Open Staged instance for this game, if any: the game Mods tab takes
    /// over its content for it.
    pub(crate) fn open_staged_for(&self, game: &str) -> Option<String> {
        match &self.config_edit.as_ref()?.level {
            ConfigLevel::Staged { game: g, instance } if g == game => Some(instance.clone()),
            _ => None,
        }
    }

}
