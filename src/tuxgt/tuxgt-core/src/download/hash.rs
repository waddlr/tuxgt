use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use super::*;
use crate::Result;

pub(crate) const CACHE_DIR: &str = "downloads";
pub(crate) const DOWNLOADS_LOG: &str = "downloads.log";
pub(crate) const MANIFESTS_DIR: &str = "manifests";
pub(crate) const USER_AGENT: &str = "tuxgt/0.1";

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

/// File name for a cached asset: last URL path segment, sanitized.
/// The on-disk name keeps its extension so archive detection works.
pub(crate) fn filename_from_url(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let path = after_scheme.split('?').next().unwrap_or(after_scheme);
    let last = path.rsplit('/').next().unwrap_or("");
    let clean: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim_matches(|c| c == '.' || c == '_');
    if clean.is_empty() {
        "asset".into()
    } else {
        clean.into()
    }
}

pub(crate) fn url_key(url: &str) -> String {
    sha256_hex(url.as_bytes())[..16].into()
}

/// One in-flight transfer per URL key: overlapping `fetch_url` waits, so
/// two writers never append the same `.part`.
pub(crate) fn entry_lock(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<StdMutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    let mut map = LOCKS
        .get_or_init(|| StdMutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Expected on-disk size after the body is written. `206` is remaining
/// bytes on top of `resume_from`; anything else is a full body (caller
/// has already truncated `.part` when the server ignored Range).
pub(crate) fn expected_bytes(
    partial: bool,
    resume_from: u64,
    content_length: Option<u64>,
) -> Option<u64> {
    content_length.map(|n| {
        if partial {
            resume_from.saturating_add(n)
        } else {
            n
        }
    })
}

pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_DIR)
}

/// Scratch extract dir. Never under PREFIX.
pub fn tmp_unpack_dir(key: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tuxgt-{key}"))
}

/// Drop an in-flight download entry after the payload has landed.
pub fn drop_download(data_dir: &Path, key: &str) {
    let _ = fs::remove_dir_all(cache_dir(data_dir).join(key));
}

/// Where `fetch_url` stores a URL, without fetching. In-flight only;
/// covers land under `config/cache/art/` and the hash dir is dropped.
pub fn cached_file(data_dir: &Path, url: &str) -> PathBuf {
    cache_dir(data_dir)
        .join(url_key(url))
        .join(filename_from_url(url))
}

pub(crate) fn game_id_safe(id: &str) -> String {
    id.replace([':', '/'], "_")
}

/// Cover file for one game: `$PREFIX/config/cache/art/<game-id-safe>/cover`.
pub fn art_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("config").join("cache").join("art")
}

pub fn art_file(data_dir: &Path, game_id: &str) -> PathBuf {
    art_dir(data_dir).join(game_id_safe(game_id)).join("cover")
}

pub fn hero_file(data_dir: &Path, game_id: &str) -> PathBuf {
    art_dir(data_dir).join(game_id_safe(game_id)).join("hero")
}

pub fn drop_game_art(data_dir: &Path, game_id: &str) {
    let _ = fs::remove_dir_all(art_dir(data_dir).join(game_id_safe(game_id)));
}

/// Hero file for one game only. AppID changes re-fetch the hero while the
/// cover stays cached.
pub fn drop_game_hero(data_dir: &Path, game_id: &str) {
    let _ = fs::remove_file(hero_file(data_dir, game_id));
}

/// E107: square rail icon for one game:
/// `$PREFIX/config/cache/art/<game-id-safe>/icon`.
pub fn icon_file(data_dir: &Path, game_id: &str) -> PathBuf {
    art_dir(data_dir).join(game_id_safe(game_id)).join("icon")
}

/// E107: an AppID change moves the game on SteamGridDB, so the cached icon
/// goes with the hero.
pub fn drop_game_icon(data_dir: &Path, game_id: &str) {
    let _ = fs::remove_file(icon_file(data_dir, game_id));
}

/// Copy a fetched SteamGridDB icon into the art cache.
pub fn land_icon(data_dir: &Path, game_id: &str, src: &Path) {
    land_art_named(data_dir, src, icon_file(data_dir, game_id));
}

/// Copy a fetched cover into the art cache, then cap the cache (LRU by mtime).
pub fn land_art(data_dir: &Path, game_id: &str, src: &Path) {
    land_art_named(data_dir, src, art_file(data_dir, game_id));
}

/// Copy a fetched hero (wide wash) into the art cache.
pub fn land_hero(data_dir: &Path, game_id: &str, src: &Path) {
    land_art_named(data_dir, src, hero_file(data_dir, game_id));
}

pub(crate) fn land_art_named(data_dir: &Path, src: &Path, dest: PathBuf) {
    if let Some(parent) = dest.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::copy(src, &dest);
    cap_art_cache(data_dir);
}

pub(crate) fn cap_art_cache(data_dir: &Path) {
    // Thumbs are small (~150KB/game); the cap bounds fetched originals.
    // Evicted games re-render from local files, or re-fetch on next scan.
    const CAP: usize = 1024;
    let root = art_dir(data_dir);
    let Ok(rd) = fs::read_dir(&root) else {
        return;
    };
    let mut dirs: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let m = fs::metadata(&p).ok()?.modified().ok()?;
            Some((m, p))
        })
        .collect();
    if dirs.len() <= CAP {
        return;
    }
    dirs.sort_by_key(|(t, _)| *t);
    let extra = dirs.len() - CAP;
    for (_, p) in dirs.into_iter().take(extra) {
        let _ = fs::remove_dir_all(p);
    }
}

/// Drop `downloads/<hash>/` dirs that are not in-flight (no `.part`).
pub fn drop_spent_downloads(data_dir: &Path) {
    let root = cache_dir(data_dir);
    let Ok(rd) = fs::read_dir(&root) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let has_part = fs::read_dir(&p).ok().is_some_and(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".part"))
        });
        if !has_part {
            let _ = fs::remove_dir_all(&p);
        }
    }
}

pub fn manifests_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MANIFESTS_DIR)
}

pub(crate) fn game_manifests_dir(data_dir: &Path, game_id: &str) -> Result<PathBuf> {
    let id = crate::game::GameId::parse(game_id)?;
    Ok(crate::game::game_dir(data_dir, &id).join(MANIFESTS_DIR))
}

pub fn manifest_path(data_dir: &Path, game_id: &str, instance_id: &str) -> PathBuf {
    let safe = |s: &str| s.replace([':', '/'], "_");
    game_manifests_dir(data_dir, game_id)
        .unwrap_or_else(|_| manifests_dir(data_dir))
        .join(format!("{}.toml", safe(instance_id)))
}

/// All manifests installed for one game (one file per instance).
pub fn game_manifests(data_dir: &Path, game_id: &str) -> Result<Vec<FileManifest>> {
    let dir = game_manifests_dir(data_dir, game_id)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for path in names {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(file = %path.display(), "unreadable manifest: {e}");
                continue;
            }
        };
        let m: FileManifest = match toml::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(file = %path.display(), "bad manifest: {e}");
                continue;
            }
        };
        if m.game == game_id {
            out.push(m);
        }
    }
    out.sort_by(|a, b| (a.load_order, &a.instance).cmp(&(b.load_order, &b.instance)));
    Ok(out)
}
