use std::path::{Path, PathBuf};

use super::*;
use crate::{Error, Result};

/// Intended host set, computed from this binary every time (baked hook
/// text, conf text, symlink targets). Heroic Flatpak `localfixes` entries
/// are included only when that tree exists. Never includes
/// `localfixes/__init__.py`, PREFIX internals, or `environment.d`.
pub fn intended_host_manifest(prefix: &Path, home: &Path) -> Vec<IntendedHostEntry> {
    let mut out = Vec::new();
    for name in ["tuxgt", "tuxgt-launcher"] {
        out.push(IntendedHostEntry {
            path: home.join(".local/bin").join(name),
            kind: HostKind::Symlink,
            symlink_target: Some(prefix.join("bin").join(name)),
            sha256: None,
        });
    }
    out.push(IntendedHostEntry {
        path: home.join(".local/share/applications/tuxgt.desktop"),
        kind: HostKind::Symlink,
        symlink_target: Some(prefix.join("share/applications/tuxgt.desktop")),
        sha256: None,
    });
    // Themed icons: intended only when the packaged prefix ships them
    // (`make prepare` always does). Dev/test prefixes without packaged
    // icons skip the entries so verify stays green there.
    if ICON_SIZES
        .iter()
        .all(|size| icon_path(prefix, *size).is_file())
    {
        for size in ICON_SIZES {
            let host = home.join(format!(
                ".local/share/icons/hicolor/{size}x{size}/apps/tuxgt.png"
            ));
            let sha = packaged_icon_bytes(prefix, size).map(|b| sha256_hex_bytes(&b));
            out.push(IntendedHostEntry {
                path: host,
                kind: HostKind::File,
                symlink_target: None,
                sha256: sha,
            });
        }
    }
    out.push(IntendedHostEntry {
        path: home.join(".config/tuxgt.conf"),
        kind: HostKind::File,
        symlink_target: None,
        sha256: Some(sha256_hex_bytes(intended_conf_text(prefix).as_bytes())),
    });
    let mut localfixes_dirs = vec![home.join(".config/protonfixes/localfixes")];
    if flatpak_tree_present(home) {
        localfixes_dirs.push(flatpak_localfixes(home));
    }
    for lf in localfixes_dirs {
        out.push(IntendedHostEntry {
            path: lf.join("tuxgt.py"),
            kind: HostKind::File,
            symlink_target: None,
            sha256: Some(sha256_hex_bytes(TUXGT_PY.as_bytes())),
        });
        out.push(IntendedHostEntry {
            path: lf.join("default.py"),
            kind: HostKind::File,
            symlink_target: None,
            sha256: Some(sha256_hex_bytes(intended_default_text(&lf).as_bytes())),
        });
    }
    out
}

pub(crate) fn verify_one(entry: &IntendedHostEntry) -> HostVerifyEntry {
    let status = match std::fs::symlink_metadata(&entry.path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HostStatus::Missing,
        Err(_) => HostStatus::Missing,
        Ok(meta) => {
            let is_link = meta.file_type().is_symlink();
            match entry.kind {
                HostKind::Symlink => {
                    if !is_link {
                        HostStatus::WrongKind
                    } else {
                        let actual = std::fs::read_link(&entry.path).ok();
                        if actual.as_deref() == entry.symlink_target.as_deref() {
                            HostStatus::Ok
                        } else {
                            HostStatus::WrongTarget
                        }
                    }
                }
                HostKind::File => {
                    if is_link {
                        HostStatus::WrongKind
                    } else if !meta.file_type().is_file() {
                        HostStatus::WrongKind
                    } else {
                        let actual = std::fs::read(&entry.path)
                            .ok()
                            .map(|b| sha256_hex_bytes(&b));
                        if actual.as_deref() == entry.sha256.as_deref() {
                            HostStatus::Ok
                        } else {
                            HostStatus::Modified
                        }
                    }
                }
            }
        }
    };
    HostVerifyEntry {
        path: entry.path.clone(),
        kind: entry.kind,
        status,
    }
}

/// Verify intended (this binary) vs disk for every intended host path.
/// Missing inventory does not matter: every intended path is still verified.
pub fn verify_host_install(prefix: &Path, home: &Path) -> Vec<HostVerifyEntry> {
    intended_host_manifest(prefix, home)
        .iter()
        .map(verify_one)
        .collect()
}

/// `tuxgt install --check` report shape: one `path\tstatus` line per
/// non-`ok` entry, in intended-manifest order, except the 9 themed-icon
/// entries collapse to one `icons\tn/9 STATUS` line. Writes nothing.
pub fn install_check_lines(report: &[HostVerifyEntry]) -> Vec<String> {
    let mut lines: Vec<String> = report
        .iter()
        .filter(|e| e.status != HostStatus::Ok && !is_icon_host_path(&e.path))
        .map(|e| format!("{}\t{}", e.path.display(), e.status))
        .collect();
    if let Some(line) = icons_check_line(report) {
        lines.push(line);
    }
    lines
}

