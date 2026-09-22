use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::*;
use crate::detect::{detect_one, DetectOpts};
use crate::{Error, PluginHost, Result};

pub async fn add_manual(pool: &SqlitePool, host: &PluginHost, exe: &Path) -> Result<GameRow> {
    if !host.is_enabled("manual") {
        return Err(Error::PluginDisabled("manual".into()));
    }
    if !exe.is_file() {
        return Err(Error::NotAFile(exe.display().to_string()));
    }
    let canon = exe.canonicalize()?;
    let mut taken = taken_ids(pool).await?;
    let first = id8(&canon.to_string_lossy(), &HashSet::new());
    taken.remove(&format!("manual:standalone:{first}"));
    let rec = crate::provider::manual::record_from_exe(&canon, &taken)?;
    upsert_record(pool, &rec).await?;
    rebuild_fts(pool).await?;
    detect_one(pool, &rec.id.to_string(), DetectOpts::default()).await?;
    crate::db::mark_cache_dirty();
    Ok(GameRow {
        id: rec.id.to_string(),
        name: Some(rec.name),
        cover_path: rec.cover_path.map(|p| p.to_string_lossy().into_owned()),
        manager: rec.id.manager,
        store: rec.id.store,
        header_path: rec.header_path.map(|p| p.to_string_lossy().into_owned()),
        platform: None,
        api: None,
        install_dir: rec.install_dir.map(|p| p.to_string_lossy().into_owned()),
        exe_path: rec.exe_path.map(|p| p.to_string_lossy().into_owned()),
        prefix_path: rec.prefix_path.map(|p| p.to_string_lossy().into_owned()),
        proton: rec.proton,
        bitness: None,
        engine: None,
        hidden: rec.hidden,
        last_played: None,
        steam_appid: None,
    })
}
/// R01: persist last-played on Play. Sets `games.last_played` to now
/// (unix seconds). Unknown id errors; scan never writes this column.
pub async fn touch_last_played(pool: &SqlitePool, id: &str) -> Result<i64> {
    GameId::parse(id)?;
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    sqlx::query("UPDATE games SET last_played = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(now)
}

/// R34: manual game removal. Only `manual:standalone:*` rows can be removed;
/// Steam/Heroic rows are provider-owned and must disappear via scan/prune.
/// Deletes the DB row, rebuilds FTS, and best-effort removes the per-game dir.
pub async fn remove_manual(pool: &SqlitePool, data_dir: &Path, id: &str) -> Result<()> {
    let gid = GameId::parse(id)?;
    if gid.manager != "manual" {
        return Err(Error::InvalidGameId(format!("not a manual game: {id}")));
    }
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
    sqlx::query("DELETE FROM games WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    rebuild_fts(pool).await?;
    let dir = game_dir(data_dir, &gid);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    crate::download::drop_game_art(data_dir, id);
    crate::db::mark_cache_dirty();
    Ok(())
}

/// R38: proper manual add with name, prefix, art, and appid overlays.
/// `exe` is required (must exist); the rest are optional. Empty/blank
/// optionals are ignored. Art paths must be existing files or http(s) URLs;
/// prefix must be an existing dir when local. Appid validation matches
/// `set_steam_appid`. Returns the fresh row.
pub async fn add_manual_full(
    pool: &SqlitePool,
    host: &PluginHost,
    exe: &Path,
    name: Option<&str>,
    prefix: Option<&Path>,
    cover: Option<&str>,
    header: Option<&str>,
    appid: Option<&str>,
) -> Result<GameRow> {
    if !host.is_enabled("manual") {
        return Err(Error::PluginDisabled("manual".into()));
    }
    if !exe.is_file() {
        return Err(Error::NotAFile(exe.display().to_string()));
    }
    let canon = exe.canonicalize()?;
    let mut taken = taken_ids(pool).await?;
    let first = id8(&canon.to_string_lossy(), &HashSet::new());
    taken.remove(&format!("manual:standalone:{first}"));
    let mut rec = crate::provider::manual::record_from_exe(&canon, &taken)?;
    if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
        rec.name = n.to_string();
    }
    if let Some(p) = prefix {
        if !p.is_dir() {
            return Err(Error::NotAFile(format!(
                "prefix not a dir: {}",
                p.display()
            )));
        }
        rec.prefix_path = Some(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()));
    }
    let norm_art = |s: &str| -> Option<String> {
        let t = s.trim();
        if t.is_empty() {
            return None;
        }
        if t.starts_with("http://") || t.starts_with("https://") {
            return Some(t.to_string());
        }
        let p = Path::new(t);
        if p.is_file() {
            return Some(
                p.canonicalize()
                    .unwrap_or_else(|_| p.to_path_buf())
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        None
    };
    if let Some(c) = cover.and_then(norm_art) {
        rec.cover_path = Some(Path::new(&c).to_path_buf());
    }
    if let Some(h) = header.and_then(norm_art) {
        rec.header_path = Some(Path::new(&h).to_path_buf());
    }
    // Reject bad art paths explicitly (typo guard): a non-empty art that
    // normalizes to None is neither a file nor a URL.
    for (raw, label) in [(cover, "cover"), (header, "header")] {
        if let Some(r) = raw.map(str::trim).filter(|s| !s.is_empty()) {
            let ok_url = r.starts_with("http://") || r.starts_with("https://");
            if !ok_url && !Path::new(r).is_file() {
                return Err(Error::NotAFile(format!("{label} not found: {r}")));
            }
        }
    }
    upsert_record(pool, &rec).await?;
    let id = rec.id.to_string();
    // User prefix survives redetect: store as override (manual has no
    // runtime detector, but an override is explicit and future-proof).
    if let Some(p) = prefix {
        let canon_p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        sqlx::query("UPDATE games SET override_prefix_path = ? WHERE id = ?")
            .bind(canon_p.to_string_lossy().into_owned())
            .bind(&id)
            .execute(pool)
            .await?;
    }
    if let Some(a) = appid.map(str::trim).filter(|s| !s.is_empty()) {
        set_steam_appid(pool, &id, Some(a)).await?;
    }
    rebuild_fts(pool).await?;
    crate::db::mark_cache_dirty();
    detect_one(pool, &id, DetectOpts::default()).await?;
    let rows = list_games(pool, None, None, None).await?;
    rows.into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| Error::UnknownGame(id))
}

pub(crate) async fn taken_ids(pool: &SqlitePool) -> Result<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM games")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub fn id8(key: &str, taken: &HashSet<String>) -> String {
    id8_prefixed("", key, taken)
}

