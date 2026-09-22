//! Park a disabled instance outside the live roots.
//!
//! Disable moves that instance's dests and generated files from
//! `<game>/runtime/` (and, for the install adapter, generated files from
//! the game dir, including a generated dest whose bytes no longer match
//! tracked content) to `<game>/disabled/<instance>/{runtime,game}/`. Enable
//! moves them back onto the root the current adapter launches from.
//! Uninstall deletes the park and generated files no remaining instance
//! claims. Staging is untouched, so install-adapter dests still come back
//! from there.

mod drop;

pub(in crate::mods) use drop::drop_disabled;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::{Error, FileManifest, Result};

const RUNTIME_SIDE: &str = "runtime";
const GAME_SIDE: &str = "game";

fn globs_of(m: &FileManifest) -> BTreeSet<String> {
    if !m.generated_globs.is_empty() {
        return m.generated_globs.iter().cloned().collect();
    }
    let dests: Vec<&str> = m.files.iter().map(|f| f.dest.as_str()).collect();
    crate::generated_globs_for(&m.mod_type, &dests)
        .into_iter()
        .collect()
}

fn matches_any(globs: &BTreeSet<String>, name: &str) -> bool {
    globs.iter().any(|g| crate::download::glob_match(g, name))
}

pub(in crate::mods) fn claims_generated(m: &FileManifest, rel: &str) -> bool {
    matches_any(&globs_of(m), base_name(rel))
}

fn base_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn disabled_root(data_dir: &Path, game: &str, instance: &str) -> Result<PathBuf> {
    crate::stage::check_rel(instance)?;
    let base = match crate::game::GameId::parse(game) {
        Ok(id) => crate::game::game_dir(data_dir, &id),
        Err(_) => data_dir.join("games").join(crate::stage::game_safe(game)),
    };
    Ok(base.join("disabled").join(instance))
}

fn plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
}

