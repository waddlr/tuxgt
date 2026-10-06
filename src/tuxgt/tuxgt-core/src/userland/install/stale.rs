use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::userland::{
    is_ours, sha256_hex_bytes, HostInstallInventory, HostKind, IntendedHostEntry,
};

/// Remove previous-inventory paths that are no longer intended, when we
/// still own them: our symlink target, old-inventory file sha256, or the
/// hook marker. Returns `(removed, skipped)`. Missing paths are already
/// gone (neither). Paths under PREFIX are never deleted (`games/`,
/// `config/`, the tree itself) — they land in `skipped`. Only files and
/// symlinks are ever removed; directories are left alone.
pub(crate) fn remove_stale_owned(
    prefix: &Path,
    intended: &[IntendedHostEntry],
    previous: &HostInstallInventory,
) -> (Box<[PathBuf]>, Box<[PathBuf]>) {
    let intended_set: HashSet<&Path> = intended.iter().map(|e| e.path.as_path()).collect();
    let (mut removed, mut skipped): (Vec<PathBuf>, Vec<PathBuf>) = (Vec::new(), Vec::new());
    for old in &previous.paths {
        if intended_set.contains(old.path.as_path()) {
            continue;
        }
        if old.path.starts_with(prefix) {
            skipped.push(old.path.clone());
            continue;
        }
        let owned = match old.kind {
            HostKind::Symlink => match std::fs::read_link(&old.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
                Ok(target) => Some(target == old.target.clone().unwrap_or_default()),
            },
            HostKind::File => match std::fs::read(&old.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
                Ok(bytes) => {
                    if Some(sha256_hex_bytes(&bytes)) == old.sha256 {
                        Some(true)
                    } else {
                        // Ours-marker hook file still owned even when the
                        // bytes drifted from the recorded hash.
                        Some(std::str::from_utf8(&bytes).map(is_ours).unwrap_or(false))
                    }
                }
            },
        };
        match owned {
            None => {}
            Some(true) => {
                if std::fs::remove_file(&old.path).is_ok() {
                    removed.push(old.path.clone());
                } else {
                    // Vanished or turned into a dir between check and
                    // remove: leave it, report as skipped.
                    skipped.push(old.path.clone());
                }
            }
            Some(false) => skipped.push(old.path.clone()),
        }
    }
    (removed.into_boxed_slice(), skipped.into_boxed_slice())
}
