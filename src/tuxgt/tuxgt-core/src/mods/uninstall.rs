use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    if let Some(roots) = install_roots(pool, data_dir, game, &m).await? {
        let foreign = foreign_dests(&m, &roots)?;
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
            &roots.root,
            roots.prefix.as_deref(),
            &roots.tracked,
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

/// Resolved install-adapter roots for one game: game dir, Wine
/// prefix, and tracked dest hashes. Computed once and shared by the
/// foreign pre-check and the copy removal below.
pub(in crate::mods) struct InstallRoots {
    pub root: PathBuf,
    pub prefix: Option<PathBuf>,
    pub tracked: BTreeMap<String, String>,
}

/// Resolve install-adapter roots, or `None` for preload manifests
/// (no game-dir outs). Shared by `uninstall_instance` and the Settings
/// scope scan.
pub(in crate::mods) async fn install_roots(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    m: &FileManifest,
) -> Result<Option<InstallRoots>> {
    if !crate::is_install(&m.adapter) {
        return Ok(None);
    }
    let root = crate::game_root(pool, game).await?;
    let prefix =
        crate::install::prefix_for(pool, game, m.files.iter().map(|f| f.dest.as_str())).await?;
    let tracked = tracked_dests(data_dir, game)?;
    Ok(Some(InstallRoots {
        root,
        prefix,
        tracked,
    }))
}

/// Foreign game-dir dests an uninstall would leave in place, given
/// resolved roots. Generated-dest claims never count as foreign, so the
/// dialog warns about exactly what the run would confirm.
pub(in crate::mods) fn foreign_dests(
    m: &FileManifest,
    roots: &InstallRoots,
) -> Result<Vec<String>> {
    Ok(foreign_occupied(
        m.files.iter().map(|f| f.dest.as_str()),
        &roots.root,
        roots.prefix.as_deref(),
        &roots.tracked,
    )?
    .into_iter()
    .filter(|rel| !super::park::claims_generated(m, rel))
    .collect())
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
