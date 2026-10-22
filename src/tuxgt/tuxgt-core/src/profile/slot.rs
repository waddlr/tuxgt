//! Park and load one profile root. The active profile is the live tree;
//! this module only moves bytes and the per-game sqlite rows.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::game_args::store::{self, Snapshot, StoredArg};
use crate::{atomic_write, Error, FileManifest, Result};

use super::{slot_dir, Profile};

const DIRS: &[&str] = &["manifests", "stage", "runtime", "disabled"];
const FILES: &[&str] = &["tuxgt-launcher.ini"];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct KnobFile {
    #[serde(default)]
    pub knobs: Vec<KnobSnap>,
    #[serde(default)]
    pub custom: Vec<CustomSnap>,
    #[serde(default)]
    pub wrappers: Vec<String>,
    /// Launch-argument working copy. `game_args_published` stays on the game.
    #[serde(default)]
    pub args: Vec<StoredArg>,
    #[serde(default)]
    pub args_custom: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct KnobSnap {
    pub knob: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CustomSnap {
    pub key: String,
    pub value: String,
}

pub(crate) fn write_profile_toml(slot: &Path, profile: &Profile) -> Result<()> {
    fs::create_dir_all(slot)?;
    let text = toml::to_string(profile).map_err(|e| Error::Toml(e.to_string()))?;
    atomic_write(&slot.join("profile.toml"), text.as_bytes())
}

pub(crate) fn write_empty_slot(gdir: &Path, profile: &Profile) -> Result<()> {
    let slot = slot_dir(gdir, &profile.id)?;
    if slot.exists() {
        wipe_payload(&slot)?;
    }
    write_profile_toml(&slot, profile)?;
    write_knobs(&slot, &KnobFile::default())
}

pub(crate) async fn read_live_knobs(pool: &SqlitePool, game_id: &str) -> Result<KnobFile> {
    let knobs = crate::knob_rows(pool, game_id)
        .await?
        .into_iter()
        .map(|r| KnobSnap {
            knob: r.knob,
            value: r.value,
            enabled: r.enabled,
        })
        .collect();
    let custom = crate::custom_env(pool, game_id)
        .await?
        .into_iter()
        .map(|(key, value)| CustomSnap { key, value })
        .collect();
    let wrappers = crate::game_wrappers(pool, game_id).await?;
    let args = store::stored_args(pool, game_id).await?;
    let args_custom = store::custom_text(pool, game_id).await?;
    Ok(KnobFile {
        knobs,
        custom,
        wrappers,
        args,
        args_custom,
    })
}

pub(crate) fn read_knobs(slot: &Path) -> Result<KnobFile> {
    let path = slot.join("knobs.toml");
    if !path.is_file() {
        return Ok(KnobFile::default());
    }
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))
}

fn write_knobs(slot: &Path, knobs: &KnobFile) -> Result<()> {
    fs::create_dir_all(slot)?;
    let text = toml::to_string(knobs).map_err(|e| Error::Toml(e.to_string()))?;
    atomic_write(&slot.join("knobs.toml"), text.as_bytes())
}

pub(crate) fn slot_is_empty(slot: &Path) -> Result<bool> {
    if !slot.exists() {
        return Ok(true);
    }
    if dir_has_files(&slot.join("manifests"))? {
        return Ok(false);
    }
    let knobs = read_knobs(slot)?;
    Ok(knobs.knobs.is_empty()
        && knobs.custom.is_empty()
        && knobs.wrappers.is_empty()
        && knobs.args.is_empty()
        && knobs.args_custom.trim().is_empty())
}