/// `prefix` is the display prefix before the 8id (e.g. `manual:standalone:`).
pub fn id8_prefixed(prefix: &str, key: &str, taken: &HashSet<String>) -> String {
    let digits = base62_sha256(key.as_bytes());
    let mut i = 0;
    while i + 8 <= digits.len() {
        let slice = &digits[i..i + 8];
        let display = format!("{prefix}{slice}");
        if !taken.contains(&display) {
            return slice.to_string();
        }
        i += 8;
    }
    let mut n = 0u32;
    loop {
        let extra = format!("{n}");
        let mut slice = digits[digits.len().saturating_sub(8.min(digits.len()))..].to_string();
        while slice.len() < 8 {
            slice.push('A');
        }
        let mut chars: Vec<u8> = slice.into_bytes();
        let tail = extra.as_bytes();
        for (j, b) in tail.iter().enumerate() {
            chars[7 - j] = *b;
        }
        let slice = String::from_utf8(chars).unwrap_or_else(|_| format!("{n:08}"));
        let display = format!("{prefix}{slice}");
        if !taken.contains(&display) {
            return slice;
        }
        n += 1;
    }
}

pub(crate) fn base62_sha256(key: &[u8]) -> String {
    let hash = Sha256::digest(key);
    let mut buf = hash.to_vec();
    let mut digits = Vec::new();
    while buf.iter().any(|&b| b != 0) {
        let mut rem = 0u16;
        for b in &mut buf {
            let v = (rem << 8) | u16::from(*b);
            *b = (v / 62) as u8;
            rem = v % 62;
        }
        digits.push(ALPH[rem as usize] as char);
    }
    if digits.is_empty() {
        digits.push(ALPH[0] as char);
    }
    digits.reverse();
    digits.into_iter().collect()
}

pub fn existing_path(p: PathBuf) -> Option<PathBuf> {
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}
