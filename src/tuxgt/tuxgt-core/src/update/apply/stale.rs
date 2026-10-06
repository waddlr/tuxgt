//! Stale-file removal for self-update: manifest-backed when the prefix
//! has one, official-recipe tree comparison for legacy prefixes.

use std::collections::HashSet;
use std::path::Path;

use super::super::manifest::PrefixManifest;
use super::is_owned;
use crate::{Error, Result};

/// Drop previous-owned files missing from the new set. Paths outside the
/// owned roots are never deleted; a dropped official recipe also drops its
/// payload dir (deploy parity). No previous manifest (legacy prefix) runs
/// only the official-recipe tree comparison below; anything else stale is
/// kept. A removal that fails for any reason other than already-gone
/// errors loudly (the manifest is saved after this, so a re-run retries
/// cleanly). Shared with install (`reconcile_prefix_manifest`); `err`
/// maps failures to the caller's domain.
pub(crate) fn remove_stale(
    prefix: &Path,
    previous: Option<&PrefixManifest>,
    owned: &HashSet<String>,
    err: impl Fn(String) -> Error,
) -> Result<Box<[String]>> {
    let Some(prev) = previous else {
        return drop_gone_officials(prefix, owned, err);
    };
    let mut removed = Vec::new();
    for f in &prev.files {
        if owned.contains(&f.path) || !is_owned(&f.path) {
            continue;
        }
        match std::fs::remove_file(prefix.join(&f.path)) {
            Ok(()) => removed.push(f.path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(err(format!("cannot remove stale {}: {e}", f.path))),
        }
        if let Some(id) = f
            .path
            .strip_prefix("mods/official/")
            .and_then(|s| s.strip_suffix(".toml"))
        {
            if !id.contains('/') {
                let dir = prefix.join("mods/official").join(id);
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => removed.push(format!("mods/official/{id}/")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(err(format!(
                            "cannot remove dropped payload mods/official/{id}/: {e}"
                        )))
                    }
                }
            }
        }
    }
    Ok(removed.into_boxed_slice())
}

/// Legacy-prefix fallback (no manifest to compare against): dest
/// `mods/official/*.toml` files missing from the new owned set drop with
/// that id's payload dir — the same tree comparison `install` runs
/// without a manifest. No new official entry (a partial tree) skips the
/// pass: intent unknown, drop nothing.
fn drop_gone_officials(
    prefix: &Path,
    owned: &HashSet<String>,
    err: impl Fn(String) -> Error,
) -> Result<Box<[String]>> {
    if !owned.iter().any(|r| r.starts_with("mods/official/")) {
        return Ok(Box::default());
    }
    let mut removed = Vec::new();
    let dest_official = prefix.join("mods/official");
    let entries = match std::fs::read_dir(&dest_official) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Box::default()),
        Err(e) => return Err(Error::Io(e)),
    };
    for e in entries {
        let e = e?;
        let path = e.path();
        if !e.file_type()?.is_file() || path.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let rel = format!("mods/official/{name}");
        if owned.contains(&rel) {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => removed.push(rel),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(err(format!("cannot remove stale {}: {e}", path.display()))),
        }
        let id = name.strip_suffix(".toml").unwrap_or(&name);
        match std::fs::remove_dir_all(dest_official.join(id)) {
            Ok(()) => removed.push(format!("mods/official/{id}/")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(err(format!(
                    "cannot remove dropped payload mods/official/{id}/: {e}"
                )))
            }
        }
    }
    Ok(removed.into_boxed_slice())
}
