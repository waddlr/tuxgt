use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::claims::DestClaim;
use super::copies::Removal;
use super::{backup_rel, resolve_target};
use crate::stage::game_safe;
use crate::{sha256_file, Error, FileManifest, PlannedFile, Result};

/// Game-dir materialization owned by one planned operation. Copy writes
/// staged bytes; Links writes an absolute symlink to the staged file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMaterial {
    Copy,
    Links,
}

impl InstallMaterial {
    pub(crate) fn of(adapter: &str) -> Self {
        if crate::is_links(adapter) {
            Self::Links
        } else {
            Self::Copy
        }
    }

    pub(crate) fn is_links(self) -> bool {
        self == Self::Links
    }
}

/// What currently occupies a game-dir dest path, resolved against every
/// enabled claim. `Owned` names all matching claims because conflicting
/// payloads may have byte-identical content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DestStatus {
    Absent,
    Owned { instances: Vec<String>, link: bool },
    ForeignFile,
    ForeignLink,
    Other,
}

impl DestStatus {
    pub(crate) fn is_owned(&self) -> bool {
        matches!(self, Self::Owned { .. })
    }
}

/// Classify one dest without following a symlink decision: `symlink_metadata`
/// sees the link itself, and `read_link` identifies our staged target.
/// A foreign symlink never counts as owned even when its target bytes match.
pub(crate) fn classify_dest(target: &Path, claims: &[DestClaim]) -> Result<DestStatus> {
    let meta = match fs::symlink_metadata(target) {
        Ok(meta) => meta,
        // NotFound or an unresolvable parent (a file where a directory
        // belongs): the dest cannot exist, so it is absent, not foreign.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(DestStatus::Absent);
        }
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        let actual = fs::read_link(target)?;
        let owners = claims
            .iter()
            .filter(|claim| {
                InstallMaterial::of(&claim.adapter).is_links() && claim.staged == actual
            })
            .map(|claim| claim.instance.clone())
            .collect::<Vec<_>>();
        return Ok(if owners.is_empty() {
            DestStatus::ForeignLink
        } else {
            DestStatus::Owned {
                instances: owners,
                link: true,
            }
        });
    }
    if meta.file_type().is_file() {
        let hash = sha256_file(target)?;
        // A Links claim materializes as a symlink, never as bytes: only a
        // Copy claim can own a regular file, even when the hashes coincide.
        let owners = claims
            .iter()
            .filter(|claim| !InstallMaterial::of(&claim.adapter).is_links() && claim.sha256 == hash)
            .map(|claim| claim.instance.clone())
            .collect::<Vec<_>>();
        return Ok(if owners.is_empty() {
            DestStatus::ForeignFile
        } else {
            DestStatus::Owned {
                instances: owners,
                link: false,
            }
        });
    }
    Ok(DestStatus::Other)
}

/// Prove a Links root can hold a symlink before any dest mutates. The probe
/// file is uniquely named, immediately removed, and never mistaken for a
/// payload.
pub(crate) fn ensure_symlink_supported(root: &Path, what: &str) -> Result<()> {
    let probe = root.join(format!(
        ".tuxgt-link-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if let Err(e) = std::os::unix::fs::symlink(root, &probe) {
        if e.kind() == std::io::ErrorKind::NotFound {
            return Err(Error::Install(format!(
                "{what} does not exist ({}): {e}",
                root.display()
            )));
        }
        return Err(Error::Install(format!(
            "{what} does not support symlinks ({}): {e}",
            root.display()
        )));
    }
    fs::remove_file(&probe)?;
    Ok(())
}

/// Replace any file or symlink at `target` with a symlink to `src`. A
/// directory is an error and nothing is removed, preserving the current
/// copy-path failure for a wrong-kind destination.
pub(crate) fn write_link(target: &Path, src: &Path) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() || meta.file_type().is_file() => {
            fs::remove_file(target)?;
        }
        Ok(_) => {
            return Err(Error::NotAFile(target.display().to_string()));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink(src, target)?;
    Ok(())
}

/// Replace a symlink at `target` before a byte copy. Copying onto a symlink
/// would write through it into staging (or another claimant's staged file),
/// so the link entry itself must be unlinked first.
pub(crate) fn unlink_for_copy(target: &Path) -> Result<()> {
    if fs::symlink_metadata(target).is_ok_and(|meta| meta.file_type().is_symlink()) {
        fs::remove_file(target)?;
    }
    Ok(())
}

/// Read bytes through a foreign symlink for a backup. A dangling foreign
/// link has no bytes to preserve; the link entry is removed by the caller.
pub(crate) fn foreign_link_bytes(target: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(target) {
        Ok(meta) if meta.file_type().is_symlink() => match fs::read(target) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        },
        _ => Ok(None),
    }
}

