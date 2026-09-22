use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::instance::{list_mods, payload_dir, Mod, PayloadRule};
use crate::{Error, Result};

/// One install request. `adapter` is the payload delivery mechanism; R37
/// resolves it from the game's persisted choice before it reaches here,
/// and `FileManifest.adapter` stays the per-instance truth once written.
#[derive(Clone, Debug)]
pub struct InstallOpts {
    /// `None` = use the game's persisted choice (`games.adapter`), the
    /// only default a caller can get wrong by accident. `Some` is an
    /// explicit per-call override, validated by `install_instance` against
    /// the same `preload`/`install` vocabulary the setter uses.
    pub adapter: Option<String>,
    pub redownload: bool,
    pub with_requires: Option<String>,
    pub yes: bool,
    pub force: bool,
    /// Password for an encrypted downloaded archive. `None` still reports
    /// [`crate::Error::ArchivePasswordRequired`] instead of prompting.
    pub password: Option<String>,
    /// `<self>` or a proxy stem for a slot-configurable install. Consumed
    /// by the named instance only; nested requires installs pass `None`.
    pub slot: Option<String>,
}

impl Default for InstallOpts {
    fn default() -> Self {
        Self {
            // R37: no override, so the game's persisted choice decides.
            // `preload` stays the effective default for a game that never
            // picked one, which is every pre-R37 row.
            adapter: None,
            redownload: false,
            with_requires: None,
            yes: false,
            force: false,
            password: None,
            slot: None,
        }
    }
}

pub fn find_mod(config_dir: &Path, data_dir: &Path, id: &str) -> Result<Mod> {
    list_mods(config_dir, data_dir)?
        .mods
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| Error::UnknownInstance(id.into()))
}

/// Effective arch/api for payload gates: override, then detected.
pub(crate) async fn game_arch_api(pool: &SqlitePool, game: &str) -> Result<(String, String)> {
    let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT override_bitness, detected_bitness, override_api, detected_api FROM games WHERE id = ?",
    )
    .bind(game)
    .fetch_optional(pool)
    .await?;
    let (ob, db, oa, da) = row.unwrap_or_default();
    Ok((ob.or(db).unwrap_or_default(), oa.or(da).unwrap_or_default()))
}

/// Game-independent keep check for payload previews: union of `keep`
/// across ALL rules (arch/api gates ignored — the preview names a mod,
/// not a (game, instance) pair, so a file kept for any gate shows).
/// No rule declaring `keep` keeps everything. Drops and junk are NOT
/// applied here; callers layer them as before.
pub(crate) fn payload_keeps(rules: &[PayloadRule], dest: &str) -> bool {
    if !rules.iter().any(|r| !r.keep.is_empty()) {
        return true;
    }
    rules
        .iter()
        .any(|r| r.keep.iter().any(|g| crate::download::glob_match(g, dest)))
}

/// Recipe payload filter: which extracted dests enter the manifest. A rule
/// matches when every gate it declares equals the game; unknown game values
/// match no gate. Matching keeps win; if some rule declares keep but none of
/// the matching rules do, use the union of every keep glob (gates ignored);
/// if no rule declares keep, keep everything. Then subtract matching drops.
/// Built-in repo junk (`is_junk_dest`) always drops.

pub(crate) fn filter_payload(
    rules: &[PayloadRule],
    dests: &[&str],
    arch: &str,
    api: &str,
) -> Vec<bool> {
    let matching: Vec<&PayloadRule> = rules
        .iter()
        .filter(|r| {
            r.arch.as_deref().map_or(true, |a| a == arch)
                && r.api.as_deref().map_or(true, |a| a == api)
        })
        .collect();
    let keep_from_matching = matching.iter().any(|r| !r.keep.is_empty());
    let keep_globs: Vec<&String> = if keep_from_matching {
        matching.iter().flat_map(|r| r.keep.iter()).collect()
    } else {
        rules.iter().flat_map(|r| r.keep.iter()).collect()
    };
    let any_keep = !keep_globs.is_empty();
    dests
        .iter()
        .map(|d| {
            if is_junk_dest(d) {
                return false;
            }
            let kept = !any_keep || keep_globs.iter().any(|g| crate::download::glob_match(g, d));
            let dropped = matching
                .iter()
                .any(|r| r.drop.iter().any(|g| crate::download::glob_match(g, d)));
            kept && !dropped
        })
        .collect()
}

/// Built-in repo-junk drops (effect-junk-dests): GitHub repo zips carry
/// files ReShade never reads (live AstrayFX mint: `.github/`, `_config.yml`,
/// `README.md`, empty `Textures/dummy`) that stage and consume ini budget.
/// Minted recipes predate the rule, so this runs at install regardless of
/// `keep`: existing installs shed junk on reinstall.
pub(crate) fn is_junk_dest(dest: &str) -> bool {
    if dest
        .split('/')
        .any(|seg| seg.len() > 1 && seg.starts_with('.'))
    {
        return true;
    }
    let base = dest.rsplit('/').next().unwrap_or(dest);
    if base == "_config.yml" || base == "dummy" {
        return true;
    }
    let lower = base.to_ascii_lowercase();
    lower == "readme"
        || lower.starts_with("readme.")
        || lower.starts_with("readme-")
        || lower.starts_with("readme_")
}

/// Config-text allowlist for per-file edit (`gui.mod-config-edit`): true
/// when the basename extension (after the last `.`, ASCII case-insensitive)
/// is one of `ini cfg conf toml json xml txt`, and the dest is neither a
/// DLL nor repo junk. No extension or a trailing dot → false.
pub fn is_config_text(rel: &str) -> bool {
    if crate::is_dll(rel) || is_junk_dest(rel) {
        return false;
    }
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let ext = match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext,
        _ => return false,
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "ini" | "cfg" | "conf" | "toml" | "json" | "xml" | "txt"
    )
}

/// Payload-side absolute path of one catalog file (`gui.mod-config-edit`
/// Settings level): resolves kind/official/registry via `find_mod`, rejects
/// escape/absolute rels via `check_rel`, and errors when the payload file
/// does not exist.
pub fn payload_config_path(
    config_dir: &Path,
    data_dir: &Path,
    id: &str,
    rel: &str,
) -> Result<PathBuf> {
    let m = find_mod(config_dir, data_dir, id)?;
    crate::check_rel(rel)?;
    let path = payload_dir(data_dir, m.official, m.registry.as_deref(), &m.id).join(rel);
    if !path.is_file() {
        return Err(Error::Manifest(format!("{id}: no payload file for {rel}")));
    }
    Ok(path)
}
