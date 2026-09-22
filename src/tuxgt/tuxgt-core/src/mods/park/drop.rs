use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::{
    base_name, claim_of, disabled_root, globs_of, load_mine, matches_any, move_replacing,
    move_unless_dest, plain_file, remember_parent, sweep_parents, Claim, GAME_SIDE, RUNTIME_SIDE,
};
use crate::{Error, FileManifest, Result};

fn adapter_side(m: &FileManifest) -> &'static str {
    if crate::is_preload(&m.adapter) {
        RUNTIME_SIDE
    } else {
        GAME_SIDE
    }
}

/// Enabled claimant wins; otherwise the lowest instance id.
fn pick_claimant<'a>(
    others: &[&'a FileManifest],
    pred: impl Fn(&FileManifest) -> bool,
) -> Option<&'a FileManifest> {
    let mut claimants: Vec<&FileManifest> = others.iter().copied().filter(|m| pred(m)).collect();
    claimants.sort_by(|a, b| a.instance.cmp(&b.instance));
    claimants
        .iter()
        .copied()
        .find(|m| m.enabled)
        .or_else(|| claimants.first().copied())
}

fn live_root_for(pool_root: Option<&Path>, runtime: &Path, m: &FileManifest) -> Result<PathBuf> {
    if crate::is_preload(&m.adapter) {
        return Ok(runtime.to_path_buf());
    }
    pool_root
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::MissingExe(m.game.clone()))
}

fn owns_enabled_dest(m: &FileManifest, rel: &str) -> bool {
    m.files.iter().any(|f| f.enabled && f.dest == rel)
}

/// Generated files this instance claims: leave one an enabled instance still
/// uses, move one only a disabled instance claims into that park (live bytes
/// replace a parked copy), and delete the rest. A pure runtime dest a
/// disabled instance still has enabled moves into that instance's runtime
/// park. `remove_runtime_dests` deletes whatever of those dests is left.
fn settle_live(
    data_dir: &Path,
    game: &str,
    root: &Path,
    runtime: &Path,
    game_dir: Option<&Path>,
    mine: &Claim,
    others: &[&FileManifest],
    restored: &std::collections::BTreeSet<String>,
    remove_root: bool,
    settle_dests: bool,
) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut parents = Vec::new();
    for rel in crate::download::walk_game_root(root) {
        crate::stage::check_rel(&rel)?;
        if restored.contains(&rel) {
            continue;
        }
        let src = root.join(&rel);
        if !plain_file(&src) {
            continue;
        }
        let name = base_name(&rel);
        if matches_any(&mine.globs, name) {
            if let Some(target) = pick_claimant(others, |m| matches_any(&globs_of(m), name)) {
                if target.enabled {
                    let live = live_root_for(game_dir, runtime, target)?;
                    if live.as_path() != root {
                        move_unless_dest(&src, &live.join(&rel))?;
                        remember_parent(&mut parents, root, &src);
                    }
                } else {
                    let dst = disabled_root(data_dir, game, &target.instance)?
                        .join(adapter_side(target))
                        .join(&rel);
                    move_replacing(&src, &dst)?;
                    remember_parent(&mut parents, root, &src);
                }
            } else if let Some(target) = pick_claimant(others, |m| owns_enabled_dest(m, &rel)) {
                // No glob claimant. A disabled instance that still has this
                // path enabled gets the file; an enabled dest owner keeps it.
                if !target.enabled {
                    let dst = disabled_root(data_dir, game, &target.instance)?
                        .join(RUNTIME_SIDE)
                        .join(&rel);
                    move_replacing(&src, &dst)?;
                    remember_parent(&mut parents, root, &src);
                }
            } else {
                fs::remove_file(&src)?;
                remember_parent(&mut parents, root, &src);
            }
            continue;
        }
        if settle_dests && mine.dests.contains(&rel) {
            let Some(target) = pick_claimant(others, |m| owns_enabled_dest(m, &rel)) else {
                continue;
            };
            if !target.enabled {
                let dst = disabled_root(data_dir, game, &target.instance)?
                    .join(RUNTIME_SIDE)
                    .join(&rel);
                move_replacing(&src, &dst)?;
                remember_parent(&mut parents, root, &src);
            }
        }
    }
    sweep_parents(root, parents, remove_root);
    Ok(())
}