fn dir_has_files(dir: &Path) -> Result<bool> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for ent in fs::read_dir(dir)? {
        let ent = ent?;
        if ent.file_type()?.is_dir() {
            if dir_has_files(&ent.path())? {
                return Ok(true);
            }
        } else {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Copy the live profile into `id`'s slot. Live stays put.
pub(crate) async fn park_live(
    pool: &SqlitePool,
    gdir: &Path,
    game_id: &str,
    profile: &Profile,
) -> Result<()> {
    let slot = slot_dir(gdir, &profile.id)?;
    wipe_payload(&slot)?;
    for name in DIRS {
        copy_if_present(&gdir.join(name), &slot.join(name))?;
    }
    for name in FILES {
        copy_if_present(&gdir.join(name), &slot.join(name))?;
    }
    write_knobs(&slot, &read_live_knobs(pool, game_id).await?)?;
    write_profile_toml(&slot, profile)
}

/// Replace the live profile trees and sqlite rows from a slot.
/// A missing slot leaves the live trees empty.
pub(crate) async fn load_slot(
    pool: &SqlitePool,
    gdir: &Path,
    game_id: &str,
    id: &str,
) -> Result<()> {
    clear_live(gdir)?;
    let slot = slot_dir(gdir, id)?;
    if slot.is_dir() {
        for name in DIRS {
            copy_if_present(&slot.join(name), &gdir.join(name))?;
        }
        for name in FILES {
            copy_if_present(&slot.join(name), &gdir.join(name))?;
        }
    }
    let knobs = if slot.is_dir() {
        read_knobs(&slot)?
    } else {
        KnobFile::default()
    };
    replace_sqlite(pool, game_id, &knobs).await
}

pub(crate) fn clear_live(gdir: &Path) -> Result<()> {
    wipe_payload(gdir)
}

pub(crate) fn read_slot_manifests(gdir: &Path, id: &str) -> Result<Vec<FileManifest>> {
    let slot = slot_dir(gdir, id)?;
    read_manifests_in(&slot.join("manifests"))
}

pub(crate) fn read_manifests_in(dir: &Path) -> Result<Vec<FileManifest>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in fs::read_dir(dir)? {
        let path = ent?.path();
        if path.extension().is_none_or(|ext| ext != "toml") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let manifest: FileManifest =
            toml::from_str(&text).map_err(|e| Error::Manifest(e.to_string()))?;
        out.push(manifest);
    }
    out.sort_by(|a, b| {
        a.load_order
            .cmp(&b.load_order)
            .then_with(|| a.instance.cmp(&b.instance))
    });
    Ok(out)
}

pub(crate) fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    copy_if_present(src, dst)
}

fn wipe_payload(root: &Path) -> Result<()> {
    for name in DIRS {
        let path = root.join(name);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
    }
    for name in FILES {
        let path = root.join(name);
        if path.is_file() || path.is_symlink() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn copy_if_present(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() && fs::symlink_metadata(src).is_err() {
        return Ok(());
    }
    copy_path(src, dst)
}

fn copy_path(src: &Path, dst: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        if dst.symlink_metadata().is_ok() {
            fs::remove_file(dst)?;
        }
        std::os::unix::fs::symlink(fs::read_link(src)?, dst)?;
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for ent in fs::read_dir(src)? {
            let ent = ent?;
            copy_path(&ent.path(), &dst.join(ent.file_name()))?;
        }
        return Ok(());
    }
    if meta.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}

async fn replace_sqlite(pool: &SqlitePool, game_id: &str, knobs: &KnobFile) -> Result<()> {
    sqlx::query("DELETE FROM env_knobs WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM env_custom WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM game_wrappers WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    for row in &knobs.knobs {
        sqlx::query("INSERT INTO env_knobs (game_id, knob, value, enabled) VALUES (?, ?, ?, ?)")
            .bind(game_id)
            .bind(&row.knob)
            .bind(&row.value)
            .bind(i64::from(row.enabled))
            .execute(pool)
            .await?;
    }
    for row in &knobs.custom {
        sqlx::query("INSERT INTO env_custom (game_id, key, value) VALUES (?, ?, ?)")
            .bind(game_id)
            .bind(&row.key)
            .bind(&row.value)
            .execute(pool)
            .await?;
    }
    for wrapper in &knobs.wrappers {
        sqlx::query("INSERT INTO game_wrappers (game_id, wrapper) VALUES (?, ?)")
            .bind(game_id)
            .bind(wrapper)
            .execute(pool)
            .await?;
    }
    store::replace_working(
        pool,
        game_id,
        &Snapshot {
            args: knobs.args.clone(),
            custom: knobs.args_custom.clone(),
        },
    )
    .await?;
    Ok(())
}

pub(crate) fn refresh_parked_manifests(gdir: &Path, id: &str) -> Result<()> {
    let slot = slot_dir(gdir, id)?;
    let dest = slot.join("manifests");
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_if_present(&gdir.join("manifests"), &dest)
}
