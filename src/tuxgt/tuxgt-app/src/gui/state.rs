use std::path::PathBuf;

use tuxgt_core::{PackageFile, StageState};

use super::paint_ids::{InstanceIds, ModIds};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EnvPage {
    Game,
    Global,
}

#[derive(Clone)]
pub enum Note {
    // T23: no live constructor; kept so the toast map stays four-kind.
    #[allow(dead_code)]
    Info(String),
    Ok(String),
    Warn(String),
    Err(String),
}

#[derive(Clone)]
pub struct PluginRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct InstanceRow {
    pub id: String,
    pub label: String,
    pub mod_type: String,
    pub source: String,
    pub official: bool,
    pub enabled: bool,
    /// Recipe `EffectFiles` (recipe drops applied) for the `Effects` preview.
    pub effect_files: Box<[String]>,
    /// One catalog file name for the archive; `None` = not derivable.
    pub asset: Option<String>,
    /// Local payload holds files the recipe keeps — the `Files` preview gate.
    pub payload_present: bool,
    /// T24 precomputed paint ids, built once in `load_instances`.
    pub ids: InstanceIds,
}

/// E91 Add-Mod form state (inline panel).
/// Name doubles as id slug source + label; `includes[i]` forces that file's
/// DLL dest to IncludeFile — a Rescan form seeds it from the recipe's
/// `include` list; `requires` holds picked ReShade Mod ids.
/// `select_visible` mirrors whether every file row is checked (first-row
/// checkbox, hidden for a single file); unchecking a row drops its dest.
pub struct AddForm {
    pub rescan_id: Option<String>,
    pub password: Option<String>,
    pub mod_type: String,
    pub path: PathBuf,
    pub name: String,
    pub files: Box<[PackageFile]>,
    pub select_visible: bool,
    pub includes: Box<[bool]>,
    pub requires: Vec<String>,
}

#[derive(Clone)]
pub struct PendingArchivePassword {
    pub path: PathBuf,
    pub mod_type: &'static str,
    pub files: bool,
    pub directories: bool,
    pub prompt_key: &'static str,
}
/// HDR packs mint card: merged RenoDX/Luma live assets. `assets` fills from
/// both family lists on open; `selected` holds checked asset keys
/// (`template_id + name`); `game_for` maps asset key to the bound game id.
#[derive(Clone)]
pub struct FamilyAssetRow {
    pub template_id: String,
    pub vendor: String,
    pub name: String,
    pub tag: String,
}
#[derive(Clone)]
pub struct FamilyMint {
    pub loading: bool,
    pub assets: Box<[FamilyAssetRow]>,
    pub err: Option<String>,
    pub selected: Vec<String>,
    pub game_for: std::collections::HashMap<String, String>,
}
impl FamilyAssetRow {
    pub fn key(&self) -> String {
        format!("{}:{}", self.template_id, self.name)
    }
}
/// ReShade extras mint kind filter.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ExtrasKindFilter {
    #[default]
    All,
    Effects,
    Addons,
}
impl ExtrasKindFilter {
    /// Row order for the extras card: `(element id, filter, label key)`.
    /// The caller compares against the active filter for the primary style.
    pub fn buttons() -> [(&'static str, Self, &'static str); 3] {
        [
            (
                "reshade-extras-filter-all",
                Self::All,
                "gui-reshade-extras-filter-all",
            ),
            (
                "reshade-extras-filter-effects",
                Self::Effects,
                "gui-reshade-extras-filter-effects",
            ),
            (
                "reshade-extras-filter-addons",
                Self::Addons,
                "gui-reshade-extras-filter-addons",
            ),
        ]
    }
}
/// E96: open ReShade extras mint card. `packages` fills from the live INIs;
/// `selected` holds newly checked mintable keys (present rows stay locked).
#[derive(Clone)]
pub struct ReshadePackagesMint {
    pub loading: bool,
    pub packages: Box<[tuxgt_core::ReshadePackage]>,
    pub err: Option<String>,
    pub selected: Vec<String>,
    pub kind_filter: ExtrasKindFilter,
}

