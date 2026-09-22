use std::collections::{HashMap, HashSet};

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::VirtualListScrollHandle;
use gpui_kit::*;

use tuxgt_core::{
    DetectSnapshot, EnvKnob, GameIndexRow, GameRow, GpuCard, HostGpu, HostVerifyEntry, KnobRow,
    LoadConflict, Strings, ToolStatus, UpdateStatus,
};

use super::prefs::Prefs;
use super::sys::SysInfo;
use super::tray::{AppCommand, HideState};
use super::tray::{InstallGuard, RecentSnapshot};

use super::notice_store::NoticeStore;
use super::*;

pub struct Shell {
    pub(crate) strings: Strings,
    pub(crate) nav: Nav,
    /// R12: process-level tray state. Shared, not per-window, because the
    /// tray item outlives the window: a Hide ends the GUI process, and the
    /// stub serves the tray until a Show respawns it.
    pub(crate) hide_state: std::sync::Arc<HideState>,
    /// R12: the one channel the window controller drains. The close and
    /// minimize buttons write their decisions here instead of acting on the
    /// window directly.
    pub(crate) tray_commands: std::sync::mpsc::Sender<AppCommand>,
    /// Controller-owned recent-games snapshot, shared so a GUI Play refreshes
    /// the tray menu's Play list. Rebuilt per process — the stub re-reads
    /// it after the handoff — never carried window state.
    pub(crate) recents: RecentSnapshot,
    /// R12: false when no SNI host accepted the item, so the Settings rows
    /// cannot promise a tray and Close stays an ordinary close.
    pub(crate) tray_registered: bool,
    pub(crate) game_tab: GameTab,
    pub(crate) settings_tab: SettingsTab,
    /// Selected inner tab for the Settings Mods page (persisted).
    pub(crate) mods_tab: SettingsModsTab,
    pub(crate) prefs: Prefs,
    pub(crate) sys: SysInfo,
    pub(crate) host_gpu: HostGpu,
    /// Concrete host GPUs for the Settings GPU chooser, detected once with
    /// the other boot probes. Empty on a host without DRM sysfs.
    pub(crate) host_gpus: Box<[GpuCard]>,
    pub(crate) plugins: Box<[PluginRow]>,
    pub(crate) instances: Box<[InstanceRow]>,
    /// Always-held minimal library index. Sidebar, inventory counts, and the
    /// narrow pass read this; full rows live in `games` per page.
    pub(crate) index: Box<[GameIndexRow]>,
    /// Stored base filter+sort over `index`: all Library filters except text,
    /// hidden, and disabled managers (those narrow at paint). Recomputed on
    /// Library entry and whenever a base filter changes there.
    pub(crate) base_filtered: Vec<usize>,
    /// Page-scoped full rows. Library: all (same order as `index`, so base
    /// positions align); Game: the selected row only; Settings: empty.
    pub(crate) games: Box<[GameRow]>,
    pub(crate) tiers: HashMap<String, String>,
    /// E84: cached ProtonDB summaries for the Info tab (cache-only).
    pub(crate) proton: HashMap<String, ProtonSummary>,
    pub(crate) awacy: HashMap<String, library::AwacyFlag>,
    pub(crate) hover_card: Option<String>,
    /// Enabled-mod counts for every game (cheap; no file lists).
    pub(crate) mod_counts: HashMap<String, usize>,
    /// Manifest + instance rows for the selected game only.
    pub(crate) mods: HashMap<String, Vec<ModRow>>,
    pub(crate) applied: HashMap<String, bool>,
    pub(crate) handle: HashMap<String, bool>,
    pub(crate) session_payload: HashMap<String, bool>,
    pub(crate) pending_note: Option<Note>,
    pub(crate) knobs: Box<[&'static EnvKnob]>,
    /// T24 precomputed knob paint ids by knob id. Rebuilt wherever `knobs`
    /// is assigned (boot, plugin toggle).
    pub(crate) knob_ids: HashMap<&'static str, KnobIds>,
    pub(crate) knob_values: HashMap<String, String>,
    pub(crate) knob_enabled: HashMap<String, bool>,
    pub(crate) global_knobs: HashMap<String, KnobRow>,
    /// Hero counts for the selected game, without the rows. Refreshed on
    /// game select and derived from reloaded maps after every env write.
    pub(crate) knob_count: usize,
    pub(crate) custom_count: usize,
    pub(crate) game_env_advance: bool,
    /// General → Advanced accordion.
    pub(crate) general_advanced: bool,
    pub(crate) settings_env_advance: bool,
    pub(crate) custom_env: Box<[(String, String)]>,
    pub(crate) secret_states: HashMap<String, bool>,
    /// E61: unpack-tool availability (`all_tools()` order) for the Settings
    /// Host tools block. Probed once per Settings entry, never on paint.
    pub(crate) tools: Box<[ToolStatus]>,
    /// E67: start-of-session host-install verify (`verify_host_install`
    /// for `data_dir()` / `$HOME`). Loaded once at boot, never blocks the
    /// window, never toasts. E69 renders it in the Settings TuxGT Install
    /// card and rewrites it after install/uninstall.
    pub(crate) host_install: Box<[HostVerifyEntry]>,
    /// E69: no `$PREFIX/config/host-install.toml` at the last verify, so
    /// Uninstall has no file list. Still a drift list: every intended path
    /// is verified, inventory or not.
    pub(crate) host_inventory_missing: bool,
    /// E44: single masked key editor for the keyring source (one InputState).
    pub(crate) secret_input: Entity<InputState>,
    /// `true` while replacing a stored key.
    pub(crate) secret_editing: bool,
    /// SteamGridDB key sub-section expansion (gear-gated, enabled only).
    pub(crate) show_steamgriddb_settings: bool,
    /// R14 Core Plugins registry: the HTTPS URL Add input, the added
    /// registries, and their manifest rows joined with `remote.toml`
    /// install state. Reloaded on tab entry and after every mutation.
    pub(crate) registry_url_input: Entity<InputState>,
    pub(crate) registries: Box<[RegistryRow]>,
    pub(crate) remote_plugins: Box<[RemotePluginRow]>,
    /// Registry network/disk work in flight. One at a time: a second click
    /// while a clone or payload fetch runs would race one `remote.toml`.
    pub(crate) registry_busy: bool,
    /// The single mutation whose result failed, named so the row keeps
    /// painting its old version. Cleared by the next successful mutation.
    pub(crate) registry_error: Option<(String, String)>,
    /// R14 install/update/remove consent (registry ids, no game).
    pub(crate) registry_confirm: Option<PendingRegistryConfirm>,
    /// E91 Add-Mod form Name input. The remaining live form state (files,
    /// dests, includes, requires) lives in the `add_form` entity.
    pub(crate) instance_id_input: Entity<InputState>,
    pub(crate) add_form: Option<Entity<AddForm>>,
    pub(crate) archive_password_input: Entity<InputState>,
    pub(crate) pending_archive_password: Option<PendingArchivePassword>,
    pub(crate) add_sync_queued: bool,
    /// E91 per-row dest inputs for the open Add/Rescan form, index-aligned
    /// with its files. Built once in render (creation needs `window`) and
    /// bound via subscriptions; `add_dest_for` marks which form they belong
    /// to. Unchecked rows keep their input value; save ignores them.
    pub(crate) add_dest_inputs: Box<[Entity<InputState>]>,
    pub(crate) add_dest_for: Option<Entity<AddForm>>,
    /// E44: Launch-tab Steam-AppID overlay editor.
    pub(crate) appid_input: Entity<InputState>,
    /// E44: stored overlay wrappers for the selected game, table order.
    pub(crate) wrappers: Box<[String]>,
    /// Launch legality for the selected game, from persisted manifests +
    /// game env + wrappers (core `game_launch_needs`, same inputs the old
    /// map-backed radio read). Loaded with `wrappers`; refreshed after any
    /// mod/env/wrapper mutation so General-tab arming never reads dropped
    /// per-tab rows.
    pub(crate) launch_needs: tuxgt_core::LaunchNeeds,
    /// Cached store launch config for the selected game (About read-only
    /// reference). Loaded with `wrappers`.
    pub(crate) launch_cfg: Option<tuxgt_core::GameLaunchConfig>,
    /// E63: family templates for the Settings Mods Add flow, loaded on
    /// Settings Mods-tab entry and refreshed by Re-sync official.
    pub(crate) family_templates: Box<[tuxgt_core::ModTemplate]>,
    /// HDR packs mint card (Settings Mods ReShade tab) + filter input.
    pub(crate) family_mint: Option<FamilyMint>,
    pub(crate) family_filter_input: Entity<InputState>,
    pub(crate) extras_mint: Option<ReshadePackagesMint>,
    pub(crate) extras_filter_input: Entity<InputState>,
    /// ReShade pack UX: per-list filter inputs (packs, picker, installed).
    pub(crate) packs_filter_input: Entity<InputState>,
    pub(crate) picker_filter_input: Entity<InputState>,
    pub(crate) installed_filter_input: Entity<InputState>,
    /// Shared file-preview cache keyed `"mod:<id>"` + open toggle set.
    pub(crate) file_preview_cache: HashMap<String, (Vec<String>, usize)>,
    pub(crate) preview_open: HashSet<String>,
    /// Preview keys showing the full list past the 10-row default.
    pub(crate) preview_expanded: HashSet<String>,
    /// Mod ids whose payload preview walk errored (paints preview-error), keyed
    /// like the cache (`"mod:<id>"`).
    pub(crate) preview_errors: HashSet<String>,
    /// `gui.mod-config-edit`: open inline config editor (`None` = closed).
    pub(crate) config_edit: Option<ConfigEdit>,
    /// Parked navigation while the editor holds unsaved edits (`None` = no
    /// discard modal). Confirm replays the original op, Cancel stays.
    pub(crate) config_nav_pending: Option<ConfigNavPending>,
    /// Editor buffer for the open config file.
    pub(crate) config_input: Entity<TextareaState>,
    /// External editor opened for the current file: refocus re-reads pills.
    pub(crate) config_external_open: bool,
    /// Level the external editor was opened for: the in-app editor closes on
    /// external open, so the focus hook refreshes from this, not `config_edit`.
    pub(crate) config_external_level: Option<ConfigLevel>,
    /// E44: stored Steam-AppID overlay for the selected game.
    pub(crate) appid_stored: Option<String>,
    /// Steam name-search state for the Heroic/manual AppID row (unset only).
    /// Hits hold (appid, name) strings — no core type in GUI state.
    pub(crate) appid_searching: bool,
    pub(crate) appid_searched: bool,
    pub(crate) appid_hits: Box<[(String, String)]>,
    pub(crate) detect: Box<[DetectSnapshot]>,
    /// R62: stored extra correlator exes for the selected game, sort order.
    pub(crate) extras: Box<[String]>,
    /// Game id whose extras `extras` holds; `None` = not loaded.
    pub(crate) extra_exe_for: Option<String>,
    /// R62: extra-exe add-path input.
    pub(crate) extra_exe_input: Entity<InputState>,
    /// Game id whose detection snapshot `detect` holds; `None` = not loaded.
    pub(crate) detect_for: Option<String>,
    pub(crate) override_edit: Option<&'static str>,
    pub(crate) override_input: Entity<InputState>,
    pub(crate) custom_env_input: Entity<InputState>,
    pub(crate) custom_env_adding: bool,
    pub(crate) redetect_confirm: bool,
    /// R34: manual id awaiting remove confirm (`None` = no dialog).
    pub(crate) manual_remove: Option<String>,
    pub(crate) pending_confirm: Option<PendingConfirm>,
    /// E62 inline Install picker: open flag + checked ids in check order
    /// (the batch installs in that order).
    pub(crate) mods_picker_open: bool,
    pub(crate) picker_checked: Vec<String>,
    /// Picker sections the user collapsed (`SettingsModsTab::pref_id`).
    /// Empty = all expanded; a collapsed section is out of Select Visible scope.
    pub(crate) picker_collapsed: HashSet<String>,
    /// Installed sections the user collapsed (`SettingsModsTab::pref_id`).
    /// Empty = all expanded; a collapsed section paints its header only and
    /// is out of Uninstall-visible scope.
    pub(crate) installed_collapsed: HashSet<String>,
    /// E62 batch install: instances still to install, in check order. A
    /// `NeedConfirm` / `MissingRequires` stops the batch (E34 confirm card);
    /// Confirm resumes it, Cancel drops the rest.
    pub(crate) install_queue: Vec<String>,
    /// R12: the veto a parked batch keeps armed. A finished transfer hands
    /// its guard here while this window still owns unconfirmed work, so a
    /// Hide cannot drop the confirm card or the instances behind it; the
    /// work resolving (`clear_pending_confirm`, a drained batch) drops it
    /// and Close is free again.
    pub(crate) install_hold: Option<InstallGuard>,
    /// R70: the in-flight `(game, instance)` spawn. Set for the running
    /// install and while an E34 card parks it; picker Install appends
    /// instead of starting a second spawn.
    pub(crate) install_current: Option<(String, String)>,
    /// E102: Live card per in-flight `(game, instance)` install, plus the cell
    /// its transfer thread writes bytes into.
    pub(crate) install_live: HashMap<(String, String), game::InstallLive>,
    /// E102: bumped per Live card so a reused card's old pump task exits.
    pub(crate) install_live_seq: u64,
    /// Bulk uninstall queue mirroring the install pair.
    pub(crate) uninstall_queue: Vec<String>,
    pub(crate) uninstall_current: Option<(String, String)>,
    pub(crate) uninstall_done: usize,
    /// R32 last `check_update` result per (game, instance). Absent = not yet
    /// checked (card stays quiet while `mod_update_pending` holds the key,
    /// same empty box as `UpToDate`, so landing the verdict never shifts
    /// the card); `UpToDate` stays quiet on the card.
    pub(crate) mod_updates: HashMap<(String, String), UpdateStatus>,
    /// E104 catalog poll: last run, in-flight guard, per-Mod `available`
    /// ids (Attention + page badges). Unknown is never stored — it stays
    /// on the page card only. `update_poll_gen` invalidates stale 2h
    /// re-arm timers (a Settings Update refresh re-arms its own).
    pub(crate) update_last_poll: Option<std::time::Instant>,
    pub(crate) update_polling: bool,
    pub(crate) update_poll_gen: u64,
    /// Recounts from per-game Update. A poll that started before an entry
    /// uses the entry for that game instead of its own snapshot.
    pub(crate) game_attention_settled: HashMap<String, super::poll_attention::GameSettle>,
    pub(crate) catalog_updates: std::collections::HashSet<String>,
    /// Installed names, AppID display names, and the last catalog-check
    /// row for the Settings Mods Details disclosure.
    pub(crate) catalog_meta: CatalogMeta,
    /// R32 instances with an in-flight `check_update`.
    pub(crate) mod_update_pending: HashSet<(String, String)>,
    /// R33 last `stage_status` rows per game id.
    pub(crate) mod_stage: HashMap<String, Vec<StageRow>>,
    /// Mods-tab `load_conflicts` per game id. Filled in `refresh_mod_extra`
    /// (same spot as `mod_stage`); paint reads the cache, never the disk.
    pub(crate) mod_conflicts: HashMap<String, Box<[LoadConflict]>>,
    /// Epoch per game id for the stage/conflict caches: every sync write
    /// (forced refresh, resync, row merge) bumps it; a background rehash
    /// snapshots it at spawn and drops its result when it moved, so a slow
    /// snapshot can never overwrite newer pills.
    pub(crate) mod_extra_epoch: HashMap<String, u64>,
    /// E90 per-card dest Accordion expansion, keyed `"{game}/{instance}"`.
    /// Empty = all collapsed (default closed).
    pub(crate) mod_files_open: HashSet<String>,
    /// R37 cache of the selected game's persisted Install adapter
    /// (`games.adapter`). Never a source of truth: it is re-read from the
    /// `GameRow` on selection and after every conversion result, and no
    /// install path consumes it.
    pub(crate) adapter_choice: String,
    pub(crate) filters: Filters,
    /// Manager ids whose provider plugin is currently disabled.
    /// Loaded once per startup/rescan (not per paint); display-layer only —
    /// rows stay in the DB per game-provider.md E12, re-enable restores.
    pub(crate) disabled_managers: std::collections::HashSet<String>,
    pub(crate) selected: Option<String>,
    pub(crate) search: Entity<InputState>,
    pub(crate) page_scroll: ScrollHandle,
    /// Tracked offsets for the capped inner lists (chain, don't trap: wheel
    /// over a list stops at the page only when the list has room).
    pub(crate) inner_scrolls: [ScrollHandle; 3],
    /// About Launch-Env list offset (three visible rows + own scroll).
    pub(crate) about_env_scroll: ScrollHandle,
    /// Sidebar virtual list offset. `v_virtual_list` builds a fresh handle per
    /// frame; without this the list snaps back to the top (E37).
    pub(crate) sidebar_scroll: VirtualListScrollHandle,
    /// Content-y of the User Packs cards inside the page scroller, from the
    /// last layout. The page scrolls the whole tab; off-screen cards are a
    /// spacer of this height instead of real rows.
    pub(crate) pack_list_top: std::rc::Rc<std::cell::Cell<Option<Pixels>>>,
    /// One-shot: a zero page viewport retries once after the first layout.
    pub(crate) pack_viewport_armed: bool,
    pub(crate) status: String,
    /// E101: owned notification store (Live / Activity / Attention).
    /// Runtime only — process death wipes it.
    pub(crate) notices: NoticeStore,
    /// E101: sidecar open (bell toggle; click-catcher / Escape close).
    pub(crate) sidecar_open: bool,
    /// E101: shell focus handle so Escape reaches the root while the
    /// sidecar is open even when a page input holds focus.
    pub(crate) focus: FocusHandle,
    pub(crate) hist_back: Vec<Place>,
    pub(crate) hist_fwd: Vec<Place>,
    /// Last Library grid/list, independent of `prefs.view` (which is the page).
    pub(crate) library_list: bool,
    /// Applied library grid `(card_w, card_h, cols)`. Frozen while the
    /// page width is moving; `tick_grid_metrics` settles after idle.
    pub(crate) grid_metrics: Option<(f32, f32, usize)>,
    /// Desired metrics waiting on the settle timer (debounce target).
    pub(crate) grid_metrics_pending: Option<(f32, f32, usize)>,
    /// Bumped to cancel stale settle timers.
    pub(crate) grid_metrics_gen: u64,
}
