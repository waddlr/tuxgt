use std::path::Path;

use sqlx::SqlitePool;

use crate::{
    apply_copies, plan_copies, prewire_game, remove_copies, tracked_dests, write_manifest, Error,
    FileManifest, Result,
};

pub async fn set_instance_enabled(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    on: bool,
    yes: bool,
) -> Result<FileManifest> {
    tracing::debug!(game, instance, on, "enable entry");
    let mut m = crate::need_manifest(data_dir, game, instance)?;
    m.enabled = on;
    // Unified budget gate (both directions), same rule as the file-keep
    // gate: an unchanged body skips prewire; a changed body must fit the
    // budget or strictly shrink an over-budget list, else the toggle is
    // refused before any manifest write (R35 P1 for the on-path; the
    // ratchet for the off-path).
    let mut manifests = crate::game_manifests(data_dir, game)?;
    let current = crate::prewire::body_for(&manifests);
    if let Some(slot) = manifests.iter_mut().find(|o| o.instance == instance) {
        *slot = m.clone();
    }
    let new = crate::prewire::body_for(&manifests);
    let skip_prewire = if new == current {
        true
    } else {
        match crate::prewire::ensure_ini_budget(pool, data_dir, game, &m).await {
            Ok(()) => false,
            Err(e) => {
                if !super::keep::ratchet_shrink(&current, &new) {
                    if let Some((key, len)) = super::keep::over_list(&new) {
                        return Err(refuse_toggle(game, instance, on, key, len));
                    }
                    return Err(e);
                }
                // A shrinking disable lands even though the game is still
                // over budget; the ini stays stale until a later toggle
                // brings the lists under budget.
                true
            }
        }
    };
    // Install adapter: settle the foreign-overwrite confirm before the
    // manifest write so a refused enable changes nothing.
    if m.adapter == "install" && on && !yes {
        let root = crate::game_root(pool, game).await?;
        let prefix =
            crate::install::prefix_for(pool, game, m.files.iter().map(|f| f.dest.as_str())).await?;
        let tracked = tracked_dests(data_dir, game)?;
        let ops = plan_copies(
            data_dir,
            game,
            &m.instance,
            m.files.iter().filter(|f| f.enabled),
            &root,
            prefix.as_deref(),
            &tracked,
        )?;
        let need: Vec<String> = ops
            .iter()
            .filter(|o| o.needs_confirm)
            .map(|o| o.dest.clone())
            .collect();
        if !need.is_empty() {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests: {}",
                need.join(", ")
            )));
        }
    }
    write_manifest(data_dir, &m)?;
    if m.adapter == "install" {
        let root = crate::game_root(pool, game).await?;
        let prefix =
            crate::install::prefix_for(pool, game, m.files.iter().map(|f| f.dest.as_str())).await?;
        let tracked = tracked_dests(data_dir, game)?;
        if on {
            let ops = plan_copies(
                data_dir,
                game,
                &m.instance,
                m.files.iter().filter(|f| f.enabled),
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
            // Confirm pre-settled above (same plan inputs).
            apply_copies(
                data_dir,
                game,
                &mut m,
                &root,
                prefix.as_deref(),
                &ops,
                &tracked,
            )?;
        } else {
            let dests = m.files.iter().map(|f| f.dest.as_str());
            let _ = remove_copies(
                data_dir,
                &mut m.backups,
                dests,
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
            write_manifest(data_dir, &m)?;
        }
    }
    if !skip_prewire {
        prewire_game(data_dir, pool, game).await?;
    }
    tracing::info!(game, instance, enabled = on, "toggled");
    Ok(m)
}

/// Budget refusal naming the over list and the recovery.
fn refuse_toggle(game: &str, instance: &str, on: bool, key: &str, len: usize) -> Error {
    let budget = crate::prewire::INI_LIST_BUDGET;
    let verb = if on { "enable" } else { "disable" };
    Error::Manifest(format!(
        "{game}: cannot {verb} {instance}: the {key} list would still exceed the {budget}-byte budget ({len} bytes) without shrinking; omit dests off the over-budget list or uninstall the over-budget instance (`tuxgt instance uninstall {game} <instance>`) to recover"
    ))
}
