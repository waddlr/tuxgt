use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use super::*;
use crate::stage::{check_rel, game_safe, stage_dir};
use crate::{sha256_file, write_manifest, Error, FileManifest, PlannedFile, Result};

/// Game-dir root for the install adapter: `install_dir`, else the exe parent.
pub async fn game_root(pool: &SqlitePool, game_id: &str) -> Result<PathBuf> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT install_dir, exe_path, override_exe_path FROM games WHERE id = ?")
            .bind(game_id)
            .fetch_optional(pool)
            .await?;
    let (install, store_exe, ovr_exe) = row.ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    let exe = ovr_exe.or(store_exe).filter(|e| !e.is_empty());
    match (install, exe) {
        (Some(dir), _) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        (_, Some(exe)) => {
            let p = PathBuf::from(&exe);
            p.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .ok_or_else(|| Error::MissingExe(game_id.into()))
        }
        _ => Err(Error::MissingExe(game_id.into())),
    }
}

/// Dests currently tracked by any manifest of the game, with planned hashes.
pub fn tracked_dests(data_dir: &Path, game_id: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for m in crate::game_manifests(data_dir, game_id)? {
        for f in &m.files {
            out.insert(f.dest.clone(), f.sha256.clone());
        }
    }
    Ok(out)
}
#[derive(Debug)]
pub struct CopyOp {
    pub dest: String,
    pub src: PathBuf,
    pub needs_confirm: bool,
    pub is_prefix: bool,
}

/// Validate one `pfx:` dest's shape and proxy-stem rule: the prefix
/// check shared by [`dest_needs_confirm`] and [`plan_copies`].
fn check_prefix_dest(dest: &str) -> Result<()> {
    let _ = prefix_rel(dest)?;
    if foreign_dest(dest) {
        return Err(Error::InvalidInstance(format!(
            "forbidden prefix dest (proxy stem): {dest}"
        )));
    }
    Ok(())
}

/// True when copying `dest` would overwrite content that is neither missing
/// nor TuxGT-tracked, on a protected foreign stem: the rule the
/// install/enable/copy paths confirm. Pure read, so a keep toggle can
/// settle the confirm before it mutates anything. Prefix dests never need
/// confirm: forbidden proxy stems are install errors (see
/// [`validate_prefix_dests`]), every other prefix overwrite just backs up.
pub fn dest_needs_confirm(
    dest: &str,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<bool> {
    if is_prefix_dest(dest) {
        check_prefix_dest(dest)?;
        return Ok(false);
    }
    check_rel(dest)?;
    let target = root.join(dest);
    let _ = prefix;
    let existing = target.is_file().then(|| sha256_file(&target)).transpose()?;
    Ok(match existing {
        Some(hash) if Some(&hash) == tracked.get(dest) => false,
        Some(_) => foreign_dest(dest),
        None => false,
    })
}

/// Plan the install-adapter copy for one instance's planned files. Game-dir
/// dests behave as before; `pfx:` dests resolve under the Wine `drive_c`
/// (see [`prefix_drive_c`]) and never need confirm (forbidden stems error). Re-install of identical
/// bytes is still copied (cheap) but never needs confirm and never takes a
/// backup. Callers pass the dests to sync; the install paths pass kept
/// dests only.
pub fn plan_copies<'a>(
    data_dir: &Path,
    game_id: &str,
    instance: &str,
    files: impl Iterator<Item = &'a PlannedFile>,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<Vec<CopyOp>> {
    let stage = stage_dir(data_dir, game_id, instance);
    let mut ops = Vec::new();
    for f in files {
        let is_prefix = is_prefix_dest(&f.dest);
        if is_prefix {
            check_prefix_dest(&f.dest)?;
            if prefix.is_none() {
                return Err(Error::Install(format!(
                    "{}: pfx: dest needs a Proton/Wine prefix (native game)",
                    f.dest
                )));
            }
        } else {
            check_rel(&f.dest)?;
        }
        let src = stage.join(&f.dest);
        if !src.is_file() {
            return Err(Error::Manifest(format!(
                "{game_id} {instance}: not staged: {}",
                f.dest
            )));
        }
        let needs_confirm = dest_needs_confirm(&f.dest, root, prefix, tracked)?;
        ops.push(CopyOp {
            dest: f.dest.clone(),
            src,
            needs_confirm,
            is_prefix,
        });
    }
    Ok(ops)
}

