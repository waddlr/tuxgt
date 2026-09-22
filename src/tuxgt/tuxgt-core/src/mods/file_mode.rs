use std::path::Path;

use sqlx::SqlitePool;

use super::keep::{over_list, ratchet_shrink};
use crate::{prewire_game, Error, FileManifest, Result};

/// Switch one applicable dest between LoadDLL (`load`) and IncludeFile.
/// This game only. Does not stage, copy, or change keep, required-ness, or
/// which dest claims the slot. The ini budget gate matches file keep.
pub async fn set_file_load(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    dest: &str,
    load: bool,
) -> Result<FileManifest> {
    let m = crate::need_manifest(data_dir, game, instance)?;
    let f = m
        .files
        .iter()
        .find(|f| f.dest == dest)
        .ok_or_else(|| Error::InvalidInstance(format!("unknown dest: {dest}")))?;
    if !crate::download::file_mode_applicable(dest) {
        return Err(Error::InvalidInstance(format!(
            "dest cannot switch LoadDLL/IncludeFile: {dest}"
        )));
    }
    if crate::download::file_is_loaddll(&f.dest, &m.include, f.load) == load {
        return Ok(m);
    }
    let mut prospective = m;
    if let Some(pf) = prospective.files.iter_mut().find(|f| f.dest == dest) {
        pf.load = crate::download::explicit_file_load(&pf.dest, &prospective.include, load);
    }
    let mut manifests = crate::game_manifests(data_dir, game)?;
    let current = crate::prewire::body_for(&manifests);
    if let Some(slot) = manifests.iter_mut().find(|o| o.instance == instance) {
        *slot = prospective.clone();
    }
    let new = crate::prewire::body_for(&manifests);
    let skip_prewire = if new == current {
        true
    } else {
        match crate::prewire::ensure_ini_budget(pool, data_dir, game, &prospective).await {
            Ok(()) => false,
            Err(e) => {
                if !ratchet_shrink(&current, &new) {
                    if let Some((key, len)) = over_list(&new) {
                        return Err(refuse_switch(game, instance, dest, key, len));
                    }
                    return Err(e);
                }
                true
            }
        }
    };
    crate::write_manifest(data_dir, &prospective)?;
    if !skip_prewire {
        prewire_game(data_dir, pool, game).await?;
    }
    Ok(prospective)
}

fn refuse_switch(game: &str, instance: &str, dest: &str, key: &str, len: usize) -> Error {
    let budget = crate::prewire::INI_LIST_BUDGET;
    Error::Manifest(format!(
        "{game}: cannot switch {dest} on {instance}: the {key} list would still exceed the {budget}-byte budget ({len} bytes) without shrinking; toggle dests off the over-budget list or uninstall the over-budget instance (`tuxgt instance uninstall {game} <instance>`) to recover"
    ))
}
