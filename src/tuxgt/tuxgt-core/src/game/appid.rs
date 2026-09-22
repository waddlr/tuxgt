use sqlx::SqlitePool;

use super::*;
use crate::{Error, Result};

/// E43: user Steam-AppID overlay (ProtonDB / SteamGridDB on Heroic/manual
/// rows). `None`, empty, or whitespace-only clears; otherwise the trimmed
/// value must be 1-10 ASCII digits. Never touched by the scan upsert, so it
/// survives rescan (like `override_hidden`).
pub async fn set_steam_appid(pool: &SqlitePool, id: &str, appid: Option<&str>) -> Result<()> {
    GameId::parse(id)?;
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
    let value = match appid.map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(v) if v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()) => Some(v),
        Some(v) => return Err(Error::InvalidOverride(format!("bad steam appid: {v}"))),
    };
    sqlx::query("UPDATE games SET steam_appid = ? WHERE id = ?")
        .bind(value)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// E43: stored Steam-AppID overlay, `None` when unset. Unknown id errors.
pub async fn steam_appid_of(pool: &SqlitePool, id: &str) -> Result<Option<String>> {
    GameId::parse(id)?;
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT steam_appid FROM games WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    match row {
        None => Err(Error::UnknownGame(id.into())),
        Some((v,)) => Ok(v.filter(|s| !s.is_empty())),
    }
}
