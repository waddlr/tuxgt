use std::path::PathBuf;

use tuxgt_core::{PackageFile, StageState};

use super::super::instance_facts::InstanceFacts;
use super::super::paint_ids::{InstanceIds, ModIds};

/// File status for a `provided` recipe. Filled in `load_instances`, never
/// during paint. `ready` is false until every pattern is satisfied.
#[derive(Clone)]
pub struct ProvidedRow {
    pub present: Vec<String>,
    pub missing: Vec<String>,
    pub ambiguous: Vec<String>,
    pub ready: bool,
    /// Recipe `source.note` (where to get the files). Empty when unset.
    pub note: String,
}

#[derive(Clone)]
pub struct InstanceRow {
    pub id: String,
    pub label: String,
    /// Recipe `description`. Empty = none; painted on the Settings Mods page.
    pub description: String,
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
    /// Recipe note. Empty unless `source` is `provided`.
    pub note: String,
    /// Present / missing / ambiguous patterns. `None` unless `source` is `provided`.
    pub provided: Option<ProvidedRow>,
    /// T24 precomputed paint ids, built once in `load_instances`.
    pub ids: InstanceIds,
    /// Recipe facts for the Details disclosure. Install names and the
    /// catalog-check row live on `CatalogMeta`, not here.
    pub facts: InstanceFacts,
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

/// An archive open that stopped for a password. Submit retries the same op.
#[derive(Clone)]
pub enum PendingArchivePassword {
    Add {
        path: PathBuf,
        mod_type: &'static str,
        files: bool,
        directories: bool,
        prompt_key: &'static str,
    },
    Provide {
        id: String,
        path: PathBuf,
    },
    Install {
        game: String,
        instance: String,
        adapter: Option<String>,
        with_requires: Option<String>,
        yes: bool,
        /// `true` retries through the update/redownload path.
        redownload: bool,
        force: bool,
        slot: Option<String>,
    },
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
    /// R38: required target game id; rows lock to its arch.
    pub target_game: Option<String>,
    /// Effective bitness (`"32"`/`"64"`) resolved at pick time.
    pub target_arch: Option<String>,
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
    /// Effective loader kind: LoadDLL when true, IncludeFile when false.
    pub loaddll: bool,
    /// Recipe default (`.dll` and not `include`-covered). Slot claim uses
    /// this, so a per-game IncludeFile switch does not drop the injector.
    pub recipe_load: bool,
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
    /// Slot-control label: a proxy stem, the stock DLL name, or `no proxy slot`.
    #[allow(dead_code)]
    pub slot: String,
    /// ReShade, OptiScaler, or a custom recipe that names a slot. The
    /// installed card offers the Slot dropdown. False for Streamline,
    /// addons, shaders, and manifests whose recipe is gone.
    pub slot_capable: bool,
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
    /// `provided` file status for a not-installed row; the picker paints
    /// the missing names while it is not ready. `None` for other sources
    /// and for installed rows.
    pub provided: Option<ProvidedRow>,
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
