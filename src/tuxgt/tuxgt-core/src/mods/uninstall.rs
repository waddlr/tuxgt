use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::install::remove_with_promotion;
use crate::stage::remove_staging;
use crate::{foreign_occupied, prewire_game, read_manifest, Error, FileManifest, Result};

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
        let foreign = foreign_dests(data_dir, game, &m, &roots)?;
        if !foreign.is_empty() && !yes {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests left in place: {}",
                foreign.join(", ")
            )));
        }
        let excluded = m
            .files
            .iter()
            .map(|f| (m.instance.clone(), f.dest.clone()))
            .collect();
        let dests = m.files.iter().map(|f| f.dest.clone()).collect::<Vec<_>>();
        let outcome = remove_with_promotion(
            data_dir,
            game,
            m.clone(),
            &excluded,
            &mut m.backups,
            &dests,
            &roots.roots,
            roots.prefix.as_deref(),
        )?;
        restored = outcome.removal.restored.into_iter().collect();
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

/// Resolved install-adapter roots for one game: live game dir (and any
/// legacy `install_dir` package root), Wine prefix, and enabled game-dir
/// claims. Computed once and shared by the foreign pre-check and the
/// claim removal below.
pub(in crate::mods) struct InstallRoots {
    pub roots: Vec<PathBuf>,
    pub prefix: Option<PathBuf>,
}

/// Resolve install-adapter roots, or `None` for preload manifests
/// (no game-dir outs). Shared by `uninstall_instance` and the Settings
/// scope scan.
pub(in crate::mods) async fn install_roots(
    pool: &SqlitePool,
    _data_dir: &Path,
    game: &str,
    m: &FileManifest,
) -> Result<Option<InstallRoots>> {
    if !crate::is_install(&m.adapter) {
        return Ok(None);
    }
    let roots = crate::game_roots(pool, game).await?;
    let prefix =
        crate::install::prefix_for(pool, game, m.files.iter().map(|f| f.dest.as_str())).await?;
    Ok(Some(InstallRoots { roots, prefix }))
}

/// Foreign game-dir dests an uninstall would leave in place, given
/// resolved roots. Generated-dest claims never count as foreign, so the
/// dialog warns about exactly what the run would confirm. Checks every
/// game-dir root (live + legacy package root). The departing instance is
/// forced live in the claim map so an already-disabled uninstall still
/// owns its leftovers instead of treating them as foreign.
pub(in crate::mods) fn foreign_dests(
    data_dir: &Path,
    game: &str,
    m: &FileManifest,
    roots: &InstallRoots,
) -> Result<Vec<String>> {
    let mut manifests = crate::game_manifests(data_dir, game)?;
    if let Some(slot) = manifests
        .iter_mut()
        .find(|manifest| manifest.instance == m.instance)
    {
        *slot = m.clone();
        slot.enabled = true;
        for file in slot.files.iter_mut() {
            file.enabled = true;
        }
    } else {
        let mut live = m.clone();
        live.enabled = true;
        for file in live.files.iter_mut() {
            file.enabled = true;
        }
        manifests.push(live);
    }
    let claims = crate::install::claim_inventory_from_manifests(data_dir, game, &manifests)?;
    let mut out = std::collections::BTreeSet::new();
    for root in &roots.roots {
        for rel in foreign_occupied(
            m.files.iter().map(|f| f.dest.as_str()),
            root,
            roots.prefix.as_deref(),
            &claims,
        )? {
            if !super::park::claims_generated(m, &rel) {
                out.insert(rel);
            }
        }
    }
    Ok(out.into_iter().collect())
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
