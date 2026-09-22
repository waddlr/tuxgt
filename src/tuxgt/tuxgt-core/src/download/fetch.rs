use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;
use crate::{atomic_write, Error, Result};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CacheMeta {
    pub(crate) url: String,
    #[serde(default = "default_filename")]
    pub(crate) filename: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) fetched_at: u64,
}

pub(crate) fn default_filename() -> String {
    "asset".into()
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn append_log(data_dir: &Path, line: &str) -> Result<()> {
    use std::io::Write;
    let dir = cache_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(DOWNLOADS_LOG))?;
    writeln!(f, "{line}")?;
    Ok(())
}

#[derive(Debug)]
pub struct CachedAsset {
    pub key: String,
    pub file: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

pub(crate) fn read_meta(dir: &Path) -> Result<Option<CacheMeta>> {
    let path = dir.join("meta.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let meta: CacheMeta = toml::from_str(&text).map_err(|e| Error::Cache(e.to_string()))?;
    Ok(Some(meta))
}

pub(crate) fn write_meta(dir: &Path, meta: &CacheMeta) -> Result<()> {
    let text = toml::to_string(meta).map_err(|e| Error::Cache(e.to_string()))?;
    atomic_write(&dir.join("meta.toml"), text.as_bytes())?;
    Ok(())
}

/// Is the cached asset present and does it hash clean? `pinned` enforces a
/// recipe hash; otherwise the recorded meta hash is the trust anchor.
/// Any miss re-fetches (logged); only a pinned mismatch errors.
/// Digest of a file or directory tree: file bytes, or the tree digest for
/// directories. Used by the E76 provenance repair to hash bytes already on
/// disk (unpack depot or stage) without re-unpacking.
pub(crate) fn digest_path(path: &Path) -> Result<(String, u64)> {
    if path.is_file() {
        Ok((sha256_file(path)?, fs::metadata(path)?.len()))
    } else {
        tree_digest(path)
    }
}

pub(crate) fn tree_digest(dir: &Path) -> Result<(String, u64)> {
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    for f in &files {
        let rel = f.strip_prefix(dir).unwrap_or(f).to_string_lossy();
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(sha256_file(f)?.as_bytes());
        hasher.update([0]);
        bytes += fs::metadata(f)?.len();
    }
    Ok((hex(&hasher.finalize()), bytes))
}

pub(crate) fn cache_valid(entry: &Path, pinned: Option<&str>) -> Result<Option<CachedAsset>> {
    let Some(meta) = read_meta(entry)? else {
        return Ok(None);
    };
    let file = entry.join(&meta.filename);
    if !file.exists() {
        return Ok(None);
    }
    let (actual, bytes) = digest_path(&file)?;
    let expect = pinned.unwrap_or(&meta.sha256);
    if actual != expect {
        return Ok(None);
    }
    Ok(Some(CachedAsset {
        key: entry
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        file,
        sha256: actual,
        bytes,
    }))
}

pub(crate) fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| Error::Download(e.to_string()))
}

/// E102: byte progress for one transfer. `total` is the expected on-disk size
/// (`Content-Length`, plus the `.part` resume offset on a 206); `None` when the
/// server sent no length — a progress bar goes indeterminate rather than
/// inventing a percent. Cloneable so a caller can ferry the latest value out of
/// the transfer thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FetchProgress {
    /// Bytes on disk for this transfer so far (resume offset + new chunks).
    pub bytes: u64,
    /// Expected total bytes, when the server advertised a length.
    pub total: Option<u64>,
}

impl FetchProgress {
    /// 0–100 percent, `None` when `total` is unknown or zero.
    pub fn percent(&self) -> Option<f32> {
        match self.total {
            Some(t) if t > 0 => Some((self.bytes as f32 / t as f32 * 100.0).clamp(0.0, 100.0)),
            _ => None,
        }
    }
}

/// E102: optional byte-progress sink. A borrowed trait object keeps the fetch
/// allocation-free and lets the caller own the state it writes into.
pub type ProgressSink<'a> = Option<&'a (dyn Fn(FetchProgress) + Send + Sync)>;

/// One progress report; no-op without a sink.
pub(crate) fn report(progress: ProgressSink<'_>, bytes: u64, total: Option<u64>) {
    if let Some(f) = progress {
        f(FetchProgress { bytes, total });
    }
}

