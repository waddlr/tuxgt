use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::instance::Mod;
use crate::Result;

/// Kept payload files for a Files ▸ preview: recipe `keep` union
/// (game-independent) minus `drop` globs minus built-in junk. Sorted and
/// deduped, uncapped — callers truncate for display (`+ N more` / expand).
pub fn preview_payload_files(
    config_dir: &Path,
    data_dir: &Path,
    id: &str,
) -> Result<(Vec<String>, usize)> {
    let inst = find_mod(config_dir, data_dir, id)?;
    let dir =
        crate::instance::payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    let mut files = Vec::new();
    // Missing dir errors into an empty walk; GUI paints the needs-install hint.
    collect_payload_files(&dir, &mut files).unwrap_or(());
    let drops: Vec<String> = inst
        .payload
        .iter()
        .flat_map(|r| r.drop.iter().cloned())
        .collect();
    let mut names: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(&dir)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|s| !s.is_empty())
        .filter(|s| payload_keeps(&inst.payload, s))
        .filter(|s| !drops.iter().any(|g| crate::download::glob_match(g, s)))
        .filter(|s| !is_junk_dest(s))
        .collect();
    names.sort();
    names.dedup();
    let total = names.len();
    Ok((names, total))
}

/// Recipe `effect_files` with the recipe's own payload drops applied — the
/// same matching [`preview_payload_files`] does on disk, so a pack's denied
/// files stay hidden. Sorted and deduped, uncapped — callers truncate.
pub fn effect_names_for(inst: &Mod) -> Vec<String> {
    let drops: Vec<String> = inst
        .payload
        .iter()
        .flat_map(|r| r.drop.iter().cloned())
        .collect();
    let mut names: Vec<String> = inst
        .effect_files
        .iter()
        .filter(|f| !drops.iter().any(|g| crate::download::glob_match(g, f)))
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Whether the local payload holds any file the recipe keeps — the gate for
/// the `Files` preview button, so it never offers an empty list. Takes the
/// loaded recipe (row builders already hold it), so it never re-reads the
/// catalog. Early-exits on the first surviving file; missing dir counts as
/// empty.
pub fn payload_has_files(data_dir: &Path, inst: &Mod) -> bool {
    let dir =
        crate::instance::payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    let drops: Vec<String> = inst
        .payload
        .iter()
        .flat_map(|r| r.drop.iter().cloned())
        .collect();
    payload_dir_has_file(&dir, &dir, &inst.payload, &drops).unwrap_or(false)
}

/// Recursive early-exit walk for [`payload_has_files`]. Mirrors
/// [`collect_payload_files`] (sorted entries, `.provenance.toml` is not
/// payload, `/`-joined relative paths) and stops on the same read error, so
/// the gate and the painted list always agree.
pub(crate) fn payload_dir_has_file(
    root: &Path,
    dir: &Path,
    rules: &[crate::instance::PayloadRule],
    drops: &[String],
) -> Option<bool> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if payload_dir_has_file(root, &path, rules, drops)? {
                return Some(true);
            }
            continue;
        }
        if path.file_name().is_some_and(|n| n == ".provenance.toml") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.is_empty()
            && payload_keeps(rules, &rel)
            && !drops.iter().any(|g| crate::download::glob_match(g, &rel))
            && !is_junk_dest(&rel)
        {
            return Some(true);
        }
    }
    Some(false)
}

pub(crate) fn provenance_from_payload(payload: &Path, inst: &Mod) -> Result<crate::ModProvenance> {
    if let Some(p) = crate::instance::read_payload_provenance(payload) {
        if !p.asset_sha256.is_empty() {
            return Ok(p);
        }
    }
    let (sha, bytes) = crate::download::digest_path(payload)?;
    let source = match crate::instance::resolve_source(inst) {
        crate::instance::SourceRef::Local { path } => {
            crate::download::local_key_src(Path::new(&path))
                .unwrap_or_else(|_| format!("local:{}", payload.display()))
        }
        crate::instance::SourceRef::ManualUrl { url } => url,
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            if !asset_glob.contains(['*', '?', '[']) && !prerelease {
                crate::download::github_release_url(&owner, &repo, tag.as_deref(), &asset_glob)
            } else {
                format!("github:{owner}/{repo}/{asset_glob}")
            }
        }
    };
    let provenance = crate::ModProvenance {
        source,
        asset_sha256: sha,
        asset_bytes: bytes,
        fetched_at: crate::download::now_unix(),
    };
    crate::instance::write_payload_provenance(payload, &provenance);
    Ok(provenance)
}
