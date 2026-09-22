use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::*;
use crate::stage::{sync_staging, StageInput};
use crate::{
    apply_copies, plan_copies, tracked_dests, write_manifest, Error, FileManifest, PlannedFile,
    Result,
};

mod apply;
mod choice;
mod modes;
mod push;
mod snap;
mod unplace_convert;

pub use apply::apply_slot_picks;
pub(crate) use choice::{
    conversion_needs_prompt, ensure_dest_free, ensure_slot_free, is_named_injector,
    resolve_slot_dest, stock_basename,
};
pub use choice::{
    ensure_repick_free, is_self_slot, recipe_slot_configurable, resync_repick_instances, SELF_SLOT,
};
pub(crate) use modes::forget_instance;
pub use modes::remembered_slot;
pub use push::{payload_drift, push_global_edits, push_global_edits_all, PushReport};
pub use unplace_convert::{convert_after_unplace, preflight_convert_picks};

pub fn resync_instance(
    data_dir: &Path,
    game: &str,
    instance: &str,
    force: bool,
) -> Result<Vec<crate::StageLine>> {
    let m = crate::need_manifest(data_dir, game, instance)?;
    let sources: Vec<(String, PathBuf, String)> = m
        .files
        .iter()
        .filter(|f| f.enabled)
        .map(|f| {
            let src = depot_source(data_dir, f).ok_or_else(|| {
                Error::Manifest(format!("{game} {instance}: no depot source for {}", f.dest))
            })?;
            if !src.is_file() {
                return Err(Error::Manifest(format!(
                    "{game} {instance}: depot source missing for {}: {}",
                    f.dest,
                    src.display()
                )));
            }
            Ok((f.dest.clone(), src, f.sha256.clone()))
        })
        .collect::<Result<_>>()?;
    let inputs: Vec<StageInput> = sources
        .iter()
        .map(|(rel, src, sha)| StageInput { rel, src, sha })
        .collect();
    sync_staging(data_dir, game, instance, &m.mod_type, &inputs, force)?;
    Ok(crate::stage_status(data_dir, game)?
        .into_iter()
        .filter(|l| l.instance == instance)
        .collect())
}

/// Force re-sync staging for every installed instance of a game (R33).
/// Stops at the first instance that reports user-touched files without `force`.
pub fn resync_game(data_dir: &Path, game: &str, force: bool) -> Result<Vec<crate::StageLine>> {
    let mut out = Vec::new();
    for m in crate::game_manifests(data_dir, game)? {
        out.extend(resync_instance(data_dir, game, &m.instance, force)?);
    }
    Ok(out)
}

fn dest_basename(d: &str) -> &str {
    d.rsplit(['/', '\\']).next().unwrap_or(d)
}

/// Index of the claiming Load dest: the enabled sibling Load DLL (an
/// `include`-covered DLL never claims; subdir companions and `pfx:` dests
/// never claim — only a top-level dest sits beside the game exe). A
/// slot-named sibling wins. Otherwise the single named injector
/// (`OptiScaler.dll`, `ReShade64.dll`, `ReShade32.dll`) wins over companion
/// DLLs. A lone sibling wins when nothing is named. `None` when there is
/// no candidate or two injectors (both ReShade arch DLLs, two unnamed DLLs).
pub(crate) fn claiming_slot_index(files: &[PlannedFile], include: &[String]) -> Option<usize> {
    let claiming: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.enabled
                && crate::prewire::is_dll(&f.dest)
                && !crate::download::include_covers(include, &f.dest)
                && !f.dest.contains('/')
                && !f.dest.contains('\\')
                && !crate::install::is_prefix_dest(&f.dest)
        })
        .map(|(i, _)| i)
        .collect();
    if let Some(i) = claiming
        .iter()
        .copied()
        .find(|i| crate::modtype::parse_slot(dest_basename(&files[*i].dest)).is_ok())
    {
        return Some(i);
    }
    let injectors: Vec<usize> = claiming
        .iter()
        .copied()
        .filter(|i| choice::is_named_injector(&files[*i].dest))
        .collect();
    if injectors.len() == 1 {
        return Some(injectors[0]);
    }
    (claiming.len() == 1).then(|| claiming[0])
}