/// Move parked generated files another remaining instance still claims, then
/// delete the park. A pure dest in the park is this instance's copy; staging
/// still holds those bytes, so it is not handed off. A generated dest
/// (OptiScaler.ini) is handed off like any other generated file. An enabled
/// claimant keeps a live file it already has. A disabled claimant keeps the
/// copy already in its park; a live file moved by [`settle_live`] replaces it.
fn handoff_park(
    data_dir: &Path,
    game: &str,
    park: &Path,
    runtime: &Path,
    game_dir: Option<&Path>,
    mine: &Claim,
    others: &[&FileManifest],
) -> Result<()> {
    if !park.is_dir() {
        return Ok(());
    }
    for side in [RUNTIME_SIDE, GAME_SIDE] {
        let from = park.join(side);
        if !from.is_dir() {
            continue;
        }
        for rel in crate::download::walk_game_root(&from) {
            crate::stage::check_rel(&rel)?;
            let name = base_name(&rel);
            if mine.dests.contains(&rel) && !matches_any(&mine.globs, name) {
                continue;
            }
            let Some(target) = pick_claimant(others, |m| matches_any(&globs_of(m), name)) else {
                continue;
            };
            let src = from.join(&rel);
            if target.enabled {
                let live = live_root_for(game_dir, runtime, target)?;
                move_unless_dest(&src, &live.join(&rel))?;
            } else {
                let dst = disabled_root(data_dir, game, &target.instance)?
                    .join(adapter_side(target))
                    .join(&rel);
                move_unless_dest(&src, &dst)?;
            }
        }
    }
    fs::remove_dir_all(park)?;
    if let Some(parent) = park.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}

pub(in crate::mods) async fn drop_disabled(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    restored: &std::collections::BTreeSet<String>,
) -> Result<()> {
    let (manifests, mine) = load_mine(data_dir, game, instance)?;
    let others: Vec<&FileManifest> = manifests
        .iter()
        .filter(|m| m.instance != instance)
        .collect();
    let mine_claim = claim_of(&mine);
    let runtime = crate::stage::runtime_dir(data_dir, game);
    let needs_game =
        crate::is_install(&mine.adapter) || others.iter().any(|m| crate::is_install(&m.adapter));
    let game_dir = if needs_game {
        Some(crate::install::game_root(pool, game).await?)
    } else {
        None
    };
    handoff_park(
        data_dir,
        game,
        &disabled_root(data_dir, game, instance)?,
        &runtime,
        game_dir.as_deref(),
        &mine_claim,
        &others,
    )?;
    // Same rule as park: `restored` is the game dir remove_copies rewrote.
    let empty = std::collections::BTreeSet::new();
    let runtime_restored = if game_dir.as_deref() == Some(runtime.as_path()) {
        restored
    } else {
        &empty
    };
    settle_live(
        data_dir,
        game,
        &runtime,
        &runtime,
        game_dir.as_deref(),
        &mine_claim,
        &others,
        runtime_restored,
        true,
        true,
    )?;
    if crate::is_install(&mine.adapter) {
        if let Some(root) = game_dir.as_deref() {
            if root != runtime {
                settle_live(
                    data_dir,
                    game,
                    root,
                    &runtime,
                    game_dir.as_deref(),
                    &mine_claim,
                    &others,
                    restored,
                    false,
                    false,
                )?;
            }
        }
    }
    tracing::debug!(game, instance, "dropped disabled files");
    Ok(())
}
