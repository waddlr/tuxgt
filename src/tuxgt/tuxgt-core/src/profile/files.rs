//! Game-dir and prefix half of a switch. Reuses `remove_copies` / `apply_copies`.
//! Does not stop the store client.

use std::collections::BTreeMap;
use std::path::Path;

use sqlx::SqlitePool;

use crate::install::{claim_inventory, InstallMaterial};
use crate::{
    apply_copies, game_manifests, game_root, game_roots, is_install, plan_copies, prefix_for,
    remove_copies, write_manifest, FileManifest, LaunchNeeds, Result,
};

use super::slot::{self, KnobFile};

pub(crate) fn changed_dests(outgoing: &[FileManifest], incoming: &[FileManifest]) -> Vec<String> {
    let left = dest_map(outgoing);
    let right = dest_map(incoming);
    let mut names = Vec::new();
    for (dest, sha) in &left {
        if right.get(dest) != Some(sha) {
            names.push(dest.clone());
        }
    }
    for dest in right.keys() {
        if !left.contains_key(dest) {
            names.push(dest.clone());
        }
    }
    names.sort();
    names.dedup();
    names
}

fn dest_map(manifests: &[FileManifest]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for manifest in manifests {
        if !manifest.enabled || !is_install(&manifest.adapter) {
            continue;
        }
        for file in manifest.files.iter().filter(|file| file.enabled) {
            out.insert(file.dest.clone(), file.sha256.clone());
        }
    }
    out
}

pub(crate) fn channel_needed(manifests: &[FileManifest], knobs: &KnobFile) -> bool {
    let has_env = knobs
        .knobs
        .iter()
        .any(|row| row.enabled && !row.value.is_empty())
        || !knobs.custom.is_empty();
    let mut needs = LaunchNeeds::from_manifests(manifests, has_env, &knobs.wrappers);
    if knobs.args.iter().any(|row| row.enabled) || !knobs.args_custom.trim().is_empty() {
        needs.args = true;
        needs.install_only = false;
    }
    needs.channel_needed()
}

pub(crate) async fn remove_outgoing(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    let manifests = game_manifests(data_dir, game_id)?;
    let install: Vec<FileManifest> = manifests
        .iter()
        .filter(|m| m.enabled && is_install(&m.adapter))
        .cloned()
        .collect();
    if install.is_empty() {
        return Ok(());
    }
    let roots = game_roots(pool, game_id).await?;
    let dests: Vec<String> = install
        .iter()
        .flat_map(|m| m.files.iter().filter(|f| f.enabled).map(|f| f.dest.clone()))
        .collect();
    let prefix = prefix_for(pool, game_id, dests.iter().map(String::as_str)).await?;
    for manifest in install.iter().rev() {
        let mut backups = manifest.backups.clone();
        let owned: Vec<String> = manifest
            .files
            .iter()
            .filter(|f| f.enabled)
            .map(|f| f.dest.clone())
            .collect();
        let claims = claim_inventory(data_dir, game_id)?;
        for root in &roots {
            remove_copies(
                data_dir,
                &mut backups,
                owned.iter().map(String::as_str),
                game_id,
                &manifest.instance,
                &manifest.adapter,
                &manifest.files,
                root,
                prefix.as_deref(),
                &claims,
            )?;
        }
        let mut written = manifest.clone();
        written.backups = backups;
        write_manifest(data_dir, &written)?;
    }
    Ok(())
}

pub(crate) async fn apply_incoming(
    pool: &SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    let manifests = game_manifests(data_dir, game_id)?;
    let install: Vec<FileManifest> = manifests
        .into_iter()
        .filter(|m| m.enabled && is_install(&m.adapter))
        .collect();
    if install.is_empty() {
        return Ok(());
    }
    let root = game_root(pool, game_id).await?;
    for mut manifest in install {
        let prefix = prefix_for(
            pool,
            game_id,
            manifest.files.iter().map(|f| f.dest.as_str()),
        )
        .await?;
        let claims = claim_inventory(data_dir, game_id)?;
        let ops = plan_copies(
            data_dir,
            game_id,
            &manifest.instance,
            manifest.files.iter().filter(|f| f.enabled),
            &root,
            prefix.as_deref(),
            InstallMaterial::of(&manifest.adapter),
            &claims,
        )?;
        apply_copies(
            data_dir,
            game_id,
            &mut manifest,
            &root,
            prefix.as_deref(),
            &ops,
            &claims,
        )?;
    }
    Ok(())
}

pub(crate) fn incoming_view(
    gdir: &Path,
    active: &str,
    target: &str,
    live_manifests: &[FileManifest],
    live_knobs: &KnobFile,
) -> Result<(Vec<FileManifest>, KnobFile)> {
    if target == active {
        return Ok((live_manifests.to_vec(), live_knobs.clone()));
    }
    let slot = super::slot_dir(gdir, target)?;
    if !slot.exists() {
        return Ok((Vec::new(), KnobFile::default()));
    }
    Ok((
        slot::read_slot_manifests(gdir, target)?,
        slot::read_knobs(&slot)?,
    ))
}
