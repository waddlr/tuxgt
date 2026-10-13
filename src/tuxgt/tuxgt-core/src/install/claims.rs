use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::stage::stage_dir;
use crate::Result;

/// One enabled, kept claim on a shared game-dir root. Copy and Links use the
/// same root, so both Install kinds appear in one group; preload manifests
/// stage through the managed loader and never claim a game-dir dest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestClaim {
    pub(crate) instance: String,
    pub(crate) adapter: String,
    pub(crate) sha256: String,
    pub(crate) staged: PathBuf,
}

/// Enabled game-dir claims by dest. `game_manifests` is already ordered by
/// `(load_order, instance)`, so each dest's claims list is ordered loser to
/// winner.
#[derive(Clone, Debug, Default)]
pub struct ClaimInventory {
    claims: BTreeMap<String, Vec<DestClaim>>,
}

impl ClaimInventory {
    pub(crate) fn claims_for(&self, dest: &str) -> &[DestClaim] {
        self.claims.get(dest).map(Vec::as_slice).unwrap_or(&[])
    }

    pub(crate) fn remove_claim(&mut self, instance: &str, dest: &str) {
        if let Some(claims) = self.claims.get_mut(dest) {
            claims.retain(|claim| claim.instance != instance);
            if claims.is_empty() {
                self.claims.remove(dest);
            }
        }
    }
}

/// Collect every enabled, kept Install-manifest claim on the game-dir root.
/// Disabled manifests, omitted dests, and preload manifests contribute
/// nothing at runtime and are excluded.
pub(crate) fn claim_inventory(data_dir: &std::path::Path, game_id: &str) -> Result<ClaimInventory> {
    claim_inventory_from_manifests(
        data_dir,
        game_id,
        &crate::game_manifests(data_dir, game_id)?,
    )
}

pub(crate) fn claim_inventory_from_manifests(
    data_dir: &std::path::Path,
    game_id: &str,
    manifests: &[crate::FileManifest],
) -> Result<ClaimInventory> {
    let mut inventory = ClaimInventory::default();
    for manifest in manifests {
        if !manifest.enabled || !crate::is_install(&manifest.adapter) {
            continue;
        }
        for file in manifest.files.iter().filter(|file| file.enabled) {
            inventory
                .claims
                .entry(file.dest.clone())
                .or_default()
                .push(DestClaim {
                    instance: manifest.instance.clone(),
                    adapter: manifest.adapter.clone(),
                    sha256: file.sha256.clone(),
                    staged: stage_dir(data_dir, game_id, &manifest.instance).join(&file.dest),
                });
        }
    }
    Ok(inventory)
}
