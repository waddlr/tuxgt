//! Byte snapshot of one slot write. Apply and convert share it so a failed
//! call puts the same files back without going through `set_instance_slot`
//! (that refuses a stem another enabled mod still holds, and it rewrites
//! mode memory).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::claiming_slot_index;
use super::modes::mode_path;
use crate::{Error, Result};

struct TreeSnap {
    root: PathBuf,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

pub(crate) struct Snap {
    adapter: String,
    ini_path: PathBuf,
    ini: Option<Vec<u8>>,
    files: BTreeMap<PathBuf, Option<Vec<u8>>>,
    trees: Vec<TreeSnap>,
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn restore_bytes(bytes: Option<&[u8]>, path: &Path) -> Result<()> {
    match bytes {
        Some(b) => crate::fs::atomic_write(path, b),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        },
    }
}

fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, base, out)?;
        } else {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            out.insert(rel, fs::read(&path)?);
        }
    }
    Ok(())
}

fn snap_tree(dir: &Path) -> Result<TreeSnap> {
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files)?;
    Ok(TreeSnap {
        root: dir.to_path_buf(),
        files,
    })
}

fn restore_tree(snap: &TreeSnap) -> Result<()> {
    let mut current = BTreeMap::new();
    walk(&snap.root, &snap.root, &mut current)?;
    for rel in current.keys() {
        if !snap.files.contains_key(rel) {
            restore_bytes(None, &snap.root.join(rel))?;
        }
    }
    for (rel, bytes) in &snap.files {
        restore_bytes(Some(bytes), &snap.root.join(rel))?;
    }
    Ok(())
}

/// Adapter column, managed ini, each pick's current game-dir claiming dest,
/// manifest, staging toml, mode sidecar, stage tree, and backups tree.
pub(crate) async fn capture(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    picks: &[(&str, &str)],
) -> Result<Snap> {
    let adapter = crate::game::game_adapter(pool, game).await?;
    let gid = crate::game::GameId::parse(game)?;
    let ini_path = crate::prewire::managed_ini(&crate::game::game_dir(data_dir, &gid));
    let manifests = crate::game_manifests(data_dir, game)?;
    let mut dests = Vec::new();
    let mut instances = Vec::new();
    for (inst, _) in picks {
        let m = manifests
            .iter()
            .find(|m| m.instance == *inst)
            .ok_or_else(|| Error::NoManifest(format!("missing {inst}")))?;
        let idx = claiming_slot_index(&m.files, &m.include).ok_or_else(|| {
            Error::InvalidInstance(format!("{inst}: no proxy slot dest to rewrite"))
        })?;
        dests.push(m.files[idx].dest.clone());
        instances.push((*inst).to_string());
    }
    let root = crate::game_root(pool, game).await?;
    let prefix = crate::install::prefix_for(pool, game, dests.iter().map(String::as_str)).await?;
    let mut files = BTreeMap::new();
    for dest in &dests {
        let path = crate::install::resolve_target(&root, prefix.as_deref(), dest)?;
        files.insert(path.clone(), read_optional(&path)?);
    }
    for inst in &instances {
        let manifest = crate::manifest_path(data_dir, game, inst);
        files.insert(manifest.clone(), read_optional(&manifest)?);
        let toml = crate::stage::staging_toml_path(data_dir, game, inst);
        files.insert(toml.clone(), read_optional(&toml)?);
        let modes = mode_path(data_dir, game, inst)?;
        files.insert(modes.clone(), read_optional(&modes)?);
    }
    let mut trees = Vec::new();
    for inst in &instances {
        trees.push(snap_tree(&crate::stage_dir(data_dir, game, inst))?);
    }
    trees.push(snap_tree(&crate::install::backups_dir(data_dir, game))?);
    Ok(Snap {
        adapter,
        ini_path: ini_path.clone(),
        ini: read_optional(&ini_path)?,
        files,
        trees,
    })
}

impl Snap {
    /// Game-dir paths a later write may create. Absent files are stored as
    /// missing so restore deletes them. Paths already captured are kept.
    pub(crate) async fn include_dests(
        &mut self,
        pool: &SqlitePool,
        game: &str,
        dests: &[String],
    ) -> Result<()> {
        if dests.is_empty() {
            return Ok(());
        }
        let root = crate::game_root(pool, game).await?;
        let prefix =
            crate::install::prefix_for(pool, game, dests.iter().map(String::as_str)).await?;
        for dest in dests {
            let path = crate::install::resolve_target(&root, prefix.as_deref(), dest)?;
            if !self.files.contains_key(&path) {
                self.files.insert(path.clone(), read_optional(&path)?);
            }
        }
        Ok(())
    }

    pub(crate) async fn restore(&self, pool: &SqlitePool, game: &str) {
        if let Err(e) = crate::game::restore_game_adapter(pool, game, &self.adapter).await {
            tracing::error!(game, error = %e, "slot snapshot adapter restore failed");
        }
        if let Err(e) = restore_bytes(self.ini.as_deref(), &self.ini_path) {
            tracing::error!(game, error = %e, "slot snapshot ini restore failed");
        }
        for tree in &self.trees {
            if let Err(e) = restore_tree(tree) {
                tracing::error!(game, error = %e, "slot snapshot tree restore failed");
            }
        }
        for (path, bytes) in &self.files {
            if let Err(e) = restore_bytes(bytes.as_deref(), path) {
                tracing::error!(game, path = %path.display(), error = %e, "slot snapshot file restore failed");
            }
        }
    }
}