/// Move `src` onto `dst`, replacing an existing file. Rename replaces
/// atomically on the same filesystem. Across devices, copy to a sibling
/// temp and rename that over `dst` so a failed copy leaves the old file.
/// A directory or symlink at `dst` is an error and nothing is removed.
fn move_replacing(src: &Path, dst: &Path) -> Result<()> {
    if !plain_file(src) {
        return Ok(());
    }
    if dst
        .symlink_metadata()
        .is_ok_and(|m| !m.file_type().is_file())
    {
        return Err(Error::NotAFile(dst.display().to_string()));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let name = dst.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let tmp = dst.with_file_name(format!(".{name}.tuxgt-park"));
    fs::copy(src, &tmp)?;
    if let Err(e) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    fs::remove_file(src)?;
    Ok(())
}

/// Move `src` onto `dst` only when `dst` is absent. An existing file wins
/// and the parked copy is dropped.
fn move_unless_dest(src: &Path, dst: &Path) -> Result<()> {
    if !plain_file(src) {
        return Ok(());
    }
    if dst.exists() {
        if !plain_file(dst) {
            return Err(Error::NotAFile(dst.display().to_string()));
        }
        fs::remove_file(src)?;
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(src, dst).is_err() {
        fs::copy(src, dst)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

fn rmdir_up(stop: &Path, from: &Path, include_stop: bool) {
    let mut cur = from.to_path_buf();
    loop {
        if cur == stop && !include_stop {
            break;
        }
        if !cur.starts_with(stop) {
            break;
        }
        if fs::remove_dir(&cur).is_err() {
            break;
        }
        if cur == stop {
            break;
        }
        let Some(parent) = cur.parent() else {
            break;
        };
        cur = parent.to_path_buf();
    }
}

fn remember_parent(parents: &mut Vec<PathBuf>, root: &Path, file: &Path) {
    if let Some(parent) = file.parent() {
        if parent.starts_with(root) {
            parents.push(parent.to_path_buf());
        }
    }
}

fn sweep_parents(root: &Path, parents: Vec<PathBuf>, include_root: bool) {
    let mut parents = parents;
    parents.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    parents.dedup();
    for parent in parents {
        rmdir_up(root, &parent, include_root);
    }
}

struct Claim {
    dests: BTreeSet<String>,
    globs: BTreeSet<String>,
}

fn claim_of(m: &FileManifest) -> Claim {
    Claim {
        dests: m.files.iter().map(|f| f.dest.clone()).collect(),
        globs: globs_of(m),
    }
}

fn load_mine(
    data_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<(Vec<FileManifest>, FileManifest)> {
    let manifests = crate::game_manifests(data_dir, game)?;
    let mine = manifests
        .iter()
        .find(|m| m.instance == instance)
        .cloned()
        .ok_or_else(|| Error::NoManifest(format!("{game} {instance}")))?;
    Ok((manifests, mine))
}

/// `move_dests` is false for the game dir: install-adapter dest copies are
/// already removed, and a foreign pure dest left behind must stay. A
/// generated file that is also a dest is still parked from that root.
fn park_tree(
    from: &Path,
    into: &Path,
    mine: &Claim,
    keep_dests: &BTreeSet<String>,
    keep_globs: &BTreeSet<String>,
    restored: &BTreeSet<String>,
    move_dests: bool,
    remove_root: bool,
) -> Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    let mut parents = Vec::new();
    for rel in crate::download::walk_game_root(from) {
        crate::stage::check_rel(&rel)?;
        // A backup remove_copies just put back is the pre-TuxGT file.
        if restored.contains(&rel) {
            continue;
        }
        let name = base_name(&rel);
        let is_dest = mine.dests.contains(&rel);
        let mine_glob = matches_any(&mine.globs, name);
        let other_glob = matches_any(keep_globs, name);
        // A generated dest still moves with the dests. From the game dir
        // (`move_dests` false) only that generated match is parked, which
        // is a tuned ini remove_copies refused. Another enabled instance's
        // dest keep applies on both paths. A pure dest DLL stays.
        let held = keep_dests.contains(&rel) || (mine_glob && other_glob);
        let take_dest = move_dests && is_dest && !held;
        let take_gen =
            mine_glob && !other_glob && !keep_dests.contains(&rel) && (!is_dest || !move_dests);
        if !take_dest && !take_gen {
            continue;
        }
        let src = from.join(&rel);
        move_replacing(&src, &into.join(&rel))?;
        remember_parent(&mut parents, from, &src);
    }
    sweep_parents(from, parents, remove_root);
    Ok(())
}

pub(super) async fn park_disabled(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
    restored: &BTreeSet<String>,
) -> Result<()> {
    let (manifests, mine) = load_mine(data_dir, game, instance)?;
    let keep_dests: BTreeSet<String> = manifests
        .iter()
        .filter(|m| m.instance != instance && m.enabled)
        .flat_map(|m| m.files.iter().filter(|f| f.enabled).map(|f| f.dest.clone()))
        .collect();
    let keep_globs: BTreeSet<String> = manifests
        .iter()
        .filter(|m| m.instance != instance && m.enabled)
        .flat_map(globs_of)
        .collect();
    let mine_claim = claim_of(&mine);
    let park = disabled_root(data_dir, game, instance)?;
    let runtime = crate::stage::runtime_dir(data_dir, game);
    // `restored` names files remove_copies rewrote in the game dir. A
    // same-named copy still under runtime (preload-to-install leftover)
    // is parked unless that directory is the game dir.
    let empty = BTreeSet::new();
    if crate::is_install(&mine.adapter) {
        let root = crate::install::game_root(pool, game).await?;
        if root == runtime {
            park_tree(
                &runtime,
                &park.join(RUNTIME_SIDE),
                &mine_claim,
                &keep_dests,
                &keep_globs,
                restored,
                true,
                true,
            )?;
        } else {
            park_tree(
                &runtime,
                &park.join(RUNTIME_SIDE),
                &mine_claim,
                &keep_dests,
                &keep_globs,
                &empty,
                true,
                true,
            )?;
            park_tree(
                &root,
                &park.join(GAME_SIDE),
                &mine_claim,
                &keep_dests,
                &keep_globs,
                restored,
                false,
                false,
            )?;
        }
    } else {
        park_tree(
            &runtime,
            &park.join(RUNTIME_SIDE),
            &mine_claim,
            &keep_dests,
            &keep_globs,
            &empty,
            true,
            true,
        )?;
    }
    tracing::debug!(game, instance, "parked disabled files");
    Ok(())
}

fn restore_tree(
    from: &Path,
    to: &Path,
    take: &impl Fn(&str) -> bool,
    replace: &impl Fn(&str) -> bool,
) -> Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    let mut parents = Vec::new();
    for rel in crate::download::walk_game_root(from) {
        crate::stage::check_rel(&rel)?;
        if !take(&rel) {
            continue;
        }
        let src = from.join(&rel);
        let dst = to.join(&rel);
        if replace(&rel) {
            move_replacing(&src, &dst)?;
        } else {
            move_unless_dest(&src, &dst)?;
        }
        remember_parent(&mut parents, from, &src);
    }
    sweep_parents(from, parents, true);
    Ok(())
}

pub(super) async fn restore_disabled(
    pool: &SqlitePool,
    data_dir: &Path,
    game: &str,
    instance: &str,
) -> Result<()> {
    let (_, mine) = load_mine(data_dir, game, instance)?;
    let park = disabled_root(data_dir, game, instance)?;
    if !park.exists() {
        return Ok(());
    }
    let globs = claim_of(&mine).globs;
    let generated = |rel: &str| matches_any(&globs, base_name(rel));
    let all = |_: &str| true;
    let runtime = crate::stage::runtime_dir(data_dir, game);
    if crate::is_preload(&mine.adapter) {
        // Later call wins. Runtime is the root this adapter launches from,
        // so a leftover game-side copy must not clobber it. Generated
        // bytes replace a file recreated while disabled.
        restore_tree(&park.join(GAME_SIDE), &runtime, &generated, &generated)?;
        restore_tree(&park.join(RUNTIME_SIDE), &runtime, &all, &generated)?;
    } else {
        let root = crate::install::game_root(pool, game).await?;
        // Game dir wins over a runtime copy of the same rel, and over the
        // stock ini apply_copies just wrote. Pure dest DLLs stay parked.
        restore_tree(&park.join(RUNTIME_SIDE), &root, &generated, &generated)?;
        restore_tree(&park.join(GAME_SIDE), &root, &generated, &generated)?;
    }
    let _ = fs::remove_dir(&park);
    if let Some(parent) = park.parent() {
        let _ = fs::remove_dir(parent);
    }
    tracing::debug!(game, instance, "restored disabled files");
    Ok(())
}