#[derive(Clone)]
pub struct ModFileRow {
    /// Exact stored dest string; the keep toggle matches on it.
    pub dest: String,
    /// Depot-side source path; the row paints `basename(source)` when it
    /// differs from the dest basename (E78 merged row).
    pub source: String,
    pub enabled: bool,
    /// Required dests cannot be omitted: checkbox locked on.
    pub required: bool,
    /// `.dll` dests prewire as `LoadDLL`, everything else `IncludeFile`.
    pub loaddll: bool,
}

#[derive(Clone)]
pub struct ModEnvRow {
    /// Exact stored env key; the keep toggle matches on it.
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct ModRow {
    pub instance: String,
    pub label: String,
    pub mod_type: String,
    /// Official recipe (`mods/official/`). Pinned first inside its Mods-tab
    /// section; unknown for a manifest whose recipe is gone.
    pub official: bool,
    pub adapter: String,
    pub enabled: bool,
    pub files: usize,
    /// Per-game order; lower loads first, later wins same-dest. 0 for not-installed rows.
    pub load_order: i64,
    /// True when a FileManifest exists. False = registered but not installed.
    pub installed: bool,
    /// Dest proxy name (`dxgi.dll`) or `no proxy slot`. Never empty.
    /// Kept for diagnostics; not painted since E75 (Mode/Provides pills say it).
    #[allow(dead_code)]
    pub slot: String,
    /// Requires/conflicts text from the E13 graph. Empty when the diagnosis
    /// mentions this instance nowhere (R50: card hides the line then).
    pub graph: String,
    /// Manifest dests with their E64 keep state.
    pub file_entries: Box<[ModFileRow]>,
    /// Manifest env rows with their E74 keep state.
    pub env_entries: Box<[ModEnvRow]>,
    /// Recipe `EffectFiles` (recipe drops applied) for the `Effects` preview;
    /// empty for hand-written recipes and Mods minted before the key existed.
    pub effect_files: Box<[String]>,
    /// One catalog file name for the archive (manual-url basename, or a
    /// wildcard-free github asset); `None` = not derivable.
    pub asset: Option<String>,
    /// Local payload holds files the recipe keeps — the `Files` preview gate.
    /// Probed while the rows are built, never during paint.
    pub payload_present: bool,
    /// T24 precomputed paint ids, built once in `load_mods_for`.
    pub ids: ModIds,
}

/// `Files` preview content: the local payload walk, or one catalog file name
/// when there is no payload yet (addon archives).
#[derive(PartialEq, Eq, Debug)]
pub enum FilesPreview<'a> {
    Payload,
    Asset(&'a str),
}

/// Whether a row paints the `Effects` preview: effect Mods whose recipe
/// carries an `EffectFiles` list (E96 pack mint).
pub(crate) fn shows_effects(mod_type: &str, effect_files: &[String]) -> bool {
    mod_type == "effect" && !effect_files.is_empty()
}

/// Whether a row paints the `Files` preview, and with what: the payload walk
/// only when the payload has files, else the derivable addon archive name.
/// `None` = nothing to show, so no button.
pub(crate) fn files_preview<'a>(
    mod_type: &str,
    payload_present: bool,
    asset: Option<&'a str>,
) -> Option<FilesPreview<'a>> {
    if payload_present {
        return Some(FilesPreview::Payload);
    }
    (mod_type == "reshade_addon")
        .then_some(asset)
        .flatten()
        .map(FilesPreview::Asset)
}

