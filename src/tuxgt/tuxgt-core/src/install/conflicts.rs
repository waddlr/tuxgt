use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::{read_manifest, Error, FileManifest, Result};

/// Read a manifest or fail with the standard no-manifest error.
pub fn need_manifest(data_dir: &Path, game: &str, instance: &str) -> Result<FileManifest> {
    read_manifest(data_dir, game, instance)?
        .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))
}

/// Dests of other manifests still claiming game-dir files (for conflict
/// notes). Not an error condition: last writer wins, backups keep originals.
pub fn other_claims(data_dir: &Path, game_id: &str, instance: &str) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for m in crate::game_manifests(data_dir, game_id)? {
        if m.instance == instance || !m.enabled || m.adapter != "install" {
            continue;
        }
        out.extend(m.files.iter().map(|f| f.dest.clone()));
    }
    Ok(out)
}

/// One contested dest: every enabled manifest claiming `(adapter, dest)`,
/// in load order (winner last). Preload and install roots never collide,
/// so cross-adapter same strings are never grouped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadConflict {
    pub dest: String,
    pub adapter: String,
    pub instances: Box<[String]>,
}

/// All same-dest conflicts for one game, in deterministic group order.
/// Enabled manifests and enabled (`kept`) dests only: disabled manifests
/// and omitted dests contribute nothing at runtime and are excluded.
pub fn load_conflicts(data_dir: &Path, game_id: &str) -> Result<Vec<LoadConflict>> {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for m in crate::game_manifests(data_dir, game_id)? {
        if !m.enabled {
            continue;
        }
        for f in m.files.iter().filter(|f| f.enabled) {
            groups
                .entry((m.adapter.clone(), f.dest.clone()))
                .or_default()
                .push(m.instance.clone());
        }
    }
    Ok(groups
        .into_iter()
        .filter(|(_, instances)| instances.iter().collect::<BTreeSet<_>>().len() >= 2)
        .map(|((adapter, dest), instances)| LoadConflict {
            dest,
            adapter,
            instances: instances.into_boxed_slice(),
        })
        .collect())
}
