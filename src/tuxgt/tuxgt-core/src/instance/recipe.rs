use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub(crate) const MODS_TOML: &str = "mods.toml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Plan {
    Install,
    Preload,
    ProtonEnv,
}

impl Plan {
    pub fn as_str(self) -> &'static str {
        match self {
            Plan::Install => "install",
            Plan::Preload => "preload",
            Plan::ProtonEnv => "proton_env",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Self> {
        match s {
            "install" => Ok(Plan::Install),
            "preload" => Ok(Plan::Preload),
            "proton_env" => Ok(Plan::ProtonEnv),
            _ => Err(Error::InvalidInstance(format!("unknown plan: {s}"))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceRef {
    Github {
        owner: String,
        repo: String,
        asset_glob: String,
        /// Release tag; `None` = latest non-prerelease release (or the
        /// newest non-draft prerelease when `prerelease` follows).
        tag: Option<String>,
        /// E63: follow the newest non-draft prerelease release. Rejected
        /// together with `tag` or `sha256` at parse.
        prerelease: bool,
    },
    Local {
        path: String,
    },
    ManualUrl {
        url: String,
    },
}

impl SourceRef {
    pub fn type_str(&self) -> &'static str {
        match self {
            SourceRef::Github { .. } => "github",
            SourceRef::Local { .. } => "local",
            SourceRef::ManualUrl { .. } => "manual_url",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mod {
    pub id: String,
    pub mod_type: String,
    pub label: String,
    pub source: SourceRef,
    pub plans_allowed: Box<[Plan]>,
    pub sha256: Option<String>,
    /// Ordered payload rules: which extracted files enter the manifest.
    pub payload: Box<[PayloadRule]>,
    /// Case-insensitive display-name globs this recipe applies to.
    /// Empty = any game.
    pub games: Box<[String]>,
    /// Nonzero Steam AppIDs this recipe applies to (E63). Empty = none.
    pub appids: Box<[u32]>,
    /// Other Mod ids this recipe needs installed (E85). Empty = none.
    pub requires: Box<[String]>,
    /// Archive-relative src → dest. Empty = type dest rules only.
    pub dests: BTreeMap<String, String>,
    /// Proxy slot inferred from Remap, or explicit in the recipe (E88).
    /// `None` = no slot (unknown stem, or none/ conflicting Remap).
    pub slot: Option<String>,
    /// Dests forced to IncludeFile even when they look like DLLs (E88).
    pub include: Box<[String]>,
    /// Extra env contributed when installed (E74). Empty = none.
    pub env: BTreeMap<String, String>,
    /// Extra dest component under `reshade-shaders/Shaders` (E96).
    pub shader_dir: Option<String>,
    /// Extra dest component under `reshade-shaders/Textures` (E96).
    pub texture_dir: Option<String>,
    /// `EffectFiles` names the recipe was minted from (E96), display-only:
    /// the `Effects` preview. Empty = unknown (hand-written recipe, or minted
    /// before the key existed).
    pub effect_files: Box<[String]>,
    pub official: bool,
    /// Registry slug when this recipe lives under `mods/<registry>/`.
    /// `None` for official and user.
    pub registry: Option<String>,
    /// Missing `mods.toml` or missing id ⇒ enabled.
    pub enabled: bool,
}
impl Mod {
    /// Whether an `--adapter` value is permitted by this recipe's plans.
    pub fn allows_adapter(&self, adapter: &str) -> bool {
        self.plans_allowed.iter().any(|p| p.as_str() == adapter)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecipeFile {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) mod_type: String,
    pub(crate) label: String,
    #[serde(default = "default_plans")]
    pub(crate) plans_allowed: Box<[String]>,
    #[serde(default)]
    pub(crate) sha256: Option<String>,
    #[serde(default)]
    pub(crate) payload: Box<[PayloadRule]>,
    #[serde(default)]
    pub(crate) games: Box<[String]>,
    #[serde(default)]
    pub(crate) appids: Box<[u32]>,
    #[serde(default)]
    pub(crate) requires: Box<[String]>,
    #[serde(default)]
    pub(crate) slot: Option<String>,
    #[serde(default)]
    pub(crate) include: Box<[String]>,
    #[serde(default)]
    pub(crate) dests: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) shader_dir: Option<String>,
    #[serde(default)]
    pub(crate) texture_dir: Option<String>,
    #[serde(default)]
    pub(crate) effect_files: Box<[String]>,
    pub(crate) source: RecipeSource,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadRule {
    /// Gate: game bitness must equal (`32` | `64`). Absent = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// Gate: game api must equal (`dx9` | `dx10` | `dx11` | `dx12` | `vulkan` …). Absent = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Globs kept when this rule matches. Empty = keep all.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    pub keep: Box<[String]>,
    /// Globs dropped when this rule matches.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    pub drop: Box<[String]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecipeSource {
    #[serde(rename = "type")]
    pub(crate) source_type: String,
    #[serde(default)]
    pub(crate) owner: Option<String>,
    #[serde(default)]
    pub(crate) repo: Option<String>,
    #[serde(default)]
    pub(crate) asset_glob: Option<String>,
    #[serde(default)]
    pub(crate) tag: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    /// E63: follow the newest non-draft prerelease release. GitHub only;
    /// rejected together with a pinned tag or sha256 at parse.
    #[serde(default)]
    pub(crate) prerelease: bool,
}

pub(crate) fn default_plans() -> Box<[String]> {
    vec!["install".into(), "preload".into()].into_boxed_slice()
}

pub(crate) fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && id.len() <= 32
}
/// Infer a proxy slot from remapped sibling LoadDLL dest basenames (E86 §6):
/// exactly one distinct parsed slot persists; zero or conflicting → None.
/// `include` dests are IncludeFile, never LoadDLL, so they never vote;
/// subdir companions and `pfx:` dests never vote either — only a top-level
/// dest sits beside the game exe.
pub(crate) fn infer_slot<'a>(
    dests: impl IntoIterator<Item = &'a String>,
    include: &[String],
) -> Option<String> {
    let mut slots = BTreeSet::new();
    for d in dests {
        if crate::download::include_covers(include, d) {
            continue;
        }
        if d.contains('/') || d.contains('\\') || crate::install::is_prefix_dest(d) {
            continue;
        }
        if !crate::prewire::is_dll(d) {
            continue;
        }
        if let Ok(slot) = crate::modtype::parse_slot(d) {
            slots.insert(slot.as_str().to_string());
        }
    }
    if slots.len() == 1 {
        slots.into_iter().next()
    } else {
        None
    }
}
