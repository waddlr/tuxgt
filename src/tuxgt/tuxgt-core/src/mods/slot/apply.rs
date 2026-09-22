//! Apply every pick in one card, or leave every dest as it was.
//!
//! A stem is freed before another row takes it. A cycle parks one row with
//! `unplace_instance_slot` to `<self>`: that drops the proxy stem without
//! copying a DLL the user did not pick and without a confirm. A later error
//! restores the pre-call snapshot, including when another enabled mod still
//! holds the stem. `ensure_repick_free` still rejects a shared stem before
//! any write.

use std::collections::BTreeMap;
use std::path::Path;

use sqlx::SqlitePool;

use super::choice::{
    ensure_repick_free, is_self_slot, resolve_slot_dest, stock_basename, SELF_SLOT,
};
use super::snap;
use super::{claiming_slot_index, set_instance_slot, unplace_instance_slot};
use crate::{parse_slot, Error, FileManifest, Result};

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn held_stem(m: &FileManifest) -> Option<String> {
    let idx = claiming_slot_index(&m.files, &m.include)?;
    parse_slot(basename(&m.files[idx].dest))
        .ok()
        .map(|s| s.as_str().to_string())
}

fn held_map(manifests: &[FileManifest]) -> BTreeMap<String, String> {
    let mut held = BTreeMap::new();
    for m in manifests.iter().filter(|m| m.enabled) {
        if let Some(stem) = held_stem(m) {
            held.insert(m.instance.clone(), stem);
        }
    }
    held
}

fn update_held(held: &mut BTreeMap<String, String>, inst: &str, slot: &str) -> Result<()> {
    if is_self_slot(slot) {
        held.remove(inst);
        return Ok(());
    }
    let stem = parse_slot(slot)?.as_str().to_string();
    held.insert(inst.to_string(), stem);
    Ok(())
}

fn target_blocked(held: &BTreeMap<String, String>, inst: &str, slot: &str) -> Result<bool> {
    if is_self_slot(slot) {
        return Ok(false);
    }
    let want = parse_slot(slot)?.as_str().to_string();
    Ok(held
        .iter()
        .any(|(other, stem)| other != inst && stem == &want))
}

/// Instance in `pending` that currently holds a stem another pending pick wants.
fn cycle_breaker(held: &BTreeMap<String, String>, pending: &[(&str, &str)]) -> Option<String> {
    for (inst, slot) in pending {
        if is_self_slot(slot) {
            continue;
        }
        let Ok(want) = parse_slot(slot) else {
            continue;
        };
        let want = want.as_str();
        let Some(holder) = held.iter().find_map(|(other, stem)| {
            (other.as_str() != *inst && stem == want).then(|| other.clone())
        }) else {
            continue;
        };
        if pending.iter().any(|(id, _)| *id == holder) {
            return Some(holder);
        }
    }
    None
}

/// Order that frees a stem before it is taken. A cycle parks one row on
/// `<self>` (no proxy scratch). Nothing is touched here.
pub(crate) fn plan_slot_writes(
    manifests: &[FileManifest],
    picks: &[(&str, &str)],
) -> Result<Vec<(String, String)>> {
    let mut held = held_map(manifests);
    let mut pending: Vec<(&str, &str)> = picks.to_vec();
    let mut plan = Vec::new();
    let limit = picks.len().saturating_mul(3).saturating_add(1);
    let mut spins = 0;
    while !pending.is_empty() {
        spins += 1;
        if spins > limit {
            return Err(Error::InvalidInstance("slot picks did not settle".into()));
        }
        let mut ready = Vec::new();
        for (i, (inst, slot)) in pending.iter().enumerate() {
            if !target_blocked(&held, inst, slot)? {
                ready.push(i);
            }
        }
        if ready.is_empty() {
            let inst = cycle_breaker(&held, &pending).ok_or_else(|| {
                Error::InvalidInstance("slot pick blocked by a stem this card does not free".into())
            })?;
            // Drop the stem. The executed step unplaces to `<self>` and
            // does not copy a proxy the card did not pick.
            held.remove(&inst);
            plan.push((inst, SELF_SLOT.to_string()));
            continue;
        }
        // One row per pass so a stem freed here unblocks the next row.
        let i = ready[0];
        let (inst, slot) = pending[i];
        update_held(&mut held, inst, slot)?;
        plan.push((inst.to_string(), slot.to_string()));
        pending.remove(i);
    }
    Ok(plan)
}

