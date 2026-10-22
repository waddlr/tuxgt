//! New roots: empty, or a copy of one profile. A copy of the active profile
//! clones the live tree. A copy of an inactive profile clones its slot.

use std::path::Path;

use sqlx::SqlitePool;

use crate::{Error, Result};

use super::slot;
use super::{
    check_name, ensure_profiles, game_root_dir, next_user_id, save_index, Profile, ProfileKind,
};

pub async fn create_empty(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    name: &str,
) -> Result<Profile> {
    let mut set = ensure_profiles(pool, data_dir, game_id).await?;
    let name = check_name(&set.profiles, name, None)?;
    let profile = Profile {
        id: next_user_id(&set.profiles),
        name,
        kind: ProfileKind::User,
    };
    let gdir = game_root_dir(data_dir, game_id)?;
    slot::write_empty_slot(&gdir, &profile)?;
    set.profiles.push(profile.clone());
    save_index(&gdir, &set)?;
    Ok(profile)
}

pub async fn create_copy(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    name: &str,
    from: &str,
) -> Result<Profile> {
    let mut set = ensure_profiles(pool, data_dir, game_id).await?;
    let source = set
        .by_name(from)
        .or_else(|| set.by_id(from))
        .cloned()
        .ok_or_else(|| Error::Manifest(format!("unknown profile: {from}")))?;
    let name = check_name(&set.profiles, name, None)?;
    let profile = Profile {
        id: next_user_id(&set.profiles),
        name,
        kind: ProfileKind::User,
    };
    let gdir = game_root_dir(data_dir, game_id)?;
    if source.id == set.active {
        slot::park_live(pool, &gdir, game_id, &profile).await?;
    } else {
        let src = super::slot_dir(&gdir, &source.id)?;
        let dst = super::slot_dir(&gdir, &profile.id)?;
        if src.is_dir() {
            slot::copy_tree(&src, &dst)?;
            slot::write_profile_toml(&dst, &profile)?;
        } else {
            slot::write_empty_slot(&gdir, &profile)?;
        }
    }
    set.profiles.push(profile.clone());
    save_index(&gdir, &set)?;
    Ok(profile)
}
