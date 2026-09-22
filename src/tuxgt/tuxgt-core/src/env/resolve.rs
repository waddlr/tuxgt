use sqlx::SqlitePool;

use super::*;
use crate::{Error, Result};

pub(crate) fn live_var(var: &str) -> Option<String> {
    std::env::var(var).ok()
}

/// Reconstruct a knob value from the current process env, if any of its vars are set.
pub fn live_knob_value(k: &EnvKnob) -> Option<String> {
    if let Some(var) = k.freeform {
        return live_var(var);
    }
    for v in k.values {
        if v.env
            .iter()
            .all(|(var, val)| live_var(var).as_deref() == Some(*val))
        {
            return Some(v.value.to_string());
        }
    }
    for v in k.values {
        for (var, _) in v.env {
            if let Some(live) = live_var(var) {
                return Some(live);
            }
        }
    }
    None
}

pub(crate) fn knob_vars_present(k: &EnvKnob) -> bool {
    k.env_vars().iter().any(|var| live_var(var).is_some())
}

/// Live env matches a registered knob and we have no enabled global for it.
/// An enabled+set global is ours even if this process has not restarted yet.
pub fn knob_is_unmanaged(k: &EnvKnob, enabled_global: Option<&str>) -> bool {
    if enabled_global.is_some() {
        return false;
    }
    knob_vars_present(k)
}

pub fn knob_source(
    game: Option<&KnobRow>,
    global: Option<&KnobRow>,
    unmanaged: bool,
) -> KnobSource {
    if game.is_some_and(|r| r.enabled) {
        return KnobSource::Game;
    }
    if unmanaged {
        return KnobSource::Unmanaged;
    }
    if global.is_some_and(|r| r.enabled) {
        return KnobSource::Global;
    }
    KnobSource::None
}

/// Effective stored value for display: game enabled+set, else unmanaged live,
/// else global enabled+set.
pub fn effective_knob_value(
    k: &EnvKnob,
    game: Option<&KnobRow>,
    global: Option<&KnobRow>,
    unmanaged: bool,
) -> Option<String> {
    if game.is_some_and(|r| r.enabled) {
        return game.map(|r| r.value.clone());
    }
    if unmanaged {
        return live_knob_value(k);
    }
    if global.is_some_and(|r| r.enabled) {
        return global.map(|r| r.value.clone());
    }
    None
}

/// Same pick as launch: override else detected, else `proton` when a prefix or
/// proton is set, else `native`.
pub async fn effective_platform(pool: &SqlitePool, id: &str) -> Result<String> {
    let row: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT override_platform, detected_platform,
                override_prefix_path, detected_prefix_path, prefix_path,
                override_proton, detected_proton, proton
         FROM games WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| Error::UnknownGame(id.into()))?;
    let (ovr_plat, det_plat, ovr_pfx, det_pfx, store_pfx, ovt_pro, det_pro, store_pro) = row;
    let plat = ovr_plat.or(det_plat).unwrap_or_default();
    if !plat.is_empty() {
        return Ok(plat);
    }
    let prefix = ovr_pfx.or(det_pfx).or(store_pfx).unwrap_or_default();
    let proton = ovt_pro.or(det_pro).or(store_pro).unwrap_or_default();
    if !prefix.is_empty() || !proton.is_empty() {
        Ok("proton".into())
    } else {
        Ok("native".into())
    }
}
