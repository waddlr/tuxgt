use sqlx::SqlitePool;

use super::*;
use crate::{Error, Result};

/// R37: the three adapter kinds. `preload` injects through the managed loader
/// ini, `install` copies the proxy payload into the game dir / prefix, and
/// `install-links` links it there from staging. Single vocabulary for every
/// adapter decision in the workspace, so a new kind cannot be half-accepted.
pub const ADAPTER_PRELOAD: &str = "preload";
pub const ADAPTER_INSTALL: &str = "install";
pub const ADAPTER_INSTALL_LINKS: &str = "install-links";

/// Validate one adapter value: trimmed, and exactly one of the three kinds.
/// Returns the canonical spelling. Callers validate before any write, so an
/// unknown value never reaches the database.
pub fn validate_adapter(adapter: &str) -> Result<&str> {
    match adapter.trim() {
        a @ (ADAPTER_PRELOAD | ADAPTER_INSTALL | ADAPTER_INSTALL_LINKS) => Ok(a),
        _ => Err(Error::InvalidInstance(format!(
            "unknown adapter: {} (preload|install|install-links)",
            adapter.trim()
        ))),
    }
}

/// `true` for either game-dir adapter. Every "is this game-dir state"
/// check funnels through this so a third kind cannot read as preload by
/// accident.
pub fn is_install(adapter: &str) -> bool {
    adapter == ADAPTER_INSTALL || adapter == ADAPTER_INSTALL_LINKS
}

/// `true` only for the staged-symlink materialization.
pub fn is_links(adapter: &str) -> bool {
    adapter == ADAPTER_INSTALL_LINKS
}

/// `true` for the preload adapter.
pub fn is_preload(adapter: &str) -> bool {
    adapter == ADAPTER_PRELOAD
}

/// R37: the persisted per-game Install adapter choice. Unknown id errors.
/// The column is `NOT NULL DEFAULT 'preload'`, so every migrated row reads
/// back a valid kind; a value the column cannot hold is an error, not a
/// silent fallback, because painting the wrong adapter would mis-report
/// what a launch does.
pub async fn game_adapter(pool: &SqlitePool, id: &str) -> Result<String> {
    GameId::parse(id)?;
    let row: Option<(String,)> = sqlx::query_as("SELECT adapter FROM games WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        None => Err(Error::UnknownGame(id.into())),
        Some((v,)) => Ok(validate_adapter(&v)?.to_string()),
    }
}

/// Effective graphics API: override, else detected. Empty override counts
/// as unset (like `GameRow`). `None` when the game is unknown or neither
/// is set. Never errors on a missing row: callers keep their own
/// `UnknownGame` precedence.
pub async fn game_api(pool: &SqlitePool, id: &str) -> Result<Option<String>> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT override_api, detected_api FROM games WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(ovr, det)| ovr.filter(|s| !s.is_empty()).or(det)))
}

/// Vulkan games are preload-only: no proxy DLL loads in a Vulkan game, so
/// either install adapter is refused before any write. Preload, unknown, and
/// empty APIs pass; `extra_apis` never gates.
pub async fn ensure_install_allowed(pool: &SqlitePool, id: &str, adapter: &str) -> Result<()> {
    if !is_install(adapter) {
        return Ok(());
    }
    if game_api(pool, id).await?.as_deref() == Some("vulkan") {
        return Err(Error::InvalidInstance(format!(
            "{id}: install adapter cannot load into vulkan games (no proxy slot); use preload"
        )));
    }
    Ok(())
}

/// R37: persist the per-game adapter choice. Validates first, so an unknown
/// value is rejected before any write and the stored choice is unchanged.
/// Never touched by the scan upsert, so a user choice survives rescan.
pub async fn set_game_adapter(pool: &SqlitePool, id: &str, adapter: &str) -> Result<()> {
    GameId::parse(id)?;
    let adapter = validate_adapter(adapter)?;
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
    ensure_install_allowed(pool, id, adapter).await?;
    sqlx::query("UPDATE games SET adapter = ? WHERE id = ?")
        .bind(adapter)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// R37: restore a previously persisted choice during conversion rollback.
/// Same validation as [`set_game_adapter`], but it does not require the
/// game to still exist: the rollback path only ever rewrites a value it
/// read from this column in the same operation.
pub(crate) async fn restore_game_adapter(pool: &SqlitePool, id: &str, adapter: &str) -> Result<()> {
    let adapter = validate_adapter(adapter)?;
    sqlx::query("UPDATE games SET adapter = ? WHERE id = ?")
        .bind(adapter)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