/// Rewrite the claiming Load dest (E91). `<self>` keeps the source basename;
/// a proxy stem becomes `<stem>.dll`. Restage from the depot, rewrite the
/// manifest, sync the install-adapter game-dir copy, then prewire. See
/// [`claiming_slot_index`] for which dest claims. Reinstall keeps the new
/// dest: the preserve pass matches prior dests by source. A dest another
/// enabled mod already holds is [`Error::SlotInUse`] before the same-dest
/// no-op.
pub async fn set_instance_slot(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    slot: &str,
    yes: bool,
) -> Result<FileManifest> {
    rewrite_instance_slot(pool, data_dir, game, instance, slot, yes, true, true).await
}

/// Conversion place. Mode memory is already saved, so this rename must not
/// overwrite the mode the game is still on.
pub(crate) async fn place_instance_slot(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    slot: &str,
    yes: bool,
) -> Result<FileManifest> {
    rewrite_instance_slot(pool, data_dir, game, instance, slot, yes, true, false).await
}

/// Install-to-preload pick. The old tracked game-dir copy comes out and the
/// manifest records the new dest, but that dest is not copied in: preload
/// keeps no game-dir DLL, and copying it would only be undone by the convert.
/// A disabled mod is manifest-only, so its DLL is not put back.
pub async fn unplace_instance_slot(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    slot: &str,
    yes: bool,
) -> Result<FileManifest> {
    rewrite_instance_slot(pool, data_dir, game, instance, slot, yes, false, false).await
}

