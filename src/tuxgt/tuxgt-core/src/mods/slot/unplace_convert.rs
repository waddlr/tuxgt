//! Slot picks stay inside the conversion that can roll them back.
//!
//! Both modes' tokens are saved before any rename. Install-to-preload still
//! validates first, so a consent error returns with the proxy DLL in place.
//! Preload-to-install places inside the snapshot: consent sees the install
//! dest, and a refusal or a failed convert puts the preload dests back.

use std::path::Path;

use sqlx::SqlitePool;

use super::apply::plan_slot_writes;
use super::choice::{ensure_repick_free, resolve_slot_dest, stock_basename, SELF_SLOT};
use super::modes::remember_conversion;
use super::snap;
use super::{claiming_slot_index, place_instance_slot, unplace_instance_slot};
use crate::{convert_game_adapter, validate_adapter_convert, ConversionReport, Error, Result};

/// Read-only check for a conversion that already has picks. Runs before the
/// store client is stopped. `SlotInUse` when two picks share a stem. An
/// install target also returns the foreign game-dir `NeedConfirm`
/// `place_instance_slot` would return for the picked filenames (`<self>`
/// resolved to the stock basename), listing every such dest. `yes` skips
/// that confirm. Does not rename and does not remember mode tokens.
pub async fn preflight_convert_picks(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    picks: &[(&str, &str)],
) -> Result<()> {
    if picks.is_empty() {
        return Ok(());
    }
    ensure_repick_free(data_dir, game, picks)?;
    let target = crate::game::validate_adapter(target)?;
    if !crate::is_install(target) || yes {
        return Ok(());
    }
    let manifests = crate::game_manifests(data_dir, game)?;
    let mut need = Vec::new();
    for (inst, slot) in picks {
        let m = manifests
            .iter()
            .find(|m| m.instance == *inst)
            .ok_or_else(|| Error::NoManifest(format!("missing {inst}")))?;
        if !m.enabled {
            continue;
        }
        let idx = claiming_slot_index(&m.files, &m.include).ok_or_else(|| {
            Error::InvalidInstance(format!("{inst}: no proxy slot dest to rewrite"))
        })?;
        let stock = stock_basename(&m.files[idx].source);
        let new_dest = resolve_slot_dest(slot, &stock)?;
        if new_dest == m.files[idx].dest || need.iter().any(|d| d == &new_dest) {
            continue;
        }
        need.push(new_dest);
    }
    if need.is_empty() {
        return Ok(());
    }
    let root = crate::game_root(pool, game).await?;
    let prefix = crate::install::prefix_for(pool, game, need.iter().map(String::as_str)).await?;
    let tracked = crate::tracked_dests(data_dir, game)?;
    let mut foreign = Vec::new();
    for dest in &need {
        if crate::install::dest_needs_confirm(dest, &root, prefix.as_deref(), &tracked)?
            && !foreign.iter().any(|d| d == dest)
        {
            foreign.push(dest.clone());
        }
    }
    if !foreign.is_empty() {
        return Err(Error::NeedConfirm(format!(
            "foreign game-dir dests: {}",
            foreign.join(", ")
        )));
    }
    Ok(())
}

/// Save both modes, then rename inside the conversion. Empty `picks` is a
/// plain convert. Install-to-preload does not unplace on `NeedConfirm`.
/// Preload-to-install restores the preload dests on `NeedConfirm` or error.
pub async fn convert_after_unplace(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
    picks: &[(&str, &str)],
) -> Result<ConversionReport> {
    if picks.is_empty() {
        return convert_game_adapter(pool, data_dir, config_dir, game, target, yes, slots_chosen)
            .await;
    }
    let target = crate::game::validate_adapter(target)?;
    ensure_repick_free(data_dir, game, picks)?;
    let from = crate::game::game_adapter(pool, game).await?;
    remember_conversion(data_dir, game, &from, target, picks)?;
    if crate::is_install(target) {
        return place_then_convert(
            pool,
            data_dir,
            config_dir,
            game,
            target,
            yes,
            slots_chosen,
            picks,
        )
        .await;
    }
    validate_adapter_convert(pool, data_dir, config_dir, game, target, yes, slots_chosen).await?;
    let manifests = crate::game_manifests(data_dir, game)?;
    let plan = plan_slot_writes(&manifests, picks)?;
    let snap = snap::capture(pool, data_dir, game, picks).await?;
    for (inst, slot) in &plan {
        if let Err(e) = unplace_instance_slot(pool, data_dir, game, inst, slot, false).await {
            snap.restore(pool, game).await;
            return Err(e);
        }
    }
    finish_convert(
        pool,
        data_dir,
        config_dir,
        game,
        target,
        yes,
        slots_chosen,
        &snap,
    )
    .await
}

/// The install dest is what consent and the copy see, so the place happens
/// inside the snapshot. A refusal restores the preload names.
async fn place_then_convert(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
    picks: &[(&str, &str)],
) -> Result<ConversionReport> {
    let manifests = crate::game_manifests(data_dir, game)?;
    let plan = plan_slot_writes(&manifests, picks)?;
    let snap = snap::capture(pool, data_dir, game, picks).await?;
    for (inst, slot) in &plan {
        // A cycle step is `<self>` only so the stem is free. Placing it
        // would copy a DLL the user did not pick. The chosen step places.
        let written = if picks.iter().any(|(i, s)| *i == inst && *s == slot) {
            place_instance_slot(pool, data_dir, game, inst, slot, yes).await
        } else {
            unplace_instance_slot(pool, data_dir, game, inst, SELF_SLOT, false).await
        };
        if let Err(e) = written {
            snap.restore(pool, game).await;
            return Err(e);
        }
    }
    finish_convert(
        pool,
        data_dir,
        config_dir,
        game,
        target,
        yes,
        slots_chosen,
        &snap,
    )
    .await
}

async fn finish_convert(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
    target: &str,
    yes: bool,
    slots_chosen: bool,
    snap: &snap::Snap,
) -> Result<ConversionReport> {
    match convert_game_adapter(pool, data_dir, config_dir, game, target, yes, slots_chosen).await {
        Ok(report) => Ok(report),
        Err(e) => {
            snap.restore(pool, game).await;
            Err(e)
        }
    }
}
