use std::fs;
use std::path::Path;

use super::*;
use crate::instance::Mod;
use crate::{Error, Result};

pub async fn acquire_with_source(
    data_dir: &Path,
    inst: &Mod,
    redownload: bool,
    progress: ProgressSink<'_>,
) -> Result<(CachedAsset, String)> {
    // Single source path: resolution lives in instance::resolve_source.
    let source = crate::instance::resolve_source(inst);
    let kind = match &source {
        crate::instance::SourceRef::ManualUrl { .. } => "manual-url",
        crate::instance::SourceRef::Github { .. } => "github",
        crate::instance::SourceRef::Local { .. } => "local",
    };
    tracing::debug!(instance = inst.id.as_str(), source = kind, redownload, "acquire entry");
    let url = match &source {
        crate::instance::SourceRef::ManualUrl { url } => url.clone(),
        crate::instance::SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => github_source_url(owner, repo, tag.as_deref(), *prerelease, asset_glob).await?,
        crate::instance::SourceRef::Local { path } => {
            // Disk copy has no byte stream: the caller's card stays
            // indeterminate rather than faking a percent.
            let asset = cache_local(data_dir, Path::new(path), Some(&inst.id))?;
            tracing::info!(instance = inst.id.as_str(), bytes = asset.bytes, "acquired local");
            let key_src = local_key_src(Path::new(path))?;
            return Ok((asset, key_src));
        }
    };
    let asset = fetch_url(
        data_dir,
        &url,
        inst.sha256.as_deref(),
        Some(&inst.id),
        progress,
        redownload,
    )
    .await?;
    tracing::info!(instance = inst.id.as_str(), bytes = asset.bytes, "acquired");
    Ok((asset, url))
}

/// Canonical `local:<abs path>` key source for a disk path.
pub(crate) fn local_key_src(path: &Path) -> Result<String> {
    let canon = path
        .canonicalize()
        .map_err(|e| Error::Cache(format!("{}: {e}", path.display())))?;
    Ok(format!("local:{}", canon.display()))
}

/// Read-only digest of a local source file or directory (no cache write):
/// hex sha256 of bytes, or the tree digest for directories, plus byte size.
pub(crate) fn local_source_digest(path: &Path) -> Result<(String, u64)> {
    let canon = path
        .canonicalize()
        .map_err(|e| Error::Cache(format!("{}: {e}", path.display())))?;
    if !canon.is_file() && !canon.is_dir() {
        return Err(Error::NotAFile(canon.display().to_string()));
    }
    digest_path(&canon)
}

/// Actual on-disk digest of a cache entry by key, plus the recorded source
/// label (`meta.toml` url: download URL or `local:<abs path>`). Read-only:
/// no fetch, no re-record. Used by the E76 provenance repair to rebuild a
/// missing `[provenance]` from the bytes already on disk, without
/// re-unpacking.
pub(crate) fn cached_entry_digest(
    data_dir: &Path,
    key: &str,
) -> Result<Option<(String, String, u64)>> {
    let entry = cache_dir(data_dir).join(key);
    let Some(meta) = read_meta(&entry)? else {
        return Ok(None);
    };
    let file = entry.join(&meta.filename);
    if !file.exists() {
        return Ok(None);
    }
    let (sha, bytes) = digest_path(&file)?;
    Ok(Some((meta.url, sha, bytes)))
}

pub(crate) fn clear_cache_payload(entry: &Path) -> Result<()> {
    if !entry.exists() {
        return Ok(());
    }
    for e in fs::read_dir(entry)? {
        let p = e?.path();
        if p.file_name().is_some_and(|n| n == "meta.toml") {
            continue;
        }
        if p.is_dir() {
            fs::remove_dir_all(&p)?;
        } else {
            let _ = fs::remove_file(&p);
        }
    }
    Ok(())
}

/// Copy a local file or directory into the shared cache. Same meta/log shape as downloads.
pub fn cache_local(data_dir: &Path, path: &Path, instance: Option<&str>) -> Result<CachedAsset> {
    let canon = path
        .canonicalize()
        .map_err(|e| Error::Cache(format!("{}: {e}", path.display())))?;
    if !canon.is_file() && !canon.is_dir() {
        return Err(Error::NotAFile(canon.display().to_string()));
    }
    let key_src = format!("local:{}", canon.display());
    let entry = cache_dir(data_dir).join(url_key(&key_src));
    fs::create_dir_all(&entry)?;
    let raw = canon
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asset".into());
    let filename = filename_from_url(&format!("local://host/{raw}"));
    let (digest, bytes) = digest_path(&canon)?;
    if let Some(hit) = cache_valid(&entry, Some(&digest))? {
        return Ok(hit);
    }
    clear_cache_payload(&entry)?;
    let dest = entry.join(&filename);
    if canon.is_dir() {
        copy_tree(&canon, &dest)?;
    } else {
        fs::copy(&canon, &dest)?;
    }
    record_cached_asset(
        data_dir, &entry, &key_src, &filename, digest, bytes, instance,
    )
}
