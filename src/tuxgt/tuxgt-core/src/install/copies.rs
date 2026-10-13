use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;

use super::claims::ClaimInventory;
use super::material::{
    classify_dest, ensure_symlink_supported, foreign_link_bytes, unlink_for_copy, write_link,
    DestStatus, InstallMaterial,
};
use super::*;
use crate::stage::{check_rel, stage_dir};
use crate::{write_manifest, Error, FileManifest, PlannedFile, Result};

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

#[derive(Debug)]
pub struct CopyOp {
    pub dest: String,
    pub src: PathBuf,
    pub needs_confirm: bool,
    pub is_prefix: bool,
    pub material: InstallMaterial,
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

/// True when materializing `dest` would overwrite content that is neither
/// missing nor claimed by an enabled Install manifest, on a protected
/// foreign stem: the rule the install/enable/copy paths confirm. Pure read,
/// so a keep toggle can settle the confirm before it mutates anything.
/// Prefix dests never need confirm: forbidden proxy stems are install
/// errors (see [`validate_prefix_dests`]), every other prefix overwrite
/// just backs up.
pub fn dest_needs_confirm(
    dest: &str,
    root: &Path,
    _prefix: Option<&Path>,
    claims: &ClaimInventory,
) -> Result<bool> {
    if is_prefix_dest(dest) {
        check_prefix_dest(dest)?;
        return Ok(false);
    }
    check_rel(dest)?;
    let target = root.join(dest);
    Ok(match classify_dest(&target, claims.claims_for(dest))? {
        DestStatus::Owned { .. } | DestStatus::Absent | DestStatus::Other => false,
        DestStatus::ForeignFile | DestStatus::ForeignLink => foreign_dest(dest),
    })
}

/// Plan one instance's game-dir materialization. Copy writes staged bytes;
/// Links writes an absolute symlink to the staged file. Game-dir dests
/// behave as before; `pfx:` dests resolve under the Wine `drive_c`
/// (see [`prefix_drive_c`]) and never need confirm (forbidden stems error).
/// Re-install of identical bytes is still copied (cheap) but never needs
/// confirm and never takes a backup. Callers pass the dests to sync; the
/// install paths pass kept dests only.
pub fn plan_copies<'a>(
    data_dir: &Path,
    game_id: &str,
    instance: &str,
    files: impl Iterator<Item = &'a PlannedFile>,
    root: &Path,
    prefix: Option<&Path>,
    material: InstallMaterial,
    claims: &ClaimInventory,
) -> Result<Vec<CopyOp>> {
    let stage = stage_dir(data_dir, game_id, instance);
    let mut ops = Vec::new();
    let mut needs_game_probe = false;
    let mut needs_prefix_probe = false;
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
        let needs_confirm = dest_needs_confirm(&f.dest, root, prefix, claims)?;
        if material.is_links() {
            if is_prefix {
                needs_prefix_probe = true;
            } else {
                needs_game_probe = true;
            }
        }
        ops.push(CopyOp {
            dest: f.dest.clone(),
            src,
            needs_confirm,
            is_prefix,
            material,
        });
    }
    if needs_game_probe {
        ensure_symlink_supported(root, "game directory")?;
    }
    if needs_prefix_probe {
        let drive = prefix_drive_c(prefix.ok_or_else(|| {
            Error::Install("pfx: dest needs a Proton/Wine prefix (native game)".to_string())
        })?);
        ensure_symlink_supported(&drive, "prefix destination root")?;
    }
    Ok(ops)
}

/// Apply planned game-dir operations: back up pre-existing unclaimed content
/// (first backup wins), then copy staged bytes or link the staged file.
/// Updates `manifest.backups` on disk. `pfx:` ops land under the Wine
/// `drive_c` with identical backup semantics.
pub fn apply_copies(
    data_dir: &Path,
    game_id: &str,
    manifest: &mut FileManifest,
    root: &Path,
    prefix: Option<&Path>,
    ops: &[CopyOp],
    claims: &ClaimInventory,
) -> Result<()> {
    let bdir = backups_dir(data_dir, game_id);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for op in ops {
        let target = resolve_target(root, prefix, &op.dest)?;
        let status = classify_dest(&target, claims.claims_for(&op.dest))?;
        let owned = status.is_owned();
        if !owned && !manifest.backups.contains_key(&op.dest) {
            match status {
                DestStatus::ForeignFile => {
                    let hash = crate::sha256_file(&target)?;
                    backup_target(
                        data_dir, game_id, manifest, &bdir, ts, &op.dest, &target, &hash,
                    )?;
                }
                DestStatus::ForeignLink => {
                    if let Some(bytes) = foreign_link_bytes(&target)? {
                        let hash = crate::sha256_hex(&bytes);
                        backup_target_bytes(
                            data_dir, game_id, manifest, &bdir, ts, &op.dest, &bytes, &hash,
                        )?;
                    }
                }
                DestStatus::Absent | DestStatus::Owned { .. } | DestStatus::Other => {}
            }
        }
        if op.material.is_links() {
            if fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink())
                && fs::read_link(&target).is_ok_and(|actual| actual == op.src)
            {
                continue;
            }
            write_link(&target, &op.src)?;
        } else {
            unlink_for_copy(&target)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&op.src, &target)?;
        }
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