async fn rewrite_instance_slot(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    slot: &str,
    yes: bool,
    place: bool,
    remember: bool,
) -> Result<FileManifest> {
    tracing::debug!(game, instance, slot, place, "slot entry");
    let m = crate::need_manifest(data_dir, game, instance)?;
    let idx = claiming_slot_index(&m.files, &m.include).ok_or_else(|| {
        Error::InvalidInstance(format!("{instance}: no proxy slot dest to rewrite"))
    })?;
    let old_dest = m.files[idx].dest.clone();
    let stock = stock_basename(&m.files[idx].source);
    let want = resolve_slot_dest(slot, &stock)?;
    // Claiming dests are always top-level siblings (filtered above), so the
    // new dest is the bare slot stem — never rewritten inside a subdir.
    let new_dest = want.clone();
    let manifests = crate::game_manifests(data_dir, game)?;
    ensure_dest_free(&manifests, instance, &new_dest)?;
    if new_dest == old_dest {
        tracing::info!(game, instance, slot, "slot unchanged");
        if remember {
            modes::remember_live(data_dir, game, instance, &m.adapter, slot)?;
        }
        return Ok(m);
    }
    // Disabled mods already had their copies removed. Placing the new dest
    // would drop a DLL back while the mod is off. Unplace never writes the
    // new name, so it has nothing to confirm.
    let write_game_dir = crate::is_install(&m.adapter) && m.enabled;
    if place && write_game_dir && !yes {
        let root = crate::game_root(pool, game).await?;
        let prefix =
            crate::install::prefix_for(pool, game, [new_dest.as_str()].into_iter()).await?;
        let tracked = crate::tracked_dests(data_dir, game)?;
        if crate::install::dest_needs_confirm(&new_dest, &root, prefix.as_deref(), &tracked)? {
            return Err(Error::NeedConfirm(format!(
                "foreign game-dir dests: {new_dest}"
            )));
        }
    }
    // R35 P1: validate the prospective loader lists before staging or
    // writing the manifest.
    let mut prospective = m.clone();
    prospective.files[idx].dest = new_dest.clone();
    crate::prewire::ensure_ini_budget(pool, data_dir, game, &prospective).await?;
    // Restage: drop the old rel, stage the new rel from the depot source.
    let f = m.files[idx].clone();
    let src = depot_source(data_dir, &f);
    crate::stage::set_staged(
        data_dir,
        game,
        instance,
        &m.mod_type,
        &old_dest,
        None,
        "",
        false,
    )?;
    crate::stage::set_staged(
        data_dir,
        game,
        instance,
        &m.mod_type,
        &new_dest,
        src.as_deref(),
        &f.sha256,
        true,
    )?;
    let mut m = crate::read_manifest(data_dir, game, instance)?
        .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
    if crate::is_install(&m.adapter) && m.enabled {
        let root = crate::game_root(pool, game).await?;
        let prefix = crate::install::prefix_for(
            pool,
            game,
            [old_dest.as_str(), new_dest.as_str()].into_iter(),
        )
        .await?;
        // While the manifest still names the old dest, that game-dir copy
        // is tracked and comes out. Rewriting the dest first would leave
        // the proxy DLL behind (the game would keep loading it).
        let tracked = crate::tracked_dests(data_dir, game)?;
        let _ = crate::install::remove_dest_copy(
            data_dir,
            &mut m,
            &old_dest,
            &root,
            prefix.as_deref(),
            &tracked,
        )?;
        m.files[idx].dest = new_dest.clone();
        crate::write_manifest(data_dir, &m)?;
        if place {
            let tracked = crate::tracked_dests(data_dir, game)?;
            let ops = crate::install::plan_dest_copy(
                data_dir,
                game,
                &m,
                &new_dest,
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
            crate::apply_copies(
                data_dir,
                game,
                &mut m,
                &root,
                prefix.as_deref(),
                &ops,
                &tracked,
            )?;
        }
    } else {
        m.files[idx].dest = new_dest.clone();
        crate::write_manifest(data_dir, &m)?;
    }
    crate::prewire_game(data_dir, pool, game).await?;
    if remember {
        modes::remember_live(data_dir, game, instance, &m.adapter, slot)?;
    }
    tracing::info!(game, instance, slot, "slot set");
    Ok(m)
}

/// Rewrite the per-game installed-mod order (first = loads first = loses,
/// last = wins) and make both adapters match at once: install-adapter
/// copies re-apply top-to-bottom in the new order (as `--yes`, backups
/// kept), then the ini prewires in the new order. Staging is per-instance
/// and order-independent, so no resync. Validates first: `order` must name
/// exactly the installed instance ids (unknown/missing/duplicates error
/// naming the id); nothing is written on failure.
pub async fn set_load_order(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    order: &[String],
) -> Result<Vec<FileManifest>> {
    let installed = crate::game_manifests(data_dir, game)?;
    let ids: std::collections::BTreeSet<String> =
        installed.iter().map(|m| m.instance.clone()).collect();
    let mut seen = std::collections::BTreeSet::new();
    for id in order {
        if !seen.insert(id.clone()) {
            return Err(Error::InvalidInstance(format!("duplicate instance: {id}")));
        }
        if !ids.contains(id) {
            return Err(Error::InvalidInstance(format!("unknown instance: {id}")));
        }
    }
    if let Some(id) = ids.difference(&seen).next() {
        return Err(Error::InvalidInstance(format!("missing instance: {id}")));
    }
    let mut by_id: std::collections::BTreeMap<String, FileManifest> = installed
        .into_iter()
        .map(|m| (m.instance.clone(), m))
        .collect();
    let mut ordered: Vec<FileManifest> = Vec::with_capacity(order.len());
    for (idx, id) in order.iter().enumerate() {
        let mut m = by_id
            .remove(id)
            .ok_or_else(|| Error::InvalidInstance(format!("unknown instance: {id}")))?;
        m.load_order = idx as i64;
        write_manifest(data_dir, &m)?;
        ordered.push(m);
    }
    // Re-apply install-adapter copies top-to-bottom so on-disk bytes match
    // the new winner. Thread the in-memory struct through (it persists
    // `backups`); explicit user reorder, so `--yes`, no confirm.
    if ordered
        .iter()
        .any(|m| crate::is_install(&m.adapter) && m.enabled)
    {
        let root = crate::game_root(pool, game).await?;
        let tracked = tracked_dests(data_dir, game)?;
        for m in ordered
            .iter_mut()
            .filter(|m| crate::is_install(&m.adapter) && m.enabled)
        {
            let prefix = crate::install::prefix_for(
                pool,
                game,
                m.files
                    .iter()
                    .filter(|f| f.enabled)
                    .map(|f| f.dest.as_str()),
            )
            .await?;
            let ops = plan_copies(
                data_dir,
                game,
                &m.instance,
                m.files.iter().filter(|f| f.enabled),
                &root,
                prefix.as_deref(),
                &tracked,
            )?;
            apply_copies(data_dir, game, m, &root, prefix.as_deref(), &ops, &tracked)?;
        }
    }
    crate::prewire_game(data_dir, pool, game).await?;
    tracing::info!(game, instances = order.len(), "load order set");
    Ok(ordered)
}
