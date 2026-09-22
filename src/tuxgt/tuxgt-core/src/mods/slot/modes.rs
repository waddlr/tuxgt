//! Slot token each adapter last used for one instance.
//!
//! A conversion writes both modes before it renames anything. The snapshot
//! that rolls a failed convert back includes this file, so the mode the game
//! is still on and the mode it was heading toward both survive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::choice::{is_self_slot, SELF_SLOT};
use super::claiming_slot_index;
use crate::{parse_slot, Error, Result};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ModeFile {
    #[serde(default)]
    preload: String,
    #[serde(default)]
    install: String,
}

fn safe_instance(instance: &str) -> String {
    instance.replace([':', '/'], "_")
}

pub(crate) fn mode_path(data_dir: &Path, game: &str, instance: &str) -> Result<PathBuf> {
    let id = crate::game::GameId::parse(game)?;
    Ok(crate::game::game_dir(data_dir, &id)
        .join("slot-modes")
        .join(format!("{}.toml", safe_instance(instance))))
}

/// `<self>` or a proxy stem. A filename is not a token.
pub(crate) fn canonical_token(slot: &str) -> Result<String> {
    if is_self_slot(slot) {
        return Ok(SELF_SLOT.to_string());
    }
    Ok(parse_slot(slot)?.as_str().to_string())
}

/// Live dest → the token that would place it again.
pub(crate) fn token_for_dest(dest: &str) -> String {
    let base = dest.rsplit(['/', '\\']).next().unwrap_or(dest);
    parse_slot(base)
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|_| SELF_SLOT.to_string())
}

fn read_modes(data_dir: &Path, game: &str, instance: &str) -> Result<ModeFile> {
    let path = mode_path(data_dir, game, instance)?;
    if !path.is_file() {
        return Ok(ModeFile::default());
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).map_err(|e| Error::Manifest(e.to_string()))
}

fn write_modes(data_dir: &Path, game: &str, instance: &str, modes: &ModeFile) -> Result<()> {
    let path = mode_path(data_dir, game, instance)?;
    let text = toml::to_string(modes).map_err(|e| Error::Manifest(e.to_string()))?;
    crate::fs::atomic_write(&path, text.as_bytes())
}

fn set_key(modes: &mut ModeFile, adapter: &str, token: &str) -> Result<()> {
    let token = canonical_token(token)?;
    if crate::is_preload(adapter) {
        modes.preload = token;
        Ok(())
    } else if crate::is_install(adapter) {
        modes.install = token;
        Ok(())
    } else {
        Err(Error::InvalidInstance(format!(
            "unknown adapter: {adapter}"
        )))
    }
}

/// Token saved for `adapter`, when this instance has one.
pub fn remembered_slot(
    data_dir: &Path,
    game: &str,
    instance: &str,
    adapter: &str,
) -> Option<String> {
    let modes = read_modes(data_dir, game, instance).ok()?;
    let token = if crate::is_preload(adapter) {
        modes.preload
    } else if crate::is_install(adapter) {
        modes.install
    } else {
        return None;
    };
    (!token.is_empty()).then_some(token)
}

/// Record the token the instance is actually on.
pub(crate) fn remember_live(
    data_dir: &Path,
    game: &str,
    instance: &str,
    adapter: &str,
    slot: &str,
) -> Result<()> {
    let mut modes = read_modes(data_dir, game, instance)?;
    set_key(&mut modes, adapter, slot)?;
    write_modes(data_dir, game, instance, &modes)
}

/// Save the live dest under `source_adapter` and each pick under `target_adapter`.
/// Dest files are not touched.
pub(crate) fn remember_conversion(
    data_dir: &Path,
    game: &str,
    source_adapter: &str,
    target_adapter: &str,
    picks: &[(&str, &str)],
) -> Result<()> {
    let manifests = crate::game_manifests(data_dir, game)?;
    for (inst, pick) in picks {
        let m = manifests
            .iter()
            .find(|m| m.instance == *inst)
            .ok_or_else(|| Error::NoManifest(format!("missing {inst}")))?;
        let idx = claiming_slot_index(&m.files, &m.include).ok_or_else(|| {
            Error::InvalidInstance(format!("{inst}: no proxy slot dest to rewrite"))
        })?;
        let source = token_for_dest(&m.files[idx].dest);
        let mut modes = read_modes(data_dir, game, inst)?;
        set_key(&mut modes, source_adapter, &source)?;
        set_key(&mut modes, target_adapter, pick)?;
        write_modes(data_dir, game, inst, &modes)?;
    }
    Ok(())
}

pub(crate) fn forget_instance(data_dir: &Path, game: &str, instance: &str) -> Result<()> {
    let path = mode_path(data_dir, game, instance)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_keep_the_closed_set() {
        assert_eq!(canonical_token("<self>").unwrap(), "<self>");
        assert_eq!(canonical_token("DXGI.dll").unwrap(), "dxgi");
        assert!(canonical_token("ReShade64.dll").is_err());
        assert_eq!(token_for_dest("ReShade64.dll"), "<self>");
        assert_eq!(token_for_dest("d3d11.dll"), "d3d11");
    }
}
