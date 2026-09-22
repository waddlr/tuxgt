use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::{apply_copies, prewire_game, tracked_dests, Error, FileManifest, Result};

pub async fn set_file_keep(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    dest: &str,
    on: bool,
    yes: bool,
) -> Result<FileManifest> {
    let m = crate::need_manifest(data_dir, game, instance)?;
    let f = m
        .files
        .iter()
        .find(|f| f.dest == dest)
        .ok_or_else(|| Error::InvalidInstance(format!("unknown dest: {dest}")))?
        .clone();
    if !on && crate::download::is_required_dest(&m.mod_type, dest, &m.include, m.files.len()) {
        return Err(Error::InvalidInstance(format!(
            "required dest cannot be omitted: {dest}"
        )));
    }
    if on && m.adapter == "install" && !yes {
        // Settle the foreign-overwrite confirm before anything moves: a
        // refused enable leaves the manifest and staging byte-identical.
        let root = crate::game_root(pool, game).await?;
        let prefix = crate::install::prefix_for(pool, game, [dest].into_iter()).await?;
        let tracked = tracked_dests(data_dir, game)?;
        if crate::install::dest_needs_confirm(dest, &root, prefix.as_deref(), &tracked)? {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests: {dest}"
            )));
        }
    }
    // Unified budget gate (both directions). An unchanged body skips prewire
    // entirely (nested non-DLL toggles under always-tree); a changed body
    // must fit the budget or strictly shrink an over-budget list, else the
    // toggle is refused before any staging or manifest write.
    let mut prospective = m.clone();
    if let Some(pf) = prospective.files.iter_mut().find(|f| f.dest == dest) {
        pf.enabled = on;
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
                        return Err(refuse_toggle(game, instance, dest, key, len));
                    }
                    return Err(e);
                }
                // A shrinking write is safe file-state progress even though
                // the game is still over budget; the ini stays stale until a
                // later toggle brings the lists under budget.
                true
            }
        }
    };
    let src = depot_source(data_dir, &f);
    crate::stage::set_staged(
        data_dir,
        game,
        instance,
        dest,
        src.as_deref(),
        &f.sha256,
        on,
    )?;
    let mut m = crate::download::set_file_enabled(data_dir, game, instance, dest, on)?;
    if m.adapter == "install" {
        let root = crate::game_root(pool, game).await?;
        let prefix = crate::install::prefix_for(pool, game, [dest].into_iter()).await?;
        let tracked = tracked_dests(data_dir, game)?;
        if on {
            // The foreign-overwrite confirm was settled before any mutation
            // above, so with `yes` this is a straight apply with a backup.
            let ops = crate::install::plan_dest_copy(
                data_dir,
                game,
                &m,
                dest,
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
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
            let _ = crate::install::remove_dest_copy(
                data_dir,
                &mut m,
                dest,
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
        }
    }
    if !skip_prewire {
        prewire_game(data_dir, pool, game).await?;
    }
    Ok(m)
}

/// Ratchet verdict: no list grows and at least one over-budget list
/// strictly shrinks. Lens order matches `list_lens` (`LoadDLL`,
/// `IncludeFile`).
pub(crate) fn ratchet_shrink(current: &[String], new: &[String]) -> bool {
    let budget = crate::prewire::INI_LIST_BUDGET;
    let [cl0, cl1] = crate::prewire::list_lens(current);
    let [nl0, nl1] = crate::prewire::list_lens(new);
    nl0 <= cl0 && nl1 <= cl1 && ((nl0 < cl0 && nl0 >= budget) || (nl1 < cl1 && nl1 >= budget))
}

/// First over-budget list in `list_lens` order, if any.
pub(crate) fn over_list(body: &[String]) -> Option<(&'static str, usize)> {
    let keys = ["LoadDLL", "IncludeFile"];
    let lens = crate::prewire::list_lens(body);
    (0..2)
        .find(|i| lens[*i] >= crate::prewire::INI_LIST_BUDGET)
        .map(|i| (keys[i], lens[i]))
}

/// Budget refusal naming the over list and the recovery.
fn refuse_toggle(game: &str, instance: &str, dest: &str, key: &str, len: usize) -> Error {
    let budget = crate::prewire::INI_LIST_BUDGET;
    Error::Manifest(format!(
        "{game}: cannot toggle {dest} on {instance}: the {key} list would still exceed the {budget}-byte budget ({len} bytes) without shrinking; toggle dests off the over-budget list or uninstall the over-budget instance (`tuxgt instance uninstall {game} <instance>`) to recover"
    ))
}
