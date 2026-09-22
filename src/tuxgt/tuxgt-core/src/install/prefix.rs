use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::stage::check_rel;
use crate::{Error, Result};

/// Dest stems that need confirm before a game-dir overwrite (APP.md §7.3
/// foreign-DLL rule). Compared lowercased, sans `.dll`.
pub(crate) const FOREIGN_STEMS: &[&str] = &[
    "dxgi", "d3d11", "d3d12", "dxvk", "d8", "d9", "enb", "specialk", "vkd3d", "winmm", "version",
];

/// Prefix-dest marker: `pfx:windows/system32/<file>` and
/// `pfx:windows/syswow64/<file>` only, resolved against the game's
/// Proton/Wine prefix (Wine `drive_c`, see [`prefix_drive_c`]). Literal ASCII prefix.
pub const PREFIX_DEST_PREFIX: &str = "pfx:";
pub(crate) const PREFIX_ROOTS: &[&str] = &["windows/system32/", "windows/syswow64/"];

/// True for a `pfx:` prefix dest (validated by [`prefix_rel`]).
pub fn is_prefix_dest(dest: &str) -> bool {
    dest.starts_with(PREFIX_DEST_PREFIX)
}

/// Validate a `pfx:` dest and return the prefix-relative rel
/// (`windows/system32/<file>`). Only the two roots above; the file part must
/// be non-empty ASCII without backslashes, `..`, empty segments, a second
/// `:`, or a trailing slash.
pub fn prefix_rel(dest: &str) -> Result<String> {
    let Some(rest) = dest.strip_prefix(PREFIX_DEST_PREFIX) else {
        return Err(Error::InvalidInstance(format!("bad prefix dest: {dest}")));
    };
    if !dest.is_ascii() || rest.is_empty() {
        return Err(Error::InvalidInstance(format!("bad prefix dest: {dest}")));
    }
    let Some(root) = PREFIX_ROOTS.iter().find(|r| rest.starts_with(**r)) else {
        return Err(Error::InvalidInstance(format!(
            "bad prefix dest (want pfx:windows/system32/<file> or pfx:windows/syswow64/<file>): {dest}"
        )));
    };
    let file = &rest[root.len()..];
    if file.is_empty()
        || file.contains('\\')
        || file.starts_with('/')
        || file.ends_with('/')
        || file.contains("//")
        || file.contains(':')
    {
        return Err(Error::InvalidInstance(format!("bad prefix dest: {dest}")));
    }
    let p = Path::new(file);
    if p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(Error::InvalidInstance(format!("bad prefix dest: {dest}")));
    }
    Ok(rest.to_string())
}

/// Validate every `pfx:` dest in one install set: shape, forbidden proxy
/// stems (existing `FOREIGN_STEMS` incl. `dxgi`/`d3d11`/`d3d12`), and the
/// install-only rule (never preload). Non-`pfx:` dests are untouched.
pub fn validate_prefix_dests<'a>(
    dests: impl Iterator<Item = &'a str>,
    adapter: &str,
) -> Result<()> {
    for dest in dests {
        if !is_prefix_dest(dest) {
            continue;
        }
        let _ = prefix_rel(dest)?;
        if foreign_dest(dest) {
            return Err(Error::InvalidInstance(format!(
                "forbidden prefix dest (proxy stem): {dest}"
            )));
        }
        if adapter != "install" {
            return Err(Error::InvalidInstance(format!(
                "prefix dest needs the install adapter, not {adapter}: {dest}"
            )));
        }
    }
    Ok(())
}

/// Per-game Proton/Wine prefix: override, else detected, else store. Empty or
/// unknown game is an install error — native games cannot take `pfx:` dests.
/// Callers resolve `drive_c` via [`prefix_drive_c`]; never the Proton tree.
pub async fn prefix_root(pool: &SqlitePool, game_id: &str) -> Result<PathBuf> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT override_prefix_path, detected_prefix_path, prefix_path FROM games WHERE id = ?",
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?;
    let (ovr, det, store) = row.ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    let prefix = ovr
        .or(det)
        .or(store)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    match prefix {
        Some(p) => Ok(PathBuf::from(p)),
        None => Err(Error::Install(format!(
            "{game_id}: pfx: dest needs a Proton/Wine prefix (native game)"
        ))),
    }
}

/// Resolve the prefix when any of `dests` is a `pfx:` dest, else `None` so
/// native games without a prefix keep working for game-dir-only installs.
/// A `pfx:` dest with no configured prefix is an install error.
pub async fn prefix_for<'a>(
    pool: &SqlitePool,
    game_id: &str,
    dests: impl Iterator<Item = &'a str>,
) -> Result<Option<PathBuf>> {
    if dests.into_iter().any(is_prefix_dest) {
        Ok(Some(prefix_root(pool, game_id).await?))
    } else {
        Ok(None)
    }
}

pub fn foreign_dest(dest: &str) -> bool {
    let stem = Path::new(dest)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    FOREIGN_STEMS.contains(&stem.as_str())
}

pub fn backups_dir(data_dir: &Path, game_id: &str) -> PathBuf {
    match crate::game::GameId::parse(game_id) {
        Ok(id) => crate::game::game_dir(data_dir, &id).join("backups"),
        Err(_) => data_dir
            .join("backups")
            .join(crate::stage::game_safe(game_id)),
    }
}
pub(crate) fn backup_rel(dest: &str, ts: u64, hash8: &str) -> String {
    format!("{}.{ts}.{hash8}", dest.replace(['/', '\\'], "_"))
}

/// Wine `drive_c` dir for a stored prefix. Steam stores
/// `…/steamapps/compatdata/<appid>` (see `detect::ingest_compat`), whose Wine
/// root is one level down (`pfx/`); WINEPREFIX-shaped prefixes (Heroic
/// `winePrefix`, or a path already ending in `pfx`) hold `drive_c/` directly.
pub fn prefix_drive_c(prefix: &Path) -> PathBuf {
    let file_name = prefix.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let parent_name = prefix
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if file_name != "pfx" && parent_name == "compatdata" {
        prefix.join("pfx").join("drive_c")
    } else {
        prefix.join("drive_c")
    }
}

/// Resolve one dest to its on-disk target: `pfx:` dests go under the Wine
/// `drive_c` (see [`prefix_drive_c`]), everything else under the game root.
/// A `pfx:` dest with no prefix is an install error (native game).
pub(crate) fn resolve_target(root: &Path, prefix: Option<&Path>, dest: &str) -> Result<PathBuf> {
    if is_prefix_dest(dest) {
        let rel = prefix_rel(dest)?;
        match prefix {
            Some(pfx) => Ok(prefix_drive_c(pfx).join(rel)),
            None => Err(Error::Install(format!(
                "{dest}: pfx: dest needs a Proton/Wine prefix (native game)"
            ))),
        }
    } else {
        check_rel(dest)?;
        Ok(root.join(dest))
    }
}
