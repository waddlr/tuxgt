//! Per-game profiles. The live `games/<rel>/` tree is the active profile.
//! Inactive roots live in `profiles/<id>/`. Link, history, and one-shot are
//! later tasks and have no on-disk fields here.

mod arm;
mod copy;
mod files;
mod gate;
mod slot;
mod switch;

#[cfg(test)]
mod tests_copy;
#[cfg(test)]
mod tests_gate;
#[cfg(test)]
mod tests_migrate;
#[cfg(test)]
mod tests_support;
#[cfg(test)]
mod tests_switch;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{atomic_write, game_dir, game_exists, Error, GameId, Result};

pub use arm::rearm_remembered_launch;
pub use copy::{create_copy, create_empty};
pub use gate::{activate_default_for_write, ensure_profile_write};
pub use switch::{preview_switch, switch_profile, SwitchOutcome, SwitchPreview};

pub const UNMODDED_ID: &str = "unmodded";
pub const DEFAULT_ID: &str = "default";
pub const UNMODDED_NAME: &str = "Unmodded";
pub const DEFAULT_NAME: &str = "Default";
pub const ADD_TO_DEFAULT: &str = "Add this to Default?";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileKind {
    Unmodded,
    Default,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub kind: ProfileKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileSet {
    pub active: String,
    pub profiles: Vec<Profile>,
}

impl ProfileSet {
    pub fn active_profile(&self) -> Option<&Profile> {
        self.by_id(&self.active)
    }

    pub fn by_id(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn by_name(&self, name: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Serialize, Deserialize)]
struct IndexFile {
    active: String,
    #[serde(default)]
    profile: Vec<Profile>,
}

pub(crate) fn game_root_dir(data_dir: &Path, game_id: &str) -> Result<PathBuf> {
    let id = GameId::parse(game_id)?;
    Ok(game_dir(data_dir, &id))
}

pub(crate) fn index_path(gdir: &Path) -> PathBuf {
    gdir.join("profiles.toml")
}

pub(crate) fn journal_path(gdir: &Path) -> PathBuf {
    gdir.join("profile-switch.toml")
}

pub(crate) fn slot_dir(gdir: &Path, id: &str) -> Result<PathBuf> {
    check_id(id)?;
    Ok(gdir.join("profiles").join(id))
}

pub(crate) fn check_id(id: &str) -> Result<()> {
    let ok = id == UNMODDED_ID
        || id == DEFAULT_ID
        || (id.starts_with('p') && id.len() > 1 && id[1..].chars().all(|c| c.is_ascii_digit()));
    if ok {
        Ok(())
    } else {
        Err(Error::Manifest(format!("profile id: {id}")))
    }
}

pub async fn ensure_profiles(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<ProfileSet> {
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    let gdir = game_root_dir(data_dir, game_id)?;
    std::fs::create_dir_all(&gdir)?;
    switch::resume_if_needed(pool, data_dir, game_id).await?;
    if index_path(&gdir).is_file() {
        return load_index(&gdir);
    }
    migrate(pool, data_dir, game_id).await
}

async fn migrate(pool: &SqlitePool, data_dir: &Path, game_id: &str) -> Result<ProfileSet> {
    let args = crate::game_args::store::stored_args(pool, game_id).await?;
    let args_custom = crate::game_args::store::custom_text(pool, game_id).await?;
    let modded = !crate::game_manifests(data_dir, game_id)?.is_empty()
        || !crate::knob_rows(pool, game_id).await?.is_empty()
        || !crate::custom_env(pool, game_id).await?.is_empty()
        || !crate::game_wrappers(pool, game_id).await?.is_empty()
        || !args.is_empty()
        || !args_custom.trim().is_empty();
    let active = if modded { DEFAULT_ID } else { UNMODDED_ID };
    let set = ProfileSet {
        active: active.to_string(),
        profiles: vec![builtin_unmodded(), builtin_default()],
    };
    let gdir = game_root_dir(data_dir, game_id)?;
    save_index(&gdir, &set)?;
    if !modded {
        slot::write_empty_slot(&gdir, &builtin_default())?;
    }
    Ok(set)
}

pub(crate) fn builtin_unmodded() -> Profile {
    Profile {
        id: UNMODDED_ID.into(),
        name: UNMODDED_NAME.into(),
        kind: ProfileKind::Unmodded,
    }
}

pub(crate) fn builtin_default() -> Profile {
    Profile {
        id: DEFAULT_ID.into(),
        name: DEFAULT_NAME.into(),
        kind: ProfileKind::Default,
    }
}

pub(crate) fn load_index(gdir: &Path) -> Result<ProfileSet> {
    let text = std::fs::read_to_string(index_path(gdir))?;
    let file: IndexFile = toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?;
    if file.by_kind_missing() {
        return Err(Error::Manifest(
            "profiles.toml is missing a built-in".into(),
        ));
    }
    check_id(&file.active)?;
    if !file.profile.iter().any(|p| p.id == file.active) {
        return Err(Error::Manifest("profiles.toml active is unknown".into()));
    }
    Ok(ProfileSet {
        active: file.active,
        profiles: file.profile,
    })
}

impl IndexFile {
    fn by_kind_missing(&self) -> bool {
        !self.profile.iter().any(|p| p.kind == ProfileKind::Unmodded)
            || !self.profile.iter().any(|p| p.kind == ProfileKind::Default)
    }
}

pub(crate) fn save_index(gdir: &Path, set: &ProfileSet) -> Result<()> {
    let file = IndexFile {
        active: set.active.clone(),
        profile: set.profiles.clone(),
    };
    let text = toml::to_string(&file).map_err(|e| Error::Toml(e.to_string()))?;
    atomic_write(&index_path(gdir), text.as_bytes())
}

pub(crate) fn check_name(profiles: &[Profile], name: &str, ignore: Option<&str>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(Error::Manifest("profile name is empty or too long".into()));
    }
    if name.eq_ignore_ascii_case(UNMODDED_NAME) {
        return Err(Error::Manifest("Unmodded is a built-in profile".into()));
    }
    let taken = profiles
        .iter()
        .any(|p| Some(p.id.as_str()) != ignore && p.name.eq_ignore_ascii_case(name));
    if taken {
        return Err(Error::Manifest(format!("profile already exists: {name}")));
    }
    Ok(name.to_string())
}

pub(crate) fn next_user_id(profiles: &[Profile]) -> String {
    let mut n = 1u32;
    loop {
        let id = format!("p{n}");
        if !profiles.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

pub async fn rename_profile(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    name: &str,
    new_name: &str,
) -> Result<ProfileSet> {
    let mut set = ensure_profiles(pool, data_dir, game_id).await?;
    let profile = set
        .by_name(name)
        .cloned()
        .ok_or_else(|| Error::Manifest(format!("unknown profile: {name}")))?;
    if profile.kind == ProfileKind::Unmodded {
        return Err(Error::Manifest("Unmodded cannot be renamed".into()));
    }
    let new_name = check_name(&set.profiles, new_name, Some(&profile.id))?;
    if let Some(slot) = set.profiles.iter_mut().find(|p| p.id == profile.id) {
        slot.name = new_name.clone();
    }
    let gdir = game_root_dir(data_dir, game_id)?;
    save_index(&gdir, &set)?;
    let slot = slot_dir(&gdir, &profile.id)?;
    if slot.join("profile.toml").is_file() {
        let stored = Profile {
            id: profile.id,
            name: new_name,
            kind: profile.kind,
        };
        slot::write_profile_toml(&slot, &stored)?;
    }
    Ok(set)
}

pub struct DeleteOutcome {
    pub set: ProfileSet,
    pub restore_deferred: bool,
    pub rearm_deferred: bool,
}

pub async fn delete_profile(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
    name: &str,
    yes_files: bool,
    yes_restore: bool,
) -> Result<DeleteOutcome> {
    let set = ensure_profiles(pool, data_dir, game_id).await?;
    let profile = set
        .by_name(name)
        .cloned()
        .ok_or_else(|| Error::Manifest(format!("unknown profile: {name}")))?;
    if profile.kind != ProfileKind::User {
        return Err(Error::Manifest(
            "Unmodded and Default cannot be deleted".into(),
        ));
    }
    let (restore_deferred, rearm_deferred) = if set.active == profile.id {
        let out =
            switch_profile(pool, data_dir, game_id, DEFAULT_ID, yes_files, yes_restore).await?;
        (out.restore_deferred, out.rearm_deferred)
    } else {
        (false, false)
    };
    let gdir = game_root_dir(data_dir, game_id)?;
    let slot = slot_dir(&gdir, &profile.id)?;
    if slot.exists() {
        std::fs::remove_dir_all(&slot)?;
    }
    let mut set = load_index(&gdir)?;
    set.profiles.retain(|p| p.id != profile.id);
    save_index(&gdir, &set)?;
    Ok(DeleteOutcome {
        set,
        restore_deferred,
        rearm_deferred,
    })
}