fn is_chosen(picks: &[(&str, &str)], inst: &str, slot: &str) -> bool {
    picks.iter().any(|(i, s)| *i == inst && *s == slot)
}

/// Dest filenames this call may create or remove, beyond the current claiming
/// dests `capture` already stores. The park lands on the stock basename.
fn watched_dests(manifests: &[FileManifest], picks: &[(&str, &str)]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for (inst, slot) in picks {
        let m = manifests
            .iter()
            .find(|m| m.instance == *inst)
            .ok_or_else(|| Error::NoManifest(format!("missing {inst}")))?;
        let idx = claiming_slot_index(&m.files, &m.include).ok_or_else(|| {
            Error::InvalidInstance(format!("{inst}: no proxy slot dest to rewrite"))
        })?;
        let stock = stock_basename(&m.files[idx].source);
        out.push(resolve_slot_dest(slot, &stock)?);
        out.push(stock);
    }
    Ok(out)
}

/// Write `picks` (`instance`, slot token) or restore every byte this call changed.
pub async fn apply_slot_picks(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    picks: &[(&str, &str)],
    yes: bool,
) -> Result<()> {
    if picks.is_empty() {
        return Ok(());
    }
    ensure_repick_free(data_dir, game, picks)?;
    let manifests = crate::game_manifests(data_dir, game)?;
    let plan = plan_slot_writes(&manifests, picks)?;
    let extra = watched_dests(&manifests, picks)?;
    let mut snap = snap::capture(pool, data_dir, game, picks).await?;
    snap.include_dests(pool, game, &extra).await?;
    for (inst, slot) in &plan {
        let written = if is_chosen(picks, inst, slot) {
            set_instance_slot(pool, data_dir, game, inst, slot, yes).await
        } else {
            // Cycle park. `<self>` is not a proxy, so the stem is freed
            // without copying a scratch DLL or confirming that name.
            unplace_instance_slot(pool, data_dir, game, inst, SELF_SLOT, false).await
        };
        if let Err(e) = written {
            snap.restore(pool, game).await;
            return Err(e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlannedFile;

    fn manifest(instance: &str, dest: &str) -> FileManifest {
        FileManifest {
            game: "g".into(),
            instance: instance.into(),
            mod_type: "reshade".into(),
            adapter: "install".into(),
            enabled: true,
            load_order: 0,
            files: vec![PlannedFile {
                source: format!("mods/user/{instance}/ReShade64.dll"),
                dest: dest.into(),
                sha256: String::new(),
                enabled: true,
                load: None,
            }]
            .into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            include: Box::default(),
            harvested: Default::default(),
            provenance: Default::default(),
        }
    }

    #[test]
    fn chain_frees_the_stem_before_it_is_taken() {
        let manifests = vec![manifest("shade", "dxgi.dll"), manifest("opti", "d3d11.dll")];
        let picks = [("shade", "d3d11"), ("opti", "winmm")];
        let plan = plan_slot_writes(&manifests, &picks).unwrap();
        assert_eq!(
            plan,
            vec![
                ("opti".to_string(), "winmm".to_string()),
                ("shade".to_string(), "d3d11".to_string()),
            ]
        );
    }

    #[test]
    fn cycle_parks_on_self() {
        let manifests = vec![manifest("shade", "dxgi.dll"), manifest("opti", "d3d11.dll")];
        let picks = [("shade", "d3d11"), ("opti", "dxgi")];
        let plan = plan_slot_writes(&manifests, &picks).unwrap();
        assert!(plan
            .iter()
            .any(|(id, slot)| id == "opti" && slot == "<self>"));
        assert!(plan
            .iter()
            .any(|(id, slot)| id == "shade" && slot == "d3d11"));
        assert!(plan.iter().any(|(id, slot)| id == "opti" && slot == "dxgi"));
        assert!(plan
            .iter()
            .all(|(_, slot)| slot == "<self>" || slot == "dxgi" || slot == "d3d11"));
    }
}
