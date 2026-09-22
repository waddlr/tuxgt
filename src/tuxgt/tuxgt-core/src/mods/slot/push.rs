use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::*;
use crate::stage::{sync_staging, StageInput};
use crate::{write_manifest, Error, Result};

/// Payload-side drift for one installed instance (`gui.mod-config-edit`):
/// enabled manifest dests whose current depot bytes no longer hash to the
/// recorded sha (the Settings payload was edited after install). Missing
/// depot sources are skipped, not drift — other paths already error on
/// those. Sorted.
pub fn payload_drift(data_dir: &Path, game: &str, instance: &str) -> Result<Vec<String>> {
    let m = crate::need_manifest(data_dir, game, instance)?;
    let mut out = BTreeSet::new();
    for f in m.files.iter().filter(|f| f.enabled) {
        let Some(src) = depot_source(data_dir, f) else {
            continue;
        };
        if crate::sha256_file(&src)? != f.sha256 {
            out.insert(f.dest.clone());
        }
    }
    Ok(out.into_iter().collect())
}

/// Outcome of pushing Settings payload edits down to one installed
/// instance: staged copies refreshed from the edited payload vs per-game
/// touches left alone.
pub struct PushReport {
    pub updated: Vec<String>,
    pub preserved: Vec<String>,
}

/// Push Settings payload edits to one installed instance
/// (`gui.mod-config-edit` global save): refresh manifest shas from current
/// depot bytes (manifest rewritten only when at least one changed), then a
/// non-force sync that updates in-sync staged files while pre-existing
/// user-modified files are preserved and reported, never fatal. Every other
/// sync error propagates. Both lists sorted.
pub fn push_global_edits(data_dir: &Path, game: &str, instance: &str) -> Result<PushReport> {
    let mut m = crate::need_manifest(data_dir, game, instance)?;
    let mut changed = BTreeSet::new();
    for f in m.files.iter_mut().filter(|f| f.enabled) {
        let src = depot_source(data_dir, f).ok_or_else(|| {
            Error::Manifest(format!("{game} {instance}: no depot source for {}", f.dest))
        })?;
        let hash = crate::sha256_file(&src)?;
        if hash != f.sha256 {
            f.sha256 = hash;
            changed.insert(f.dest.clone());
        }
    }
    if !changed.is_empty() {
        write_manifest(data_dir, &m)?;
    }
    let planned: BTreeSet<String> = m
        .files
        .iter()
        .filter(|f| f.enabled)
        .map(|f| f.dest.clone())
        .collect();
    let preserved: BTreeSet<String> = crate::stage_status(data_dir, game)?
        .into_iter()
        .filter(|l| {
            l.instance == instance
                && l.state == crate::StageState::UserModified
                && planned.contains(&l.file)
        })
        .map(|l| l.file)
        .collect();
    let sources: Vec<(String, PathBuf, String)> = m
        .files
        .iter()
        .filter(|f| f.enabled)
        .map(|f| {
            let src = depot_source(data_dir, f).ok_or_else(|| {
                Error::Manifest(format!("{game} {instance}: no depot source for {}", f.dest))
            })?;
            Ok((f.dest.clone(), src, f.sha256.clone()))
        })
        .collect::<Result<_>>()?;
    let inputs: Vec<StageInput> = sources
        .iter()
        .map(|(rel, src, sha)| StageInput { rel, src, sha })
        .collect();
    match sync_staging(data_dir, game, instance, &inputs, false) {
        Ok(()) => {}
        Err(Error::StagedModified(_)) => {}
        Err(e) => return Err(e),
    }
    let updated: Vec<String> = crate::stage_status(data_dir, game)?
        .into_iter()
        .filter(|l| {
            l.instance == instance
                && l.state == crate::StageState::InSync
                && changed.contains(&l.file)
                && !preserved.contains(&l.file)
        })
        .map(|l| l.file)
        .collect();
    Ok(PushReport {
        updated,
        preserved: preserved.into_iter().collect(),
    })
}

/// Push Settings payload edits of one catalog Mod to every installed game
/// (`gui.mod-config-edit` global save fan-out). Pool-free walk of
/// `games/*/*/manifests/*.toml` (same two-level shape as
/// `harvest_all`); games without the instance are skipped, never errors.
pub fn push_global_edits_all(data_dir: &Path, mod_id: &str) -> Vec<(String, Result<PushReport>)> {
    let mut games = BTreeSet::new();
    if let Ok(l1) = std::fs::read_dir(data_dir.join("games")) {
        for a in l1.flatten() {
            let Ok(l2) = std::fs::read_dir(a.path()) else {
                continue;
            };
            for b in l2.flatten() {
                // Per-game manifests dir (`MANIFESTS_DIR` in
                // `download/hash.rs`); missing dir = nothing installed.
                let Ok(rd) = std::fs::read_dir(b.path().join("manifests")) else {
                    continue;
                };
                for ent in rd.flatten() {
                    let p = ent.path();
                    if p.extension().map_or(true, |e| e != "toml") {
                        continue;
                    }
                    let Ok(text) = std::fs::read_to_string(&p) else {
                        continue;
                    };
                    let Ok(v): std::result::Result<toml::Value, _> = toml::from_str(&text)
                    else {
                        continue;
                    };
                    let hit = v
                        .get("instance")
                        .and_then(|i| i.as_str())
                        .is_some_and(|i| i == mod_id);
                    if hit {
                        if let Some(game) = v.get("game").and_then(|g| g.as_str()) {
                            games.insert(game.to_string());
                        }
                    }
                }
            }
        }
    }
    games
        .into_iter()
        .map(|game| {
            let outcome = push_global_edits(data_dir, &game, mod_id);
            (game, outcome)
        })
        .collect()
}
