//! Deploy-parity prefix overlay for `tuxgt install` over an existing prefix.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::update::{
    load_prefix_manifest, remove_stale, save_prefix_manifest, PrefixFile, PrefixManifest,
};
use crate::{sha256_file, Error, Result};

/// Owned PREFIX roots mirrored from the source tree (`make deploy` parity).
const OWNED_ROOTS: [&str; 4] = ["bin/", "lib/", "share/", "mods/official/"];

/// Never copied here: `rewrite_desktop` writes it later in the install.
const DESKTOP: &str = "share/applications/tuxgt.desktop";

/// Copy the packaged program set from the unpacked source tree over the
/// live prefix and return the overlaid rels: every regular file under the
/// owned roots (symlinks and other kinds skipped, never followed), tmp +
/// rename per file, package modes (`755` `bin/*` + `lib/*.so`, `644` the
/// rest). Dest official TOMLs missing from a present source
/// `mods/official/` drop with their payload dirs. Never touches `games/`,
/// `downloads/`, `config/`, `mods/user/`, registries, or kept official
/// payloads.
pub(crate) fn overlay_prefix_tree(src: &Path, dest: &Path) -> Result<Vec<String>> {
    let files = owned_rels(src)?;
    for rel in &files {
        let target = dest.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        copy_tmp_rename(&src.join(rel), &target)?;
        set_package_mode(&target, rel)?;
    }
    drop_gone_officials(src, dest)?;
    Ok(files)
}

/// Owned relative paths (`/` separators) of one tree with install rules:
/// `bin/`/`lib/`/`share/` deep (minus the rewritten desktop entry),
/// `mods/official/` top-level TOMLs only (tarball layout; anything deeper
/// is a payload dir, never source material — so a same-tree run can never
/// adopt payload files a later update would delete).
pub(crate) fn owned_rels(tree: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for root in OWNED_ROOTS {
        let official = root == "mods/official/";
        collect_owned(tree, root, !official, official, &mut files)?;
    }
    files.sort();
    Ok(files)
}

/// Settle `$PREFIX/config/prefix-manifest.toml` after an install and drop
/// previous-owned files the new set no longer ships (update parity: same
/// manifest file, same `remove_stale`). `set_tree` is the unpacked source
/// tree on overlay, else the live prefix itself; bytes are always hashed
/// from the live prefix (landed bytes). The rewritten desktop entry tracks
/// dest bytes when present, like update's kept entry. No `bin/tuxgt` in the
/// new set means a partial tree: intent unknown, drop nothing, save nothing
/// (`drop_gone_officials` parity). Returns removed relative paths.
pub(crate) fn reconcile_prefix_manifest(
    set_tree: &Path,
    prefix: &Path,
    tag: &str,
) -> Result<Box<[String]>> {
    let rels = owned_rels(set_tree)?;
    if !rels.iter().any(|r| r == "bin/tuxgt") {
        return Ok(Box::default());
    }
    let previous = load_prefix_manifest(prefix).map_err(to_install_err)?;
    let mut rows = Vec::with_capacity(rels.len() + 1);
    for rel in &rels {
        rows.push(PrefixFile {
            path: rel.clone(),
            sha256: sha256_file(&prefix.join(rel))?,
        });
    }
    if prefix.join(DESKTOP).is_file() {
        rows.push(PrefixFile {
            path: DESKTOP.into(),
            sha256: sha256_file(&prefix.join(DESKTOP))?,
        });
    }
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    let new_set: HashSet<String> = rows.iter().map(|r| r.path.clone()).collect();
    let removed = remove_stale(prefix, previous.as_ref(), &new_set, Error::Install)?;
    save_prefix_manifest(
        prefix,
        &PrefixManifest {
            tag: tag.into(),
            files: rows,
        },
    )
    .map_err(to_install_err)?;
    Ok(removed)
}

/// `load/save_prefix_manifest` speak `Error::Update`; on the install path
/// a corrupt manifest must fail as install, not update.
fn to_install_err(e: Error) -> Error {
    match e {
        Error::Update(msg) => Error::Install(msg),
        other => other,
    }
}

/// Sorted-then-collected owned relative paths (`/` separators) under one
/// root. Missing root contributes nothing (a partial tree overlays what
/// it has); symlinks and non-file kinds are skipped, never followed.
fn collect_owned(
    src: &Path,
    root: &str,
    deep: bool,
    tomls_only: bool,
    out: &mut Vec<String>,
) -> Result<()> {
    let dir = src.join(root);
    let entries = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Io(e)),
    };
    for e in entries {
        let e = e?;
        let path = e.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if deep {
                collect_dir(src, &path, out)?;
            }
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        if tomls_only && path.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        push_rel(src, &path, out);
    }
    Ok(())
}

fn collect_dir(src: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || (!meta.is_file() && !meta.is_dir()) {
            continue;
        }
        if meta.is_dir() {
            collect_dir(src, &path, out)?;
        } else {
            push_rel(src, &path, out);
        }
    }
    Ok(())
}

fn push_rel(src: &Path, path: &Path, out: &mut Vec<String>) {
    let rel = path
        .strip_prefix(src)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if rel != DESKTOP {
        out.push(rel);
    }
}

/// Deploy parity: dest official TOMLs missing from the source tree drop
/// with that id's payload dir. No source `mods/official/` (a partial
/// tree) skips the pass — intent unknown, drop nothing.
fn drop_gone_officials(src: &Path, dest: &Path) -> Result<()> {
    let src_official = src.join("mods/official");
    if !src_official.is_dir() {
        return Ok(());
    }
    let dest_official = dest.join("mods/official");
    let entries = match std::fs::read_dir(&dest_official) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::Io(e)),
    };
    for e in entries {
        let e = e?;
        let path = e.path();
        if !e.file_type()?.is_file() || path.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if src_official.join(name.as_ref()).is_file() {
            continue;
        }
        std::fs::remove_file(&path)?;
        let id = name.strip_suffix(".toml").unwrap_or(&name);
        drop_gone_payload(&dest_official.join(id), id)?;
    }
    Ok(())
}

fn drop_gone_payload(dir: &Path, id: &str) -> Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Install(format!(
            "cannot remove dropped payload mods/official/{id}/: {e}"
        ))),
    }
}

/// Copy one file into place atomically (same-dir tmp + rename), so a
/// failed install never leaves a half-written binary (plain `copy`
/// would truncate a running dest binary in place).
fn copy_tmp_rename(src: &Path, dest: &Path) -> Result<()> {
    let name = dest
        .file_name()
        .ok_or_else(|| Error::Install(format!("unnameable dest: {}", dest.display())))?;
    let tmp: PathBuf = dest.with_file_name(format!(
        "{}.tmp-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let out = std::fs::copy(src, &tmp)
        .map_err(Error::Io)
        .and_then(|_| std::fs::rename(&tmp, dest).map_err(Error::Io));
    if out.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    out
}

/// Deploy parity (`install -m755/-m644` in `prepare`): executables and the
/// preload objects are `755`, everything else `644`.
fn set_package_mode(dest: &Path, rel: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exec = rel.starts_with("bin/") || (rel.starts_with("lib/") && rel.ends_with(".so"));
        let mode = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = (dest, rel);
    Ok(())
}