/// Last path segment of a download URL, when it names something.
pub(crate) fn url_file_name(url: &str) -> Option<String> {
    url.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One catalog file name from a recipe source: a manual URL's last segment, or
/// a github asset glob that names a single file.
pub(crate) fn source_asset(source: &tuxgt_core::SourceRef) -> Option<String> {
    match source {
        tuxgt_core::SourceRef::ManualUrl { url } => url_file_name(url),
        tuxgt_core::SourceRef::Github { asset_glob, .. } if !asset_glob.contains('*') => {
            Some(asset_glob.clone())
        }
        _ => None,
    }
}

/// R33 per-file staging state for the Mods tab. Mirrors
/// `tuxgt_core::StageLine`; `Clone` so the Shell cache can hand rows to paint.
#[derive(Clone)]
pub struct StageRow {
    pub instance: String,
    pub file: String,
    pub state: StageState,
}

/// E34 pending install/enable/uninstall confirm. `None` = no dialog.
/// Confirm retries the stashed op; Cancel drops it (disk unchanged).
#[derive(Clone)]
pub enum ConfirmOp {
    Install {
        with_requires: Option<String>,
    },
    /// R32 update reinstall (`--redownload`); retried with `yes` after a
    /// foreign-dest confirm.
    Update {
        adapter: String,
    },
    /// `gui.mod-config-edit`: Update after a config-edit confirm; retried
    /// with staging force so confirmed wipes land. `foreign_done` sequences
    /// the two consents: false = config leg (retry keeps `yes=false` so a
    /// foreign NeedConfirm still parks), true = foreign leg (retry `yes`
    /// after the foreign card consented).
    UpdateForce {
        adapter: String,
        foreign_done: bool,
    },
    Enable {
        on: bool,
    },
    /// E91 proxy-slot rewrite; retried with `yes` after a foreign-dest confirm.
    Slot {
        slot: String,
    },
    Uninstall,
    /// E64 per-dest keep; retried with `yes` after a foreign-dest confirm.
    FileKeep {
        dest: String,
        on: bool,
    },
    /// E69 Settings host install; retried with `yes` after a modified-path
    /// confirm. No game or instance: the Settings card renders it.
    HostInstall,
    /// E69 Settings host uninstall; always confirmed, then retried with
    /// `yes`. No game or instance: the Settings card renders it.
    HostUninstall,
}

/// Store-client stop authorization for Apply/Restore. Confirm stops the
/// running client, writes, and restarts it; Cancel leaves disk unchanged.
#[derive(Clone)]
pub enum ClientStopOp {
    ApplyMode,
    VanillaMode,
    HookMode,
    EnablePlay { hook: bool },
}

#[derive(Clone)]
pub enum PendingConfirm {
    /// Core returned `NeedConfirm`: retry the op with `yes=true`.
    Overwrite {
        game: String,
        instance: String,
        op: ConfirmOp,
        dests: Box<[String]>,
    },
    /// Core returned `MissingRequires`: install the chosen instance first.
    Requires {
        game: String,
        instance: String,
        req_type: String,
        candidates: Box<[(String, String)]>,
        chosen: Option<String>,
    },
    /// E69 Settings TuxGT install/uninstall. No game or instance: the
    /// Settings TuxGT Install card renders it via the E34 `confirm_card`;
    /// Confirm retries the stashed op with `yes=true`, Cancel drops it.
    Host { op: ConfirmOp, paths: Box<[String]> },
    /// Running store client must stop before Apply/Restore writes.
    ClientStop { game: String, op: ClientStopOp },
}

impl PendingConfirm {
    pub fn game(&self) -> &str {
        match self {
            PendingConfirm::Overwrite { game, .. } => game,
            PendingConfirm::Requires { game, .. } => game,
            PendingConfirm::ClientStop { game, .. } => game,
            // Host confirms never paint on a game page.
            PendingConfirm::Host { .. } => "",
        }
    }
}

#[derive(Clone, Default)]
pub struct Filters {
    pub store: Option<String>,
    pub platform: Option<String>,
    pub protondb: Option<String>,
    pub mods_only: bool,
    pub search: String,
    /// SortMode id from gui::library ("", "mods", "protondb", "recent").
    pub sort: String,
    /// Show only AWACY-flagged titles.
    pub awacy_only: bool,
    /// Show effective-hidden games (hidden by default).
    pub show_hidden: bool,
}
