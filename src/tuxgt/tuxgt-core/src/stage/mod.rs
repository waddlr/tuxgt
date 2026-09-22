use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{atomic_write, sha256_file, Error, Result};

mod status;

pub use status::{stage_status, StageLine, StageState};

/// Staging: depot (immutable `<data>/downloads`) → per-game staging
/// (`<game>/stage/<instance>/`) → runtime (the loader's `TUXGT_GAME_DIR`,
/// which is `<game>/runtime/`). The loader is injector-agnostic: staged
/// files load via explicit `LoadDLL` entries and `IncludeFile` tree mirrors.
pub fn game_safe(game_id: &str) -> String {
    game_id.replace([':', '/'], "_")
}

pub fn stage_root(data_dir: &Path) -> PathBuf {
    data_dir.join("stage")
}

fn game_stage_root(data_dir: &Path, game_id: &str) -> Result<PathBuf> {
    let id = crate::game::GameId::parse(game_id)?;
    Ok(crate::game::game_dir(data_dir, &id).join("stage"))
}

fn game_stage_dir(data_dir: &Path, game_id: &str) -> PathBuf {
    game_stage_root(data_dir, game_id)
        .unwrap_or_else(|_| stage_root(data_dir).join(game_safe(game_id)))
}

pub fn stage_dir(data_dir: &Path, game_id: &str, instance_id: &str) -> PathBuf {
    game_stage_dir(data_dir, game_id).join(instance_id)
}

pub fn staging_toml_path(data_dir: &Path, game_id: &str, instance_id: &str) -> PathBuf {
    game_stage_dir(data_dir, game_id).join(format!("{instance_id}.staging.toml"))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StagingFile {
    #[serde(default)]
    depot_sha: String,
    #[serde(default)]
    staged_sha: String,
    #[serde(default)]
    tuxgt_modified: bool,
}

/// Per-instance staged-file state. Change-amplifier pair with the
/// loader's status-ini manifest (`stage.c` `manifest_parse`/`prune_stale`):
/// a staged-file state change touches both.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StagingToml {
    #[serde(default)]
    game: String,
    #[serde(default)]
    instance: String,
    #[serde(default)]
    files: BTreeMap<String, StagingFile>,
}

/// One file to stage: `rel` is the manifest dest (also the staging rel path),
/// `src` the unpacked file, `sha` its verified hash.
pub struct StageInput<'a> {
    pub rel: &'a str,
    pub src: &'a Path,
    pub sha: &'a str,
}

/// Reject empty, absolute, backslash, and parent-dir escape: staged dests
/// must stay under their staging / game-dir root. Shared core with the
/// loader's `rel_path_ok` (`src/launcher/stage.c`); two deliberate deltas:
/// `:` stays allowed here (`pfx:` dests stage verbatim, the loader never
/// sees them) and `..` is rejected per path component, not substring
/// (`a..b.dll` is a legal Linux name; components suffice to stop escape).
pub fn check_rel(rel: &str) -> Result<()> {
    let p = Path::new(rel);
    if p.is_absolute() || rel.contains('\\') {
        return Err(Error::Manifest(format!("bad staged dest: {rel}")));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::Manifest(format!("bad staged dest: {rel}")));
    }
    if rel.is_empty() {
        return Err(Error::Manifest("empty staged dest".into()));
    }
    Ok(())
}

fn read_staging_toml(path: &Path) -> Result<StagingToml> {
    if !path.exists() {
        return Ok(StagingToml::default());
    }
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| Error::Manifest(e.to_string()))
}

/// Copy unpacked files into per-game staging, honoring the sync rule:
/// a staged file whose hash differs from the recorded `staged_sha` is
/// user-touched and is not overwritten unless `force` is set. v1 never
/// modifies staged content (`tuxgt_modified` stays false); type-specific
/// rewrites land per ModType later.
pub fn sync_staging(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
    files: &[StageInput<'_>],
    force: bool,
) -> Result<()> {
    let dir = stage_dir(data_dir, game_id, instance_id);
    let toml_path = staging_toml_path(data_dir, game_id, instance_id);
    let mut toml = read_staging_toml(&toml_path)?;
    if toml.game.is_empty() {
        toml.game = game_id.to_string();
        toml.instance = instance_id.to_string();
    }
    let mut touched = Vec::new();
    for f in files {
        check_rel(f.rel)?;
        let dest = dir.join(f.rel);
        let current = dest.is_file().then(|| sha256_file(&dest)).transpose()?;
        match current {
            Some(hash) => {
                let entry = toml.files.get(f.rel);
                let in_sync = entry.is_some_and(|e| e.staged_sha == hash);
                if in_sync {
                    if entry.is_some_and(|e| e.depot_sha != f.sha) {
                        copy_staged(f.src, &dest)?;
                        toml.files.insert(
                            f.rel.to_string(),
                            StagingFile {
                                depot_sha: f.sha.to_string(),
                                staged_sha: f.sha.to_string(),
                                tuxgt_modified: false,
                            },
                        );
                    }
                } else if force {
                    copy_staged(f.src, &dest)?;
                    toml.files.insert(
                        f.rel.to_string(),
                        StagingFile {
                            depot_sha: f.sha.to_string(),
                            staged_sha: f.sha.to_string(),
                            tuxgt_modified: false,
                        },
                    );
                } else {
                    touched.push(f.rel.to_string());
                }
            }
            None => {
                copy_staged(f.src, &dest)?;
                toml.files.insert(
                    f.rel.to_string(),
                    StagingFile {
                        depot_sha: f.sha.to_string(),
                        staged_sha: f.sha.to_string(),
                        tuxgt_modified: false,
                    },
                );
            }
        }
    }
    // Drop entries for files no longer planned; remove their staged copies.
    let planned: std::collections::BTreeSet<&str> = files.iter().map(|f| f.rel).collect();
    toml.files.retain(|rel, _| {
        let keep = planned.contains(rel.as_str());
        if !keep {
            let _ = fs::remove_file(dir.join(rel));
        }
        keep
    });
    let text = toml::to_string(&toml).map_err(|e| Error::Manifest(e.to_string()))?;
    atomic_write(&toml_path, text.as_bytes())?;
    if !touched.is_empty() {
        touched.sort();
        return Err(Error::StagedModified(format!(
            "{game_id} {instance_id}: {}",
            touched.join(", ")
        )));
    }
    Ok(())
}

fn copy_staged(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dest)?;
    Ok(())
}