/// Remove `owner`'s dests from the game dir / prefix: restore a recorded
/// backup, or delete content that still belongs to `owner`. Content owned by
/// another claimant, foreign content, and missing dests are left alone, and
/// their backup records are preserved for the surviving claim.
pub fn remove_copies<'a>(
    data_dir: &Path,
    backups: &mut BTreeMap<String, String>,
    dests: impl Iterator<Item = &'a str>,
    game_id: &str,
    owner_instance: &str,
    owner_adapter: &str,
    owner_files: &[PlannedFile],
    root: &Path,
    prefix: Option<&Path>,
    claims: &ClaimInventory,
) -> Result<Removal> {
    let mut out = Removal {
        restored: Vec::new(),
        deleted: Vec::new(),
        left_foreign: Vec::new(),
    };
    for dest in dests {
        let target = resolve_target(root, prefix, dest)?;
        match classify_dest(&target, claims.claims_for(dest))? {
            DestStatus::ForeignFile | DestStatus::ForeignLink => {
                out.left_foreign.push(dest.to_string());
            }
            DestStatus::Owned { instances, .. }
                if instances.iter().any(|instance| instance == owner_instance) =>
            {
                remove_owned_dest(
                    data_dir,
                    game_id,
                    owner_instance,
                    owner_adapter,
                    owner_files,
                    dest,
                    backups,
                    root,
                    prefix,
                    &mut out,
                )?;
            }
            DestStatus::Owned { .. } | DestStatus::Absent | DestStatus::Other => {}
        }
    }
    Ok(out)
}

/// Dests whose installed content is neither missing nor claimed by an
/// enabled Install manifest: uninstall leaves these alone and the confirm
/// names them. `pfx:` dests resolve under the Wine `drive_c`
/// (see [`prefix_drive_c`]); a `pfx:` dest with no prefix is an install
/// error.
pub fn foreign_occupied<'a>(
    dests: impl Iterator<Item = &'a str>,
    root: &Path,
    prefix: Option<&Path>,
    claims: &ClaimInventory,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for dest in dests {
        let target = resolve_target(root, prefix, dest)?;
        if matches!(
            classify_dest(&target, claims.claims_for(dest))?,
            DestStatus::ForeignFile | DestStatus::ForeignLink
        ) {
            out.push(dest.to_string());
        }
    }
    Ok(out)
}

/// Plan one dest's game-dir materialization: the per-dest keep path (E64).
/// `pfx:` dests resolve under `prefix` (install error when missing).
pub fn plan_dest_copy(
    data_dir: &Path,
    game_id: &str,
    manifest: &FileManifest,
    dest: &str,
    root: &Path,
    prefix: Option<&Path>,
    claims: &ClaimInventory,
) -> Result<Vec<CopyOp>> {
    let material = InstallMaterial::of(&manifest.adapter);
    let files = manifest.files.iter().filter(|f| f.dest == dest);
    plan_copies(
        data_dir,
        game_id,
        &manifest.instance,
        files,
        root,
        prefix,
        material,
        claims,
    )
}

/// Remove one dest's game-dir materialization and persist the manifest's backup map.
pub fn remove_dest_copy(
    data_dir: &Path,
    game: &str,
    manifest: &mut FileManifest,
    dest: &str,
    root: &Path,
    prefix: Option<&Path>,
    claims: &ClaimInventory,
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
        game,
        &manifest.instance,
        &manifest.adapter,
        &manifest.files,
        root,
        prefix,
        claims,
    )?;
    write_manifest(data_dir, manifest)?;
    Ok(out)
}
