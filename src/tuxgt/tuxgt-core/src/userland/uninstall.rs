use std::path::{Path, PathBuf};

use super::*;
use crate::{Error, Result};

/// What `tuxgt uninstall` removed or left alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UninstallReport {
    pub prefix: PathBuf,
    /// Recorded host paths we still owned, now deleted, plus the
    /// inventory file itself.
    pub removed: Vec<PathBuf>,
    /// Recorded paths left in place: user-modified (not ours), or under
    /// PREFIX (never delete the tree).
    pub skipped: Box<[PathBuf]>,
}

/// Remove the tracked host files of a previous `tuxgt install`.
///
/// Loads `$PREFIX/config/host-install.toml`. A missing inventory is an
/// error telling the user to run `tuxgt install` once — the delete set is
/// never guessed from the intended manifest. Each recorded path goes
/// through the same ownership rule as E66 stale-delete (our symlink
/// target, recorded file sha256, or the hook marker); missing paths are
/// already gone. Wrapped foreign `default.py` files are restored via
/// `uninstall_proton_hook` on each recorded `localfixes` tree, and the
/// inventory file is removed last. Never deletes PREFIX itself, `sqlite`,
/// `games/`, `mods/`, `downloads/`, or `environment.d`: paths under
/// PREFIX land in `skipped`, and only files/symlinks are ever removed.
pub fn uninstall_userland(prefix: &Path) -> Result<UninstallReport> {
    let prefix = std::fs::canonicalize(prefix).unwrap_or_else(|_| prefix.to_path_buf());
    let inventory = load_host_inventory(&prefix)?.ok_or_else(|| {
        Error::Install(format!(
            "no host inventory at {}: run `tuxgt install` once so there is a file list to remove",
            host_inventory_path(&prefix).display()
        ))
    })?;
    // Every recorded path goes through the E66 ownership rule; an empty
    // intended set means nothing is kept for being current.
    let (removed, skipped) = remove_stale_owned(&prefix, &[], &inventory);
    let mut removed = removed.into_vec();
    for localfixes in &inventory.wrapped {
        uninstall_proton_hook(localfixes)?;
    }
    let inv_path = host_inventory_path(&prefix);
    match std::fs::remove_file(&inv_path) {
        Ok(()) => removed.push(inv_path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(e)),
    }
    Ok(UninstallReport {
        prefix,
        removed,
        skipped,
    })
}
