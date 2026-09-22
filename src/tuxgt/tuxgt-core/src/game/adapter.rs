use sqlx::SqlitePool;

use super::*;
use crate::{Error, Result};

/// R37: the two adapter kinds. `preload` injects through the managed loader
/// ini, `install` copies the proxy payload into the game dir / prefix.
/// Single vocabulary for every adapter decision in the workspace, so a new
/// kind cannot be half-accepted.
pub const ADAPTER_PRELOAD: &str = "preload";
pub const ADAPTER_INSTALL: &str = "install";

/// Validate one adapter value: trimmed, and exactly one of the two kinds.
/// Returns the canonical spelling. Callers validate before any write, so an
/// unknown value never reaches the database.
pub fn validate_adapter(adapter: &str) -> Result<&str> {
    match adapter.trim() {
        a @ (ADAPTER_PRELOAD | ADAPTER_INSTALL) => Ok(a),
        _ => Err(Error::InvalidInstance(format!(
            "unknown adapter: {} (preload|install)",
            adapter.trim()
        ))),
    }
}

/// `true` for the install adapter. Every "is this install-adapter state"
/// check funnels through this so a third kind cannot read as preload by
/// accident.
pub fn is_install(adapter: &str) -> bool {
    adapter == ADAPTER_INSTALL
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

/// R37: persist the per-game adapter choice. Validates first, so an unknown
/// value is rejected before any write and the stored choice is unchanged.
/// Never touched by the scan upsert, so a user choice survives rescan.
pub async fn set_game_adapter(pool: &SqlitePool, id: &str, adapter: &str) -> Result<()> {
    GameId::parse(id)?;
    let adapter = validate_adapter(adapter)?;
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
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
