use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::claims::{claim_inventory_from_manifests, ClaimInventory};
use super::material::{
    classify_dest, ensure_symlink_supported, staged_source, DestStatus, InstallMaterial,
};
use super::{prefix_drive_c, resolve_target, CopyOp, Removal};
use crate::{apply_copies, write_manifest, Error, FileManifest, Result};

/// Winner-removal outcome. Promoted dests are reported separately because
/// they are neither restored foreign originals nor deleted payloads.
pub(crate) struct PromotionOutcome {
    pub removal: Removal,
    pub promoted: Vec<String>,
}

struct PendingPromotion {
    dest: String,
    instance: String,
    op: CopyOp,
    backup: Option<String>,
}

enum Pending {
    Remove(String),
    Promote(PendingPromotion),
}

/// Remove departing game-dir claims and promote the next enabled claimant
/// wherever the departing claim is physically live. A departing loser never
/// touches the winner; a departing winner is replaced by the next claimant
/// in the promoted manifest's own materialization.
pub(crate) fn remove_with_promotion(
    data_dir: &Path,
    game_id: &str,
    departing: FileManifest,
    excluded: &BTreeSet<(String, String)>,
    backups: &mut BTreeMap<String, String>,
    dests: &[String],
    root: &Path,
    prefix: Option<&Path>,
) -> Result<PromotionOutcome> {
    let mut manifests = crate::game_manifests(data_dir, game_id)?;
    let instance = departing.instance.clone();
    let departing_dests = departing
        .files
        .iter()
        .map(|file| file.dest.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(slot) = manifests
        .iter_mut()
        .find(|manifest| manifest.instance == instance)
    {
        *slot = departing;
    } else {
        manifests.push(departing);
    }
    // The live inventory answers "whose content is at this dest right now",
    // so the departing claim counts as live even when this operation is
    // what disables or omits it: force it (and its departing dests) enabled
    // for classification only.
    let mut live_manifests = manifests.clone();
    if let Some(slot) = live_manifests
        .iter_mut()
        .find(|manifest| manifest.instance == instance)
    {
        slot.enabled = true;
        for file in slot.files.iter_mut() {
            if dests.iter().any(|dest| dest == &file.dest) {
                file.enabled = true;
            }
        }
    }
    let live_inventory = claim_inventory_from_manifests(data_dir, game_id, &live_manifests)?;
    let mut inventory = claim_inventory_from_manifests(data_dir, game_id, &manifests)?;
    for (instance, dest) in excluded {
        inventory.remove_claim(instance, dest);
    }
    let mut outcome = PromotionOutcome {
        removal: Removal {
            restored: Vec::new(),
            deleted: Vec::new(),
            left_foreign: Vec::new(),
        },
        promoted: Vec::new(),
    };

    for dest in dests {
        if !departing_dests.contains(dest) {
            return Err(Error::Manifest(format!(
                "{game_id}: no departing claim: {dest}"
            )));
        }
    }
    let mut pending = Vec::with_capacity(dests.len());
    for dest in dests {
        let target = resolve_target(root, prefix, dest)?;
        let live = match classify_dest(&target, live_inventory.claims_for(dest))? {
            DestStatus::Owned { instances, .. } => instances.iter().any(|owner| owner == &instance),
            _ => false,
        };
        if !live {
            // A departing loser leaves the live dest alone, but it must not
            // take the foreign-original backup record with it while a
            // winner remains: hand the record to the live winner so the
            // original still restores when the last claim departs.
            if let Some(record) = backups.remove(dest) {
                if let Some(winner) = inventory.claims_for(dest).last().cloned() {
                    let slot = manifests
                        .iter_mut()
                        .find(|manifest| manifest.instance == winner.instance)
                        .ok_or_else(|| {
                            Error::Manifest(format!(
                                "{game_id}: no claiming manifest: {}",
                                winner.instance
                            ))
                        })?;
                    if !slot.backups.contains_key(dest) {
                        slot.backups.insert(dest.clone(), record);
                        write_manifest(data_dir, slot)?;
                    }
                } else {
                    backups.insert(dest.clone(), record);
                }
            }
            continue;
        }
        let Some(next) = inventory.claims_for(dest).last().cloned() else {
            pending.push(Pending::Remove(dest.clone()));
            continue;
        };
        // Promotion is best-effort: the departing removal the user asked
        // for must not be vetoed by the next claimant's broken staging
        // (or an incapable Links root). An unplannable promotion degrades
        // to the plain removal below; resync restores the loser later.
        match plan_promotion(
            data_dir,
            game_id,
            &manifests,
            backups,
            dest,
            &next.instance,
            root,
            prefix,
        ) {
            Ok(promotion) => pending.push(Pending::Promote(promotion)),
            Err(e) => {
                tracing::warn!(
                    game = game_id,
                    dest,
                    error = %e,
                    "promotion unplannable; removing without promoting"
                );
                pending.push(Pending::Remove(dest.clone()));
            }
        }
    }
    apply_pending(
        data_dir,
        game_id,
        &instance,
        &mut manifests,
        &live_inventory,
        backups,
        root,
        prefix,
        pending,
        &mut outcome,
    )?;
    Ok(outcome)
}

fn plan_promotion(
    data_dir: &Path,
    game_id: &str,
    manifests: &[FileManifest],
    departing_backups: &BTreeMap<String, String>,
    dest: &str,
    instance: &str,
    root: &Path,
    prefix: Option<&Path>,
) -> Result<PendingPromotion> {
    let claimant = manifests
        .iter()
        .find(|manifest| manifest.instance == instance)
        .ok_or_else(|| Error::Manifest(format!("{game_id}: no claiming manifest: {instance}")))?;
    claimant
        .files
        .iter()
        .find(|file| file.dest == dest && file.enabled)
        .ok_or_else(|| Error::Manifest(format!("{game_id}: no claiming dest: {dest}")))?;
    let material = InstallMaterial::of(&claimant.adapter);
    let src = staged_source(data_dir, game_id, &claimant.instance, dest);
    if !src.is_file() {
        return Err(Error::Manifest(format!(
            "{game_id} {}: not staged: {dest}",
            claimant.instance
        )));
    }
    // Resolve for the prefix/dest-shape error; the probe below gates Links.
    resolve_target(root, prefix, dest)?;
    if material.is_links() {
        if super::is_prefix_dest(dest) {
            let drive = prefix_drive_c(prefix.ok_or_else(|| {
                Error::Install(format!("{dest}: pfx: dest needs a Proton/Wine prefix"))
            })?);
            ensure_symlink_supported(&drive, "prefix destination root")?;
        } else {
            ensure_symlink_supported(root, "game directory")?;
        }
    }
    Ok(PendingPromotion {
        dest: dest.to_string(),
        instance: instance.to_string(),
        op: CopyOp {
            dest: dest.to_string(),
            src,
            needs_confirm: false,
            is_prefix: super::is_prefix_dest(dest),
            material,
        },
        backup: departing_backups.get(dest).cloned(),
    })
}

fn apply_pending(
    data_dir: &Path,
    game_id: &str,
    departing_instance: &str,
    manifests: &mut [FileManifest],
    live_inventory: &ClaimInventory,
    backups: &mut BTreeMap<String, String>,
    root: &Path,
    prefix: Option<&Path>,
    pending: Vec<Pending>,
    outcome: &mut PromotionOutcome,
) -> Result<()> {
    let departing = manifests
        .iter()
        .find(|manifest| manifest.instance == departing_instance)
        .ok_or_else(|| Error::Manifest(format!("{game_id}: no departing manifest")))?;
    let (owner_instance, owner_adapter, owner_files) = (
        departing.instance.clone(),
        departing.adapter.clone(),
        departing.files.clone(),
    );
    for pending in pending {
        let promotion = match pending {
            Pending::Remove(dest) => {
                super::remove_owned_dest(
                    data_dir,
                    game_id,
                    &owner_instance,
                    &owner_adapter,
                    &owner_files,
                    &dest,
                    backups,
                    root,
                    prefix,
                    &mut outcome.removal,
                )?;
                continue;
            }
            Pending::Promote(promotion) => promotion,
        };
        let dest = promotion.dest.clone();
        // Re-fetch the claimant: an earlier promotion in this same removal
        // may have persisted a backup record for another dest already, and
        // persisting a plan-time clone would drop it.
        let promoted = manifests
            .iter_mut()
            .find(|manifest| manifest.instance == promotion.instance)
            .ok_or_else(|| {
                Error::Manifest(format!(
                    "{game_id}: no claiming manifest: {}",
                    promotion.instance
                ))
            })?;
        if let Some(record) = promotion.backup {
            if !promoted.backups.contains_key(&dest) {
                promoted.backups.insert(dest.clone(), record);
            }
        }
        apply_copies(
            data_dir,
            game_id,
            promoted,
            root,
            prefix,
            &[promotion.op],
            live_inventory,
        )?;
        backups.remove(&dest);
        outcome.promoted.push(dest);
    }
    Ok(())
}