/// Absolute staged source a manifest expects at `dest`.
pub(crate) fn staged_source(data_dir: &Path, game_id: &str, instance: &str, dest: &str) -> PathBuf {
    crate::stage::stage_dir(data_dir, game_id, instance).join(dest)
}

fn store_backup(
    data_dir: &Path,
    game_id: &str,
    manifest: &mut FileManifest,
    bdir: &Path,
    dest: &str,
    rel: String,
) {
    let stored = bdir
        .join(&rel)
        .strip_prefix(data_dir)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| format!("backups/{}/{rel}", game_safe(game_id)));
    manifest.backups.insert(dest.to_string(), stored);
}

/// Back up the live target file before overwriting it. First backup wins:
/// callers persist the manifest only when `backups` gained a dest.
pub(crate) fn backup_target(
    data_dir: &Path,
    game_id: &str,
    manifest: &mut FileManifest,
    bdir: &Path,
    ts: u64,
    dest: &str,
    target: &Path,
    hash: &str,
) -> Result<()> {
    fs::create_dir_all(bdir)?;
    let rel = backup_rel(dest, ts, &hash[..8.min(hash.len())]);
    fs::copy(target, bdir.join(&rel))?;
    store_backup(data_dir, game_id, manifest, bdir, dest, rel);
    Ok(())
}

/// Back up foreign content read before the dest entry is replaced. Same
/// first-backup-wins rule as [`backup_target`].
pub(crate) fn backup_target_bytes(
    data_dir: &Path,
    game_id: &str,
    manifest: &mut FileManifest,
    bdir: &Path,
    ts: u64,
    dest: &str,
    bytes: &[u8],
    hash: &str,
) -> Result<()> {
    fs::create_dir_all(bdir)?;
    let rel = backup_rel(dest, ts, &hash[..8.min(hash.len())]);
    fs::write(bdir.join(&rel), bytes)?;
    store_backup(data_dir, game_id, manifest, bdir, dest, rel);
    Ok(())
}

/// Remove one owner-owned dest. Returns `true` when the destination was
/// owned by `owner` and removed or restored. A non-owned destination and its
/// backup record are preserved for the surviving claim.
pub(crate) fn remove_owned_dest(
    data_dir: &Path,
    game_id: &str,
    owner_instance: &str,
    owner_adapter: &str,
    owner_files: &[PlannedFile],
    dest: &str,
    backups: &mut BTreeMap<String, String>,
    root: &Path,
    prefix: Option<&Path>,
    out: &mut Removal,
) -> Result<bool> {
    let file = owner_files
        .iter()
        .find(|file| file.dest == dest)
        .ok_or_else(|| Error::Manifest(format!("{game_id}: no owned claim: {dest}")))?;
    let target = resolve_target(root, prefix, dest)?;
    let expected = DestClaim {
        instance: owner_instance.to_string(),
        adapter: owner_adapter.to_string(),
        sha256: file.sha256.clone(),
        staged: staged_source(data_dir, game_id, owner_instance, dest),
    };
    let owned = matches!(
        classify_dest(&target, std::slice::from_ref(&expected))?,
        DestStatus::Owned { .. }
    );
    if !owned {
        return Ok(false);
    }
    let recorded = backups.remove(dest);
    if let Some(rel) = recorded {
        let backup = data_dir.join(rel);
        if backup.is_file() {
            // Never copy onto a symlink: that would write through our own
            // link into staging. Replace the link entry itself.
            if fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()) {
                fs::remove_file(&target)?;
            }
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
    Ok(true)
}
