use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::*;
use crate::{atomic_write, Error, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedFile {
    pub source: String,
    pub dest: String,
    pub sha256: String,
    /// Per-dest keep for this game only (E64). Missing = true.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedEnv {
    pub key: String,
    pub value: String,
    /// Per-game keep for this game only (E74). Missing = true.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// True when `dest` is forced to IncludeFile by `include` (E88, E91):
/// an exact entry match, or a trailing-`/` entry covering the dest dir
/// (`shaders/` covers `shaders/foo.fx` and deeper paths).
pub fn include_covers(include: &[String], dest: &str) -> bool {
    include
        .iter()
        .any(|i| i == dest || (i.ends_with('/') && dest.starts_with(i.as_str())))
}

/// Required dests cannot be omitted (E64, `download.md`). `include` dests
/// are forced IncludeFile (E88): omit-able, except the lone dest of a
/// one-file pack, which stays required. Trailing-`/` include entries cover
/// whole dest dirs (E91).
pub fn is_required_dest(
    mod_type: &str,
    dest: &str,
    include: &[String],
    total_files: usize,
) -> bool {
    if include_covers(include, dest) {
        return total_files <= 1;
    }
    match mod_type {
        "reshade" => {
            let base = dest.rsplit(['/', '\\']).next().unwrap_or(dest);
            base.eq_ignore_ascii_case("ReShade64.dll") || base.eq_ignore_ascii_case("ReShade32.dll")
        }
        // E91: the claiming dest follows the current slot basename, so any
        // top-level slot-named dest is required (default `dxgi.dll`, or
        // `winmm.dll` after `instance slot`). Subdir companions never claim
        // the proxy even when their basename parses as a slot.
        "optiscaler" => {
            !dest.contains('/') && !dest.contains('\\') && crate::modtype::parse_slot(dest).is_ok()
        }
        "reshade_addon" => {
            let lower = dest.to_ascii_lowercase();
            lower.ends_with(".addon") || lower.ends_with(".addon64")
        }
        "custom" => crate::prewire::is_dll(dest),
        _ => false,
    }
}

/// Toggle one dest's keep bit by exact stored `dest` string.
/// Required dests cannot be disabled; unknown dests error.
pub fn set_file_enabled(
    data_dir: &Path,
    game: &str,
    instance: &str,
    dest: &str,
    enabled: bool,
) -> Result<FileManifest> {
    let mut m = read_manifest(data_dir, game, instance)?
        .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
    let total = m.files.len();
    let Some(f) = m.files.iter_mut().find(|f| f.dest == dest) else {
        return Err(Error::InvalidInstance(format!("unknown dest: {dest}")));
    };
    if !enabled && is_required_dest(&m.mod_type, &f.dest, &m.include, total) {
        return Err(Error::InvalidInstance(format!(
            "required dest cannot be omitted: {dest}"
        )));
    }
    f.enabled = enabled;
    write_manifest(data_dir, &m)?;
    Ok(m)
}

/// Toggle one manifest env row's keep bit by exact stored key.
/// Unknown keys error. Env-only: no staging, no prewire; callers that
/// affect launch env sync the session themselves (knob/custom pattern).
pub fn set_mod_env_enabled(
    data_dir: &Path,
    game: &str,
    instance: &str,
    key: &str,
    enabled: bool,
) -> Result<FileManifest> {
    let mut m = read_manifest(data_dir, game, instance)?
        .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
    let Some(e) = m.env.iter_mut().find(|e| e.key == key) else {
        return Err(Error::InvalidInstance(format!("unknown env key: {key}")));
    };
    e.enabled = enabled;
    write_manifest(data_dir, &m)?;
    Ok(m)
}

/// Install provenance (R32): what payload bytes a manifest was built from.
/// Recorded by every install path; the update check compares it against the
/// current source or cache. Empty on pre-R32 manifests.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModProvenance {
    /// Resolved download URL, or `local:<abs path>` for disk sources.
    #[serde(default)]
    pub source: String,
    /// Hex sha256 of the acquired asset (payload file or source tree).
    #[serde(default)]
    pub asset_sha256: String,
    /// Asset bytes at acquire time.
    #[serde(default)]
    pub asset_bytes: u64,
    /// Unix time of the acquire.
    #[serde(default)]
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileManifest {
    pub game: String,
    pub instance: String,
    #[serde(rename = "type")]
    pub mod_type: String,
    pub adapter: String,
    pub enabled: bool,
    /// Per-game order, this game only; lower stages/loads first, later wins same-dest. Missing (old manifests) = 0.
    #[serde(default)]
    pub load_order: i64,
    #[serde(default)]
    pub files: Box<[PlannedFile]>,
    /// Recipe `[env]` snapshot with per-game keep bits (E74).
    #[serde(default)]
    pub env: Box<[PlannedEnv]>,
    #[serde(default)]
    pub backups: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub generated_globs: Box<[String]>,
    /// Recipe `include` snapshot: dests forced to IncludeFile (E88).
    /// Empty on older manifests.
    #[serde(default)]
    pub include: Box<[String]>,
    /// Harvested runtime-generated files: game-root rel path → sha256.
    /// Rebuilt by every harvest; uninstall leaves these files in place.
    #[serde(default)]
    pub harvested: std::collections::BTreeMap<String, String>,
    /// Install provenance (R32). Empty on pre-R32 manifests.
    #[serde(default)]
    pub provenance: ModProvenance,
}

pub fn read_manifest(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
) -> Result<Option<FileManifest>> {
    let path = manifest_path(data_dir, game_id, instance_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let m: FileManifest = toml::from_str(&text).map_err(|e| Error::Manifest(e.to_string()))?;
    Ok(Some(m))
}

pub fn write_manifest(data_dir: &Path, m: &FileManifest) -> Result<PathBuf> {
    let path = manifest_path(data_dir, &m.game, &m.instance);
    let text = toml::to_string(m).map_err(|e| Error::Manifest(e.to_string()))?;
    atomic_write(&path, text.as_bytes())?;
    // Owner mark: every manifest mutation funnels through here, so the next
    // shared open rebuilds the mod cache (install counts, provenance sha).
    crate::db::mark_cache_dirty();
    Ok(path)
}

pub fn set_manifest_enabled(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
    enabled: bool,
) -> Result<FileManifest> {
    let mut m = read_manifest(data_dir, game_id, instance_id)?
        .ok_or_else(|| Error::NoManifest(format!("{game_id} {instance_id}")))?;
    m.enabled = enabled;
    write_manifest(data_dir, &m)?;
    Ok(m)
}
/// Default generated globs per mod type (E18): files the runtime writes
/// next to the game that harvest picks up. Addon packages also watch
/// `<addon-stem>.log` for every staged `.addon64` dest.
pub fn generated_globs_for(mod_type: &str, dests: &[&str]) -> Vec<String> {
    let mut globs: Vec<String> = match mod_type {
        "reshade" => vec!["ReShade.ini".into(), "ReShade.log".into()],
        "optiscaler" => vec!["OptiScaler.ini".into(), "OptiScaler.log".into()],
        "reshade_addon" => vec!["ReShade.ini".into(), "ReShade.log".into()],
        _ => Vec::new(),
    };
    if mod_type == "reshade_addon" {
        for d in dests {
            if d.to_lowercase().ends_with(".addon64") {
                if let Some(stem) = Path::new(d).file_stem().and_then(|s| s.to_str()) {
                    let glob = format!("{stem}.log");
                    if !globs.contains(&glob) {
                        globs.push(glob);
                    }
                }
            }
        }
    }
    globs
}

/// `*` wildcard match on a file name (case-sensitive; Linux game dirs).
pub(crate) fn glob_match(pat: &str, name: &str) -> bool {
    let p = pat.as_bytes();
    let n = name.as_bytes();
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}
