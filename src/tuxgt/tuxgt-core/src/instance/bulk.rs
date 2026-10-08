use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Settings bulk-disable memory: mod id → game ids a Settings bulk
/// disable touched, so re-enabling the catalog Mod can offer to restore
/// exactly those pairs. Sibling of `mods.toml`, same dir.
pub(crate) fn bulk_disabled_path(config_dir: &Path) -> PathBuf {
    config_dir.join(super::BULK_DISABLED_TOML)
}

pub fn read_bulk_disabled(config_dir: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let path = bulk_disabled_path(config_dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))
}

pub(crate) fn write_bulk_disabled(
    config_dir: &Path,
    map: &BTreeMap<String, BTreeSet<String>>,
) -> Result<()> {
    fs::create_dir_all(config_dir)?;
    let text = toml::to_string(map).map_err(|e| Error::Toml(e.to_string()))?;
    fs::write(bulk_disabled_path(config_dir), text)?;
    Ok(())
}

/// Write `map` back, or drop the file when nothing is recorded. Only
/// touches disk when `changed`.
fn store_bulk_disabled(
    config_dir: &Path,
    map: &BTreeMap<String, BTreeSet<String>>,
    changed: bool,
) -> Result<()> {
    if !changed {
        return Ok(());
    }
    if map.is_empty() {
        let _ = fs::remove_file(bulk_disabled_path(config_dir));
        return Ok(());
    }
    write_bulk_disabled(config_dir, map)
}

/// Merge `games` into the mod's recorded bulk set.
pub fn record_bulk_disabled(
    config_dir: &Path,
    mod_id: &str,
    games: &BTreeSet<String>,
) -> Result<()> {
    let mut map = read_bulk_disabled(config_dir)?;
    let before = map.get(mod_id).cloned().unwrap_or_default();
    let mut after = before.clone();
    after.extend(games.iter().cloned());
    let changed = after != before;
    if changed {
        map.insert(mod_id.to_string(), after);
    }
    store_bulk_disabled(config_dir, &map, changed)
}

/// Drop `games` from the mod's recorded bulk set; an emptied mod drops
/// out entirely.
pub fn unrecord_bulk_games(
    config_dir: &Path,
    mod_id: &str,
    games: &BTreeSet<String>,
) -> Result<()> {
    let mut map = read_bulk_disabled(config_dir)?;
    let Some(mut after) = map.get(mod_id).cloned() else {
        return Ok(());
    };
    let before = after.clone();
    for g in games {
        after.remove(g);
    }
    if after == before {
        return Ok(());
    }
    if after.is_empty() {
        map.remove(mod_id);
    } else {
        map.insert(mod_id.to_string(), after);
    }
    store_bulk_disabled(config_dir, &map, true)
}

/// Drop the mod's recorded bulk set entirely.
pub fn clear_bulk_disabled(config_dir: &Path, mod_id: &str) -> Result<()> {
    let mut map = read_bulk_disabled(config_dir)?;
    if map.remove(mod_id).is_none() {
        return Ok(());
    }
    store_bulk_disabled(config_dir, &map, true)
}
