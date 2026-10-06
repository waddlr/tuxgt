//! Apply one self-update: fetch the release tarball, overlay the owned
//! PREFIX set, refresh host files from the new binary. Never opens the DB.

mod stale;

use std::collections::HashSet;
use std::path::Path;

pub(crate) use stale::remove_stale;

use super::manifest::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
use super::parse_app_version;
use crate::download::{
    cancelled, drop_download, fetch_url, sha256_file, unpack, CancelFlag, ProgressSink,
};
use crate::{Error, Result};

/// Owned PREFIX roots. A package path outside these (or the previous
/// manifest holding one) is refused, never written or deleted.
const OWNED_ROOTS: [&str; 4] = ["bin/", "lib/", "share/", "mods/official/"];

/// Deploy parity: the rewritten desktop entry is written only when missing.
const DESKTOP: &str = "share/applications/tuxgt.desktop";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppUpdateReport {
    pub tag: String,
    /// Owned files overlaid (the skipped-when-present desktop entry excluded).
    pub updated: usize,
    /// Removed previous-owned relative paths (a dropped official payload
    /// dir reports as `mods/official/<id>/`).
    pub removed: Box<[String]>,
}

pub async fn apply_app_update(
    prefix: &Path,
    tag: &str,
    asset_url: &str,
    progress: ProgressSink<'_>,
    cancel: CancelFlag<'_>,
) -> Result<AppUpdateReport> {
    if !prefix.join("bin/tuxgt").is_file() {
        return Err(Error::Update(format!(
            "not an installed prefix: {}; run `tuxgt install` first",
            prefix.display()
        )));
    }
    // A failed fetch keeps its `.part` for resume; once the bytes are in
    // hand the entry is spent either way (a retry force-refetches), so it
    // always drops and `downloads/` is empty at rest.
    let asset = fetch_url(
        prefix,
        asset_url,
        None,
        Some("tuxgt"),
        progress,
        true,
        cancel,
    )
    .await?;
    if cancelled(cancel) {
        drop_download(prefix, &asset.key);
        return Err(Error::Cancelled);
    }
    let scratch = prefix
        .join("downloads")
        .join(format!(".update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;
    let out = apply_unpacked(prefix, tag, &asset.file, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    drop_download(prefix, &asset.key);
    out
}

/// Apply a local release tarball: the same owned-overlay machinery as
/// [`apply_app_update`], minus the fetch and the version check.
/// `tuxgt update --from` (and `install.sh`, which keeps its own fetch).
pub fn apply_local_update(prefix: &Path, tag: &str, asset: &Path) -> Result<AppUpdateReport> {
    if !prefix.join("bin/tuxgt").is_file() {
        return Err(Error::Update(format!(
            "not an installed prefix: {}; run `tuxgt install` first",
            prefix.display()
        )));
    }
    if parse_app_version(tag).is_none() {
        return Err(Error::Update(format!("cannot parse release tag: {tag}")));
    }
    if !asset.is_file() {
        return Err(Error::Update(format!(
            "no such tarball: {}",
            asset.display()
        )));
    }
    let scratch = prefix
        .join("downloads")
        .join(format!(".update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;
    let out = apply_unpacked(prefix, tag, asset, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    out
}

fn apply_unpacked(
    prefix: &Path,
    tag: &str,
    asset: &Path,
    scratch: &Path,
) -> Result<AppUpdateReport> {
    unpack(asset, scratch)?;
    let owned = collect_owned(scratch)?;
    if !owned.iter().any(|(rel, _)| rel == "bin/tuxgt") {
        return Err(Error::Update("release tree has no bin/tuxgt".into()));
    }
    let previous = load_prefix_manifest(prefix)?;
    let mut updated = 0usize;
    let mut rows = Vec::with_capacity(owned.len());
    for (rel, sha) in &owned {
        let dest = prefix.join(rel);
        if rel == DESKTOP && dest.is_file() {
            // Deploy parity: keep the rewritten entry, track what is there.
            rows.push(PrefixFile {
                path: rel.clone(),
                sha256: sha256_file(&dest)?,
            });
            continue;
        }
        copy_tmp_rename(&scratch.join(rel), &dest)?;
        set_package_mode(&dest, rel)?;
        let landed = sha256_file(&dest)?;
        if landed != *sha {
            return Err(Error::Update(format!(
                "{rel}: landed bytes do not match the package"
            )));
        }
        rows.push(PrefixFile {
            path: rel.clone(),
            sha256: landed,
        });
        updated += 1;
    }
    let new_set: HashSet<String> = owned.into_iter().map(|(rel, _)| rel).collect();
    let removed = remove_stale(prefix, previous.as_ref(), &new_set, Error::Update)?;
    save_prefix_manifest(
        prefix,
        &PrefixManifest {
            tag: tag.into(),
            files: rows,
        },
    )?;
    refresh_host(prefix)?;
    Ok(AppUpdateReport {
        tag: tag.into(),
        updated,
        removed,
    })
}

/// Sorted owned `(rel, sha256)` rows of the unpacked tree. Refuses symlinks,
/// non-file kinds, and paths outside the owned roots.
fn collect_owned(root: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    collect_dir(root, root, &mut out)?;
    out.sort();
    for (rel, _) in &out {
        if !is_owned(rel) {
            return Err(Error::Update(format!(
                "release tree holds an unowned path: {rel}"
            )));
        }
    }
    Ok(out)
}

fn collect_dir(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(Error::Update(format!(
                "release tree holds a symlink: {}",
                path.display()
            )));
        }
        if meta.is_dir() {
            collect_dir(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| Error::Update("release tree walk escaped".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            let sha = sha256_file(&path)?;
            out.push((rel, sha));
        } else {
            return Err(Error::Update(format!(
                "release tree holds a non-file: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn is_owned(rel: &str) -> bool {
    use std::path::Component;
    let anchored = !std::path::Path::new(rel).components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    anchored && OWNED_ROOTS.iter().any(|r| rel.starts_with(r))
}

/// Copy one package file into place atomically (same-dir tmp + rename),
/// so a failed update never leaves a half-written binary.
fn copy_tmp_rename(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = dest
        .file_name()
        .ok_or_else(|| Error::Update(format!("unnameable dest: {}", dest.display())))?;
    let tmp = dest.with_file_name(format!(
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

/// Host refresh as the NEW binary, so baked hook text comes from the new
/// build. `--prefix` pins the live prefix (a bare `install --yes` would
/// move it to `~/tuxgt`). Output is captured — success stays quiet, a
/// failure names the manual repair; the prefix itself is already new.
fn refresh_host(prefix: &Path) -> Result<()> {
    let out = std::process::Command::new(prefix.join("bin/tuxgt"))
        .arg("install")
        .arg("--prefix")
        .arg(prefix)
        .arg("--yes")
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    let mut msg = format!("host refresh failed ({})", out.status);
    let tail = String::from_utf8_lossy(&out.stderr);
    if let Some(last) = tail.lines().last().filter(|l| !l.trim().is_empty()) {
        msg.push_str(": ");
        msg.push_str(last.trim());
    }
    msg.push_str("; the prefix is new — re-run `tuxgt install`");
    Err(Error::Update(msg))
}
