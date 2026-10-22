//! First edit while Unmodded is active moves onto Default. Empty Default
//! switches with no confirm. A Default that already has changes asks first
//! and writes nothing until the caller activates it.

use std::path::Path;

use sqlx::SqlitePool;

use crate::{Error, Result};

use super::slot;
use super::{
    ensure_profiles, game_root_dir, switch_profile, SwitchOutcome, ADD_TO_DEFAULT, DEFAULT_ID,
    UNMODDED_ID,
};

pub async fn ensure_profile_write(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<()> {
    let set = ensure_profiles(pool, data_dir, game_id).await?;
    if set.active != UNMODDED_ID {
        return Ok(());
    }
    let gdir = game_root_dir(data_dir, game_id)?;
    let slot = super::slot_dir(&gdir, DEFAULT_ID)?;
    if slot::slot_is_empty(&slot)? {
        switch_profile(pool, data_dir, game_id, DEFAULT_ID, true, true).await?;
        return Ok(());
    }
    Err(Error::NeedProfile(ADD_TO_DEFAULT.into()))
}

pub async fn activate_default_for_write(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<SwitchOutcome> {
    switch_profile(pool, data_dir, game_id, DEFAULT_ID, true, true).await
}