/// Apply planned copies: back up pre-existing untracked content
/// (first backup wins), then copy. Updates `manifest.backups` on disk.
/// `pfx:` ops land under the Wine `drive_c` with identical backup semantics.
pub fn apply_copies(
    data_dir: &Path,
    game_id: &str,
    manifest: &mut FileManifest,
    root: &Path,
    prefix: Option<&Path>,
    ops: &[CopyOp],
    tracked: &BTreeMap<String, String>,
) -> Result<()> {
    let bdir = backups_dir(data_dir, game_id);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for op in ops {
        let target = resolve_target(root, prefix, &op.dest)?;
        if target.is_file() {
            let hash = sha256_file(&target)?;
            if Some(&hash) != tracked.get(&op.dest) && !manifest.backups.contains_key(&op.dest) {
                fs::create_dir_all(&bdir)?;
                let rel = backup_rel(&op.dest, ts, &hash[..8.min(hash.len())]);
                fs::copy(&target, bdir.join(&rel))?;
                let stored = bdir
                    .join(&rel)
                    .strip_prefix(data_dir)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| format!("backups/{}/{rel}", game_safe(game_id)));
                manifest.backups.insert(op.dest.clone(), stored);
            }
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&op.src, &target)?;
    }
    write_manifest(data_dir, manifest)?;
    Ok(())
}

#[derive(Debug)]
pub struct Removal {
    pub restored: Vec<String>,
    pub deleted: Vec<String>,
    pub left_foreign: Vec<String>,
}

/// Remove the given dests from the game dir / prefix: restore backups where
/// recorded, delete files that still match tracked content, and leave foreign
/// content alone. Drops those dests' backup entries from `backups`.
pub fn remove_copies<'a>(
    data_dir: &Path,
    backups: &mut BTreeMap<String, String>,
    dests: impl Iterator<Item = &'a str>,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<Removal> {
    let mut out = Removal {
        restored: Vec::new(),
        deleted: Vec::new(),
        left_foreign: Vec::new(),
    };
    for dest in dests {
        let target = resolve_target(root, prefix, dest)?;
        let recorded = backups.remove(dest);
        if !target.is_file() {
            continue;
        }
        let hash = sha256_file(&target)?;
        if Some(&hash) != tracked.get(dest) {
            out.left_foreign.push(dest.to_string());
            continue;
        }
        if let Some(rel) = recorded {
            let backup = data_dir.join(rel);
            if backup.is_file() {
                fs::copy(&backup, &target)?;
                let _ = fs::remove_file(&backup);
                out.restored.push(dest.to_string());
            } else {
                let _ = fs::remove_file(&target);
                out.deleted.push(dest.to_string());
            }
        } else {
            let _ = fs::remove_file(&target);
            out.deleted.push(dest.to_string());
        }
    }
    Ok(out)
}

/// Dests whose installed content is neither missing nor TuxGT-tracked:
/// uninstall leaves these alone and the confirm names them. `pfx:` dests
/// resolve under the Wine `drive_c` (see [`prefix_drive_c`]); a `pfx:` dest
/// with no prefix is an install error.
pub fn foreign_occupied<'a>(
    dests: impl Iterator<Item = &'a str>,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for dest in dests {
        let target = resolve_target(root, prefix, dest)?;
        if !target.is_file() {
            continue;
        }
        let hash = sha256_file(&target)?;
        if Some(&hash) != tracked.get(dest) {
            out.push(dest.to_string());
        }
    }
    Ok(out)
}

/// Plan one dest's install-adapter copy: the per-dest keep path (E64).
/// `pfx:` dests resolve under `prefix` (install error when missing).
pub fn plan_dest_copy(
    data_dir: &Path,
    game_id: &str,
    manifest: &FileManifest,
    dest: &str,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<Vec<CopyOp>> {
    let files = manifest.files.iter().filter(|f| f.dest == dest);
    plan_copies(
        data_dir,
        game_id,
        &manifest.instance,
        files,
        root,
        prefix,
        tracked,
    )
}

/// Remove one dest's install-adapter copy and persist the manifest's backup map.
pub fn remove_dest_copy(
    data_dir: &Path,
    manifest: &mut FileManifest,
    dest: &str,
    root: &Path,
    prefix: Option<&Path>,
    tracked: &BTreeMap<String, String>,
) -> Result<Removal> {
    let dests = manifest
        .files
        .iter()
        .filter(|f| f.dest == dest)
        .map(|f| f.dest.as_str());
    let out = remove_copies(
        data_dir,
        &mut manifest.backups,
        dests,
        root,
        prefix,
        tracked,
    )?;
    write_manifest(data_dir, manifest)?;
    Ok(out)
}