/// Record freshly-landed bytes as the cache entry: meta + log + handle.
/// Shared tail of `fetch_url_locked` and `cache_local`.
pub(crate) fn record_cached_asset(
    data_dir: &Path,
    entry: &Path,
    url: &str,
    filename: &str,
    digest: String,
    bytes: u64,
    instance: Option<&str>,
) -> Result<CachedAsset> {
    write_meta(
        entry,
        &CacheMeta {
            url: url.into(),
            filename: filename.into(),
            sha256: digest.clone(),
            bytes,
            fetched_at: now_unix(),
        },
    )?;
    append_log(
        data_dir,
        &format!(
            "{} instance={} url={url} sha256={digest} bytes={bytes} result=ok",
            now_unix(),
            instance.unwrap_or("-")
        ),
    )?;
    Ok(CachedAsset {
        key: url_key(url),
        file: entry.join(filename),
        sha256: digest,
        bytes,
    })
}

/// Download a URL into the shared cache (resume via `.part` + Range).
/// Returns the verified asset. Any cache miss (absent, corrupt, or changed
/// upstream hash) re-fetches; only a pinned-recipe mismatch errors.
/// `force` drops the payload first, so a valid entry re-fetches and a new
/// hash is accepted + re-recorded. No manifest touch.
pub async fn fetch_url(
    data_dir: &Path,
    url: &str,
    pinned: Option<&str>,
    instance: Option<&str>,
    progress: ProgressSink<'_>,
    force: bool,
) -> Result<CachedAsset> {
    tracing::debug!(url, instance = instance.unwrap_or("-"), force, pinned = pinned.is_some(), "fetch entry");
    let key = url_key(url);
    let lock = entry_lock(&key);
    let _guard = lock.lock().await;
    if force {
        let entry = cache_dir(data_dir).join(&key);
        for e in fs::read_dir(&entry).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_file() && p.file_name().is_some_and(|n| n != "meta.toml") {
                let _ = fs::remove_file(&p);
            }
        }
    }
    fetch_url_locked(data_dir, url, pinned, instance, progress).await
}

pub(crate) async fn fetch_url_locked(
    data_dir: &Path,
    url: &str,
    pinned: Option<&str>,
    instance: Option<&str>,
    progress: ProgressSink<'_>,
) -> Result<CachedAsset> {
    let entry = cached_file(data_dir, url);
    let entry = entry
        .parent()
        .ok_or_else(|| Error::Cache("cached file has no parent dir".into()))?
        .to_path_buf();
    fs::create_dir_all(&entry)?;
    if let Some(hit) = cache_valid(&entry, pinned)? {
        tracing::info!(instance = instance.unwrap_or("-"), bytes = hit.bytes, "cache hit");
        return Ok(hit);
    }
    let filename = filename_from_url(url);
    let _ = fs::remove_file(entry.join(&filename));
    let client = client()?;
    let part = entry.join(format!("{filename}.part"));
    let resume_from = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if resume_from > 0 {
        req = req.header("Range", format!("bytes={resume_from}-"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| Error::Download(e.to_string()))?;
    let status = resp.status();
    let mut wrote_from = resume_from;
    if resume_from > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        fs::write(&part, b"")?;
        wrote_from = 0;
    }
    if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(Error::Download(format!("{url}: HTTP {status}")));
    }
    let expect = expected_bytes(
        status == reqwest::StatusCode::PARTIAL_CONTENT,
        wrote_from,
        resp.content_length(),
    );
    // E102: report before the first chunk so a bar paints with its total (and
    // any resume offset) instead of waiting on bytes that may take a while.
    report(progress, wrote_from, expect);
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut wrote = wrote_from;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Download(e.to_string()))?;
        f.write_all(&chunk)?;
        wrote += chunk.len() as u64;
        report(progress, wrote, expect);
    }
    drop(f);
    let bytes = fs::metadata(&part)?.len();
    if let Some(expect) = expect {
        if bytes != expect {
            let digest = sha256_file(&part).unwrap_or_default();
            append_log(
                data_dir,
                &format!(
                    "{} instance={} url={url} sha256={digest} bytes={bytes} result=incomplete",
                    now_unix(),
                    instance.unwrap_or("-")
                ),
            )?;
            return Err(Error::Download(format!(
                "{url}: incomplete ({bytes} of {expect} bytes)"
            )));
        }
    }
    let digest = sha256_file(&part)?;
    if let Some(pin) = pinned {
        if digest != pin {
            let _ = fs::remove_file(&part);
            append_log(
                data_dir,
                &format!(
                    "{} instance={} url={url} sha256={digest} bytes={bytes} result=pin-mismatch",
                    now_unix(),
                    instance.unwrap_or("-")
                ),
            )?;
            return Err(Error::HashMismatch {
                want: pin.into(),
                got: digest,
            });
        }
    }
    fs::rename(&part, entry.join(&filename))?;
    let host = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url);
    tracing::info!(host, bytes, "fetched");
    record_cached_asset(data_dir, &entry, url, &filename, digest, bytes, instance)
}

pub(crate) fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dest.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}
