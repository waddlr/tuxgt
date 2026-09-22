use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    atomic_write, parse_mod_type, sha256_file, sha256_hex, Error, ModType, Result,
    StagingRewriteContext,
};

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
    /// Forward-compat: unknown fields survive rewrite round-trips (R21).
    #[serde(flatten, default)]
    extra: BTreeMap<String, toml::Value>,
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
/// user-touched and is not overwritten unless `force` is set. While
/// copying, the type-owned R21 rewrite may transform bytes: `depot_sha`
/// records the caller-verified source hash, `staged_sha` the staged bytes,
/// `tuxgt_modified` whether they differ. Unknown `mod_type` fails closed
/// before any mutation; other failures leave prior staged bytes and
/// bookkeeping for the failing file intact, and the TOML is persisted
/// only when it changed.
pub fn sync_staging(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
    mod_type: &str,
    files: &[StageInput<'_>],
    force: bool,
) -> Result<()> {
    let rewriter = parse_mod_type(mod_type)
        .map_err(|_| Error::InvalidModType(format!("{game_id} {instance_id}: {mod_type}")))?;
    let ctx = StagingRewriteContext {
        game_id: game_id.to_string(),
        instance_id: instance_id.to_string(),
        runtime_dir: runtime_dir(data_dir, game_id),
    };
    let dir = stage_dir(data_dir, game_id, instance_id);
    let toml_path = staging_toml_path(data_dir, game_id, instance_id);
    let mut toml = read_staging_toml(&toml_path)?;
    let mut dirty = false;
    if toml.game.is_empty() {
        toml.game = game_id.to_string();
        toml.instance = instance_id.to_string();
        dirty = true;
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
                        let staged = copy_with_rewrite(rewriter, &ctx, f.rel, f.src, f.sha, &dest)?;
                        toml.files.insert(f.rel.to_string(), staged);
                        write_staging_toml(&toml_path, &toml)?;
                        dirty = false;
                    }
                } else if force {
                    let staged = copy_with_rewrite(rewriter, &ctx, f.rel, f.src, f.sha, &dest)?;
                    toml.files.insert(f.rel.to_string(), staged);
                    write_staging_toml(&toml_path, &toml)?;
                    dirty = false;
                } else {
                    touched.push(f.rel.to_string());
                }
            }
            None => {
                let staged = copy_with_rewrite(rewriter, &ctx, f.rel, f.src, f.sha, &dest)?;
                toml.files.insert(f.rel.to_string(), staged);
                write_staging_toml(&toml_path, &toml)?;
                dirty = false;
            }
        }
    }
    // Drop entries for files no longer planned; remove their staged copies.
    let planned: BTreeSet<&str> = files.iter().map(|f| f.rel).collect();
    let before = toml.files.len();
    toml.files.retain(|rel, _| {
        let keep = planned.contains(rel.as_str());
        if !keep {
            let _ = fs::remove_file(dir.join(rel));
        }
        keep
    });
    if toml.files.len() != before {
        dirty = true;
    }
    if dirty {
        write_staging_toml(&toml_path, &toml)?;
    }
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

fn write_staging_toml(path: &Path, toml: &StagingToml) -> Result<()> {
    let text = toml::to_string(toml).map_err(|e| Error::Manifest(e.to_string()))?;
    atomic_write(path, text.as_bytes())
}

/// Recorded `staged_sha` per rel for one instance (R21): the install
/// pre-touch tolerance compares staged bytes against the recorded output
/// hash, not the depot hash, so a TuxGT rewrite is never mistaken for a
/// user touch on the next sync.
pub fn staged_shas(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
) -> Result<BTreeMap<String, String>> {
    let toml = read_staging_toml(&staging_toml_path(data_dir, game_id, instance_id))?;
    Ok(toml
        .files
        .into_iter()
        .map(|(rel, e)| (rel, e.staged_sha))
        .collect())
}

