//! Whether stored launch arguments need a channel, evaluated the same
//! way for the live working copy and for a parked profile slot.

use std::path::Path;

use sqlx::SqlitePool;

use super::catalog::{catalog_root, load_catalog, matching_args, GameIdentity};
use super::compose::compose;
use super::store::{self, StoredArg};
use super::suppress::suppressed_for;
use crate::game::game_row_by_id;
use crate::{Error, Result};

pub async fn args_channel(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<bool> {
    let stored = store::stored_args(pool, game_id).await?;
    let custom = store::custom_text(pool, game_id).await?;
    args_channel_for(pool, data_dir, game_id, &stored, &custom).await
}

/// `args_channel` for a stored-args snapshot that is not the live
/// working copy (a parked profile slot). Same catalog/suppression
/// evaluation, so a profile-switch prediction cannot diverge from the
/// needs the incoming profile will actually have once live.
pub async fn args_channel_for(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    stored: &[StoredArg],
    custom: &str,
) -> Result<bool> {
    if stored.iter().all(|s| !s.enabled) && custom.trim().is_empty() {
        return Ok(false);
    }
    let row = game_row_by_id(pool, game_id)
        .await?
        .ok_or_else(|| Error::UnknownGame(game_id.into()))?;
    let suppressed = suppressed_for(&row, data_dir);
    let built = compose(
        &matching_args(
            &load_catalog(&catalog_root(data_dir)),
            &GameIdentity::from_row(&row),
        ),
        stored,
        custom,
        &suppressed,
        &[],
    )?;
    Ok(built.channel)
}
