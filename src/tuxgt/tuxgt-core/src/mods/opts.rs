use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::instance::{list_mods, payload_dir, Mod, PayloadRule};
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct InstallOpts {
    pub adapter: String,
    pub redownload: bool,
    pub with_requires: Option<String>,
    pub yes: bool,
    pub force: bool,
}

impl Default for InstallOpts {
    fn default() -> Self {
        Self {
            adapter: "preload".into(),
            redownload: false,
            with_requires: None,
            yes: false,
            force: false,
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
/// match no gate. Base set = union of `keep` across matching rules, or
/// everything when no matching rule declares `keep`; then subtract union of
/// matching `drop`s. No rules at all keeps everything. Built-in repo junk
/// (`is_junk_dest`) always drops, rules or no rules.

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
    let any_keep = matching.iter().any(|r| !r.keep.is_empty());
    dests
        .iter()
        .map(|d| {
            if is_junk_dest(d) {
                return false;
            }
            let kept = !any_keep
                || matching
                    .iter()
                    .any(|r| r.keep.iter().any(|g| crate::download::glob_match(g, d)));
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
