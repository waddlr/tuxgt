use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::*;
use crate::game::{game_dir, game_exists, GameId};
use crate::{Error, PluginHost, Result};

pub(crate) const INJECT_ON: i64 = 1;
pub(crate) const CONF_NAME: &str = "tux-protonfixes.conf";
pub(crate) const CORRELATOR_NAME: &str = "load-correlator.ini";

pub(crate) fn protonfixes_conf(data_dir: &Path, id: &GameId) -> PathBuf {
    game_dir(data_dir, id).join(CONF_NAME)
}

pub(crate) fn correlator_path(data_dir: &Path) -> PathBuf {
    data_dir.join("games").join(CORRELATOR_NAME)
}

/// R55: canonical correlator key. Proton may report `C:\Games\SkyrimSE.EXE`
/// while the index stores `c:/games/skyrimse.exe`, so separators fold
/// (`\` → `/`), case folds, then surrounding whitespace and trailing
/// slashes trim. Runs on write (primaries and extras alike) AND on lookup
/// (hook `_norm`, trampoline `norm_path`): the old trim-only `norm_path`
/// is gone, every correlator key flows through here.
pub fn canonical_exe_key(s: &str) -> String {
    s.replace('\\', "/")
        .to_lowercase()
        .trim()
        .trim_end_matches('/')
        .trim()
        .to_string()
}

pub async fn migrate_handle(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_handle (
            game_id TEXT PRIMARY KEY NOT NULL,
            inject INTEGER NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn game_handle(pool: &SqlitePool, game_id: &str) -> Result<bool> {
    GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    let row: Option<(i64,)> = sqlx::query_as("SELECT inject FROM game_handle WHERE game_id = ?")
        .bind(game_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some_and(|r| r.0 != 0))
}

pub async fn set_handle(
    pool: &SqlitePool,
    data_dir: &Path,
    host: &PluginHost,
    game_id: &str,
    on: bool,
) -> Result<()> {
    GameId::parse(game_id)?;
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    // E80 exclusive arming: Hook and Apply never coexist. Handle-on while
    // applied restores the trampoline first, then arms `inject=1`.
    if on && crate::apply::read_record(data_dir, game_id)?.is_some() {
        crate::apply::restore_launch(data_dir, game_id)?;
    }
    mutate_game(
        pool,
        data_dir,
        host,
        game_id,
        write_handle(pool, game_id, on),
    )
    .await
}

pub(crate) async fn write_handle(pool: &SqlitePool, game_id: &str, on: bool) -> Result<()> {
    sqlx::query(
        "INSERT INTO game_handle (game_id, inject) VALUES (?, ?)
         ON CONFLICT(game_id) DO UPDATE SET inject = excluded.inject",
    )
    .bind(game_id)
    .bind(if on { INJECT_ON } else { 0 })
    .execute(pool)
    .await?;
    Ok(())
}

/// E94: an armed channel nothing needs is stale state — the last mod was
/// uninstalled, the last knob cleared, wrappers switched off. Drop it at the
/// one write every management mutation runs through, so the radio repaints
/// Not hooked instead of waiting for a click. Never routes through
/// `set_handle` (that calls back into `sync_session`). Best-effort by design:
/// a refused restore leaves both arms alone rather than half-disarming, and a
/// failure here must not block the session render the caller asked for.
pub(crate) async fn disarm_if_unneeded(pool: &SqlitePool, data_dir: &Path, game_id: &str) {
    let needs = match crate::launch::game_launch_needs(pool, data_dir, game_id).await {
        Ok(needs) => needs,
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "launch needs read failed");
            return;
        }
    };
    if needs.channel_needed() {
        return;
    }
    let applied = match crate::apply::read_record(data_dir, game_id) {
        Ok(record) => record.is_some(),
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "apply record read failed");
            return;
        }
    };
    let handled = match game_handle(pool, game_id).await {
        Ok(handled) => handled,
        Err(e) => {
            tracing::warn!(game = game_id, error = %e, "handle read failed");
            return;
        }
    };
    if !(applied || handled) {
        return;
    }
    // Trampoline first: if the store write is refused, `inject=1` still stands
    // and the pair cannot read as half-disarmed.
    if applied {
        if let Err(e) = crate::apply::restore_launch(data_dir, game_id) {
            tracing::warn!(game = game_id, error = %e, "auto-restore failed");
            return;
        }
    }
    if handled {
        if let Err(e) = write_handle(pool, game_id, false).await {
            tracing::warn!(game = game_id, error = %e, "handle clear failed");
        }
    }
}

pub(crate) fn find_so_for_arch(data_dir: &Path, arch: &str) -> Option<PathBuf> {
    let name = if arch == "32" {
        "lib/libtuxgt-launcher32.so"
    } else {
        "lib/libtuxgt-launcher.so"
    };
    let p = data_dir.join(name);
    if p.is_file() {
        Some(p)
    } else if arch == "32" {
        // Fallback to 64-bit when 32-bit build missing (dev without multilib); keep fallback + test with visible signal
        let fallback = data_dir.join("lib/libtuxgt-launcher.so");
        let found = fallback.is_file().then_some(fallback.clone());
        if found.is_some() {
            eprintln!("tuxgt: {} missing, falling back to {} for 32-bit game", p.display(), fallback.display());
        }
        found
    } else {
        None
    }
}

pub(crate) async fn find_so_for_game(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Option<PathBuf> {
    let arch = game_bitness(pool, game_id).await.unwrap_or_else(|| "64".into());
    find_so_for_arch(data_dir, &arch)
}

async fn game_bitness(pool: &SqlitePool, game_id: &str) -> Option<String> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT override_bitness, detected_bitness FROM games WHERE id = ?")
            .bind(game_id)
            .fetch_optional(pool)
            .await
            .ok()?;
    let (ob, db) = row?;
    Some(ob.or(db).unwrap_or_else(|| "64".into()))
}

pub(crate) fn render_session(pairs: &BTreeMap<String, String>) -> String {
    let mut out = String::from("# tuxgt launch session\n");
    for (k, v) in pairs {
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    out
}

pub(crate) fn write_session_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text.as_bytes())?;
    fs::rename(&tmp, path)?;
    Ok(())
}