/// Collapsed themed-icon check line: `None` when every icon is `ok` (or
/// there are no icon entries, e.g. dev prefixes); else `icons\tn/9 STATUS`
/// where STATUS is `missing` if any icon is missing, else `modified`
/// (covers modified/wrong-target/wrong-kind).
pub fn icons_check_line(report: &[HostVerifyEntry]) -> Option<String> {
    let icons: Vec<&HostVerifyEntry> = report
        .iter()
        .filter(|e| is_icon_host_path(&e.path))
        .collect();
    if icons.is_empty() {
        return None;
    }
    let ok = icons.iter().filter(|e| e.status == HostStatus::Ok).count();
    if ok == icons.len() {
        return None;
    }
    let status = if icons.iter().any(|e| e.status == HostStatus::Missing) {
        "missing"
    } else {
        "modified"
    };
    Some(format!("icons\t{}/{}\t{status}", ok, icons.len()))
}

/// Collapse a host-path list for display: every non-icon path stays its
/// own entry, and all icon paths collapse to one `icons\tn/9` entry (`n` =
/// icon paths present in the list). Pure grouping for
/// install/uninstall/confirm output; verify internals stay per-file.
pub fn collapse_icon_paths(paths: &[PathBuf]) -> (Vec<PathBuf>, Option<String>) {
    let n = paths.iter().filter(|p| is_icon_host_path(p)).count();
    let rest: Vec<PathBuf> = paths
        .iter()
        .filter(|p| !is_icon_host_path(p))
        .cloned()
        .collect();
    if n == 0 {
        return (rest, None);
    }
    (rest, Some(format!("icons\t{}/{}", n, ICON_SIZES.len())))
}

/// `tuxgt install --check` summary: `ok\t<ok>/<total>`.
pub fn install_check_summary(report: &[HostVerifyEntry]) -> String {
    let ok = report.iter().filter(|e| e.status == HostStatus::Ok).count();
    format!("ok\t{}/{}", ok, report.len())
}

/// `tuxgt install --check` exit decision: `true` (exit 0) iff every
/// intended path is `ok`.
pub fn install_check_ok(report: &[HostVerifyEntry]) -> bool {
    report.iter().all(|e| e.status == HostStatus::Ok)
}

/// E70: a host path required for handled Play / Apply: the protonfixes
/// `localfixes` trees (`tuxgt.py`, `default.py`, native + Heroic Flatpak).
/// PATH links, the desktop entry, and the boot conf are not required to
/// Play from PREFIX, so drift there never warns at the op.
pub fn is_required_host_path(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "localfixes")
}

/// E70: non-`ok` required entries of an E67 session report, in
/// intended-manifest order. Empty means the op needs no health toast.
pub fn required_host_failures(report: &[HostVerifyEntry]) -> Vec<&HostVerifyEntry> {
    report
        .iter()
        .filter(|e| e.status != HostStatus::Ok && is_required_host_path(&e.path))
        .collect()
}

/// E70: one-line summary of required drift for the GUI toast: the failure
/// count, whether any failure is `missing` (else the short reason is
/// `modified`, covering `modified` / `wrong-target` / `wrong-kind`), and an
/// example path for the Fluent `{ $path }` slot. `None` when healthy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredHostWarning {
    pub count: usize,
    pub missing: bool,
    pub example: PathBuf,
}

pub fn required_host_warning(report: &[HostVerifyEntry]) -> Option<RequiredHostWarning> {
    let failures = required_host_failures(report);
    let first = failures.first()?;
    Some(RequiredHostWarning {
        count: failures.len(),
        missing: failures.iter().any(|e| e.status == HostStatus::Missing),
        example: first.path.clone(),
    })
}

/// `$PREFIX/config/host-install.toml`.
pub(crate) fn host_inventory_path(prefix: &Path) -> PathBuf {
    prefix.join("config/host-install.toml")
}

/// Build the inventory record for a just-completed install from the
/// intended set. `wrapped` lists `localfixes` dirs where a foreign
/// `default.py` was renamed to `_tuxgt_wrapped_default.py`.
pub fn host_inventory_from_intended(
    prefix: &Path,
    intended: &[IntendedHostEntry],
    wrapped: &[PathBuf],
) -> HostInstallInventory {
    HostInstallInventory {
        prefix: prefix.to_path_buf(),
        paths: intended
            .iter()
            .map(|e| HostInventoryEntry {
                path: e.path.clone(),
                kind: e.kind,
                target: e.symlink_target.clone(),
                sha256: e.sha256.clone(),
            })
            .collect(),
        wrapped: wrapped.to_vec().into_boxed_slice(),
    }
}

/// Persist the last-successful-install inventory. Atomic write via tmp +
/// rename.
pub fn save_host_inventory(prefix: &Path, inventory: &HostInstallInventory) -> Result<()> {
    let path = host_inventory_path(prefix);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(inventory)
        .map_err(|e| Error::Install(format!("host inventory encode: {e}")))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Load the last-successful-install inventory. Missing file → `Ok(None)`.
pub fn load_host_inventory(prefix: &Path) -> Result<Option<HostInstallInventory>> {
    let path = host_inventory_path(prefix);
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
        Ok(text) => {
            let inv: HostInstallInventory = toml::from_str(&text)
                .map_err(|e| Error::Install(format!("host inventory parse: {e}")))?;
            Ok(Some(inv))
        }
    }
}
