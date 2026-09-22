use std::fs;
use std::path::Path;

use sqlx::SqlitePool;

use crate::stage::remove_staging;
use crate::{
    foreign_occupied, prewire_game, read_manifest, remove_copies, tracked_dests, Error,
    FileManifest, Result,
};

pub async fn uninstall_instance(
    pool: &SqlitePool,
    data_dir: &Path,
    _config_dir: &Path,
    game: &str,
    instance: &str,
    yes: bool,
) -> Result<()> {
    tracing::debug!(game, instance, "uninstall entry");
    let mut m = crate::need_manifest(data_dir, game, instance)?;
    let mut restored = std::collections::BTreeSet::new();
    if crate::is_install(&m.adapter) {
        let root = crate::game_root(pool, game).await?;
        let prefix =
            crate::install::prefix_for(pool, game, m.files.iter().map(|f| f.dest.as_str())).await?;
        let tracked = tracked_dests(data_dir, game)?;
        let foreign: Vec<String> = foreign_occupied(
            m.files.iter().map(|f| f.dest.as_str()),
            &root,
            prefix.as_deref(),
            &tracked,
        )?
        .into_iter()
        .filter(|rel| !super::park::claims_generated(&m, rel))
        .collect();
        if !foreign.is_empty() && !yes {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests left in place: {}",
                foreign.join(", ")
            )));
        }
        let dests = m.files.iter().map(|f| f.dest.as_str());
        let removal = remove_copies(
            data_dir,
            &mut m.backups,
            dests,
            &root,
            prefix.as_deref(),
            &tracked,
        )?;
        restored = removal.restored.into_iter().collect();
    }
    super::park::drop_disabled(pool, data_dir, game, instance, &restored).await?;
    let keep: std::collections::BTreeSet<String> = crate::game_manifests(data_dir, game)?
        .into_iter()
        .filter(|o| o.instance != instance && o.enabled)
        .flat_map(|o| {
            o.files
                .into_vec()
                .into_iter()
                .filter(|f| f.enabled)
                .map(|f| f.dest)
        })
        .collect();
    crate::stage::remove_runtime_dests(
        data_dir,
        game,
        m.files.iter().map(|f| f.dest.as_str()),
        &keep,
    )?;
    let _ = fs::remove_dir(crate::stage::runtime_dir(data_dir, game));
    remove_staging(data_dir, game, instance)?;
    let path = crate::manifest_path(data_dir, game, instance);
    if path.is_file() {
        fs::remove_file(&path)?;
    }
    super::slot::forget_instance(data_dir, game, instance)?;
    prewire_game(data_dir, pool, game).await?;
    tracing::info!(game, instance, "uninstalled");
    crate::db::mark_cache_dirty();
    Ok(())
}

pub fn enabled_mod_count(data_dir: &Path, game: &str) -> usize {
    crate::game_manifests(data_dir, game)
        .ok()
        .map(|ms| ms.iter().filter(|m| m.enabled).count())
        .unwrap_or(0)
}

pub fn has_apply_record(data_dir: &Path, game: &str) -> bool {
    crate::apply::read_record(data_dir, game)
        .ok()
        .flatten()
        .is_some()
}

pub fn read_manifest_opt(data_dir: &Path, game: &str, instance: &str) -> Option<FileManifest> {
    read_manifest(data_dir, game, instance).ok().flatten()
}