/// Keep or omit one planned dest in staging (E64 per-dest keep). Omit drops
/// the staged copy and its bookkeeping; the bytes stay in the depot and a
/// later keep copies them back. A user-touched staged copy is refused (R33).
pub fn set_staged(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
    rel: &str,
    src: Option<&Path>,
    sha: &str,
    keep: bool,
) -> Result<()> {
    check_rel(rel)?;
    let dir = stage_dir(data_dir, game_id, instance_id);
    let toml_path = staging_toml_path(data_dir, game_id, instance_id);
    let mut toml = read_staging_toml(&toml_path)?;
    if toml.game.is_empty() {
        toml.game = game_id.to_string();
        toml.instance = instance_id.to_string();
    }
    let dest = dir.join(rel);
    let in_sync = if dest.is_file() {
        let hash = sha256_file(&dest)?;
        toml.files.get(rel).is_some_and(|e| e.staged_sha == hash)
    } else {
        false
    };
    if keep {
        if !in_sync {
            let src = src.ok_or_else(|| {
                Error::Manifest(format!(
                    "{game_id} {instance_id}: no depot source for {rel}"
                ))
            })?;
            copy_staged(src, &dest)?;
        }
        toml.files.insert(
            rel.to_string(),
            StagingFile {
                depot_sha: sha.to_string(),
                staged_sha: sha.to_string(),
                tuxgt_modified: false,
            },
        );
    } else {
        if dest.is_file() {
            if !in_sync {
                return Err(Error::StagedModified(format!(
                    "{game_id} {instance_id}: {rel}"
                )));
            }
            fs::remove_file(&dest)?;
        }
        toml.files.remove(rel);
    }
    let text = toml::to_string(&toml).map_err(|e| Error::Manifest(e.to_string()))?;
    atomic_write(&toml_path, text.as_bytes())?;
    Ok(())
}

/// Per-game loader dest: `<game>/runtime/`.
pub fn runtime_dir(data_dir: &Path, game_id: &str) -> PathBuf {
    match crate::game::GameId::parse(game_id) {
        Ok(id) => crate::game::game_dir(data_dir, &id).join("runtime"),
        Err(_) => data_dir
            .join("games")
            .join(game_safe(game_id))
            .join("runtime"),
    }
}

/// Drop this instance's dests from `<game>/runtime/`. Dests still claimed
/// by another instance are left. Generated files (not dests) stay.
/// Empty parent dirs under runtime are rmdir'd.
pub fn remove_runtime_dests(
    data_dir: &Path,
    game_id: &str,
    dests: impl IntoIterator<Item = impl AsRef<str>>,
    keep: &BTreeSet<String>,
) -> Result<()> {
    let root = runtime_dir(data_dir, game_id);
    if !root.exists() {
        return Ok(());
    }
    let mut parents: Vec<PathBuf> = Vec::new();
    for d in dests {
        let dest = d.as_ref();
        if crate::install::is_prefix_dest(dest) || keep.contains(dest) {
            continue;
        }
        check_rel(dest)?;
        let path = root.join(dest);
        if path.is_file() {
            fs::remove_file(&path)?;
            if let Some(p) = path.parent() {
                if p.starts_with(&root) && p != root {
                    parents.push(p.to_path_buf());
                }
            }
        }
    }
    parents.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
    parents.dedup();
    for p in parents {
        let mut cur = p;
        while cur.starts_with(&root) && cur != root {
            if fs::remove_dir(&cur).is_err() {
                break;
            }
            match cur.parent() {
                Some(n) => cur = n.to_path_buf(),
                None => break,
            }
        }
    }
    Ok(())
}

/// Remove a game's staging tree (files + toml) for one instance.
pub fn remove_staging(data_dir: &Path, game_id: &str, instance_id: &str) -> Result<()> {
    let dir = stage_dir(data_dir, game_id, instance_id);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    let toml = staging_toml_path(data_dir, game_id, instance_id);
    if toml.exists() {
        fs::remove_file(&toml)?;
    }
    Ok(())
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