/// Copy one depot source to its staged dest, applying the type-owned R21
/// rewrite when the type claims `rel`. A rewritten copy records the hash
/// of the source bytes it actually read as `depot_sha` (spec: depot hash
/// is always the source bytes, so a drifted depot reports `depot-newer`
/// instead of looking in sync) and the output hash as `staged_sha` with
/// `tuxgt_modified` set. The unclaimed fast path keeps v1 semantics and
/// records the caller's verified hash verbatim. The depot source is only
/// read, never written; the staged write is atomic, so a rewrite or I/O
/// failure leaves prior staged bytes intact.
fn copy_with_rewrite(
    rewriter: &dyn ModType,
    ctx: &StagingRewriteContext,
    rel: &str,
    src: &Path,
    depot_sha: &str,
    dest: &Path,
) -> Result<StagingFile> {
    if rewriter.staging_rewrite_applies(rel) {
        let input = fs::read(src)?;
        if let Some(output) = rewriter.staging_rewrite(ctx, rel, &input)? {
            let staged = StagingFile {
                depot_sha: sha256_hex(&input),
                staged_sha: sha256_hex(&output),
                tuxgt_modified: output != input,
            };
            atomic_write(dest, &output)?;
            return Ok(staged);
        }
    }
    copy_staged(src, dest)?;
    Ok(StagingFile {
        depot_sha: depot_sha.to_string(),
        staged_sha: depot_sha.to_string(),
        tuxgt_modified: false,
    })
}

/// Keep or omit one planned dest in staging (E64 per-dest keep). Omit drops
/// the staged copy and its bookkeeping; the bytes stay in the depot and a
/// later keep copies them back. A user-touched staged copy is refused on
/// omit (R33). Keep re-renders through the R21 type rewrite when the copy
/// is missing, touched, or stale under a moved depot.
pub fn set_staged(
    data_dir: &Path,
    game_id: &str,
    instance_id: &str,
    mod_type: &str,
    rel: &str,
    src: Option<&Path>,
    sha: &str,
    keep: bool,
) -> Result<()> {
    check_rel(rel)?;
    let rewriter = parse_mod_type(mod_type)
        .map_err(|_| Error::InvalidModType(format!("{game_id} {instance_id}: {mod_type}")))?;
    let ctx = StagingRewriteContext {
        game_id: game_id.to_string(),
        instance_id: instance_id.to_string(),
        runtime_dir: runtime_dir(data_dir, game_id),
    };
    let dir = stage_dir(data_dir, game_id, instance_id);
    let toml_path = staging_toml_path(data_dir, game_id, instance_id);
    let mut toml = read_staging_toml(&toml_path)?;
    let mut dirty = false;
    if toml.game.is_empty() {
        toml.game = game_id.to_string();
        toml.instance = instance_id.to_string();
        dirty = true;
    }
    let dest = dir.join(rel);
    let in_sync = if dest.is_file() {
        let hash = sha256_file(&dest)?;
        toml.files.get(rel).is_some_and(|e| e.staged_sha == hash)
    } else {
        false
    };
    if keep {
        // Re-render when the staged copy is missing or touched (v1
        // behavior) or when the depot moved under an in-sync copy: the
        // old record-the-new-hash-without-copying path left stale bytes
        // misreported as user-modified on the next status.
        let stale = toml.files.get(rel).is_some_and(|e| e.depot_sha != sha);
        if !in_sync || stale {
            let src = src.ok_or_else(|| {
                Error::Manifest(format!(
                    "{game_id} {instance_id}: no depot source for {rel}"
                ))
            })?;
            let staged = copy_with_rewrite(rewriter, &ctx, rel, src, sha, &dest)?;
            toml.files.insert(rel.to_string(), staged);
            dirty = true;
        }
    } else {
        if dest.is_file() {
            if !in_sync {
                return Err(Error::StagedModified(format!(
                    "{game_id} {instance_id}: {rel}"
                )));
            }
            fs::remove_file(&dest)?;
            dirty = true;
        }
        if toml.files.remove(rel).is_some() {
            dirty = true;
        }
    }
    if dirty {
        write_staging_toml(&toml_path, &toml)?;
    }
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
