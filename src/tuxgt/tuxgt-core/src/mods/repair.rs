use std::fs;
use std::path::Path;

use super::*;
use crate::Result;

pub(crate) async fn repair_provenance(
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<bool> {
    let manifest = crate::need_manifest(data_dir, game, instance)?;
    if !manifest.provenance.asset_sha256.is_empty() {
        return Ok(true);
    }
    let inst = find_mod(config_dir, data_dir, instance)?;
    let payload =
        crate::instance::payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    if dir_has_files(&payload) {
        if let Some(p) = crate::instance::read_payload_provenance(&payload) {
            if !p.asset_sha256.is_empty() {
                return write_repaired_provenance(
                    data_dir,
                    &manifest,
                    p.source,
                    p.asset_sha256,
                    p.asset_bytes,
                );
            }
        }
        match crate::instance::resolve_source(&inst) {
            crate::instance::SourceRef::Local { path } => {
                let (sha, bytes) = crate::download::local_source_digest(Path::new(&path))?;
                let source = crate::download::local_key_src(Path::new(&path))?;
                return write_repaired_provenance(data_dir, &manifest, source, sha, bytes);
            }
            _ => {
                if let Some(url) = literal_source_url(&inst) {
                    let (sha, bytes) = crate::download::digest_path(&payload)?;
                    return write_repaired_provenance(data_dir, &manifest, url, sha, bytes);
                }
            }
        }
    }
    // Prefer the manifest's own cache refs: the key survives even for glob
    // sources whose URL cannot be resolved offline, and the recorded meta
    // URL is the truthful source label.
    for f in &manifest.files {
        if let Some(rest) = f.source.strip_prefix("cache/") {
            if let Some((key, _)) = rest.split_once('/') {
                if let Some((source, sha, bytes)) =
                    crate::download::cached_entry_digest(data_dir, key)?
                {
                    return write_repaired_provenance(data_dir, &manifest, source, sha, bytes);
                }
                // Key known but cache entry gone: the unpack depot for that
                // key may still hold bytes (source label unknown here, so
                // only usable when the recipe names a literal source).
                if let Some(url) = literal_source_url(&inst) {
                    let depot = data_dir.join("unpack").join(key);
                    if dir_has_files(&depot) {
                        let (sha, bytes) = crate::download::digest_path(&depot)?;
                        return write_repaired_provenance(data_dir, &manifest, url, sha, bytes);
                    }
                }
            }
        }
    }
    match crate::instance::resolve_source(&inst) {
        crate::instance::SourceRef::Local { path } => {
            let (sha, bytes) = crate::download::local_source_digest(Path::new(&path))?;
            let source = crate::download::local_key_src(Path::new(&path))?;
            write_repaired_provenance(data_dir, &manifest, source, sha, bytes)
        }
        crate::instance::SourceRef::ManualUrl { url } => {
            let key = crate::download::url_key(&url);
            if let Some((_, sha, bytes)) = crate::download::cached_entry_digest(data_dir, &key)? {
                write_repaired_provenance(data_dir, &manifest, url.clone(), sha, bytes)
            } else {
                match depot_or_stage_digest(data_dir, game, instance, &url)? {
                    Some((sha, bytes)) => {
                        write_repaired_provenance(data_dir, &manifest, url.clone(), sha, bytes)
                    }
                    None => Ok(false),
                }
            }
        }
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            if asset_glob.contains(['*', '?', '[']) || prerelease {
                // Glob URL needs an upstream resolve (network): not path 1.
                // Without cache refs the bytes have no truthful source
                // label, so leave repair to the install path.
                Ok(false)
            } else {
                let url =
                    crate::download::github_release_url(&owner, &repo, tag.as_deref(), &asset_glob);
                let key = crate::download::url_key(&url);
                if let Some((_, sha, bytes)) = crate::download::cached_entry_digest(data_dir, &key)?
                {
                    write_repaired_provenance(data_dir, &manifest, url, sha, bytes)
                } else {
                    match depot_or_stage_digest(data_dir, game, instance, &url)? {
                        Some((sha, bytes)) => {
                            write_repaired_provenance(data_dir, &manifest, url, sha, bytes)
                        }
                        None => Ok(false),
                    }
                }
            }
        }
    }
}

/// Literal download URL named by a recipe, if it needs no upstream resolve.
pub(crate) fn literal_source_url(inst: &crate::instance::Mod) -> Option<String> {
    match crate::instance::resolve_source(inst) {
        crate::instance::SourceRef::ManualUrl { url } => Some(url),
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } if !asset_glob.contains(['*', '?', '[']) && !prerelease => Some(
            crate::download::github_release_url(&owner, &repo, tag.as_deref(), &asset_glob),
        ),
        _ => None,
    }
}

/// Payload, legacy `unpack/<key>`, or stage bytes for a literal source URL.
/// The digest is a repair baseline only: it converges to the real asset
/// hash on the next Update reinstall.
pub(crate) fn depot_or_stage_digest(
    data_dir: &Path,
    game: &str,
    instance: &str,
    url: &str,
) -> Result<Option<(String, u64)>> {
    for kind in ["official", "user"] {
        let depot = data_dir.join("mods").join(kind).join(instance);
        if dir_has_files(&depot) {
            return crate::download::digest_path(&depot).map(Some);
        }
    }
    let depot = data_dir.join("unpack").join(crate::download::url_key(url));
    if dir_has_files(&depot) {
        return crate::download::digest_path(&depot).map(Some);
    }
    let stage = crate::stage::stage_dir(data_dir, game, instance);
    if dir_has_files(&stage) {
        return crate::download::digest_path(&stage).map(Some);
    }
    Ok(None)
}

pub(crate) fn dir_has_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_file() {
            return true;
        }
        if p.is_dir() && dir_has_files(&p) {
            return true;
        }
    }
    false
}

pub(crate) fn write_repaired_provenance(
    data_dir: &Path,
    manifest: &crate::FileManifest,
    source: String,
    sha: String,
    bytes: u64,
) -> Result<bool> {
    let mut manifest = manifest.clone();
    manifest.provenance = crate::ModProvenance {
        source,
        asset_sha256: sha,
        asset_bytes: bytes,
        fetched_at: crate::download::now_unix(),
    };
    crate::write_manifest(data_dir, &manifest)?;
    Ok(true)
}

/// Path 3: fill the shared cache for one instance (the `cache refresh`
/// equivalent): re-fetch, no manifest touch.
pub(crate) async fn fill_update_cache(
    data_dir: &Path,
    config_dir: &Path,
    instance: &str,
) -> Result<()> {
    let inst = find_mod(config_dir, data_dir, instance)?;
    crate::acquire_with_source(data_dir, &inst, true, None)
        .await
        .map(|_| ())
}
