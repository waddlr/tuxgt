use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::Result;

pub(crate) fn walk_game_root(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 4 || out.len() >= 20000 {
            continue;
        }
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            if ent.file_type().is_ok_and(|t| t.is_symlink()) {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let p = ent.path();
            if p.is_dir() {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
            } else if p.is_file() {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

/// Harvest roots for a game: the managed `<game>/runtime/` dir first when it
/// exists on disk, then the install-adapter `game_root` when it resolves
/// and differs. Unparseable game ids yield no roots.
pub async fn harvest_roots(
    pool: &sqlx::SqlitePool,
    data_dir: &Path,
    game_id: &str,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Ok(id) = crate::game::GameId::parse(game_id) else {
        return roots;
    };
    let runtime = crate::game::game_dir(data_dir, &id).join("runtime");
    if runtime.exists() {
        roots.push(runtime.clone());
    }
    if let Ok(root) = crate::install::game_root(pool, game_id).await {
        if Some(&root) != roots.first() && root != runtime {
            roots.push(root);
        }
    }
    roots
}

/// Harvest one game's runtime-generated files across several roots into its
/// manifests. Backfills empty `generated_globs` from the mod type so pre-E18
/// installs harvest on next scan. Walks each root, merges hits into one
/// `harvested` map (first root wins on rel collision), writes the manifest
/// once when globs or map changed. Returns absolute paths found, resolved
/// against the winning root.
pub fn harvest_game_roots(
    data_dir: &Path,
    game_id: &str,
    roots: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut manifests = game_manifests(data_dir, game_id)?;
    if manifests.is_empty() {
        return Ok(Vec::new());
    }
    let walked: Vec<Vec<String>> = roots.iter().map(|r| walk_game_root(r)).collect();
    let mut found = Vec::new();
    for m in manifests.iter_mut() {
        // Backfill once: persist only when the type actually yields globs,
        // so unknown-type manifests are not rewritten on every scan.
        let mut globs_changed = false;
        if m.generated_globs.is_empty() {
            let dests: Vec<&str> = m.files.iter().map(|f| f.dest.as_str()).collect();
            let globs = generated_globs_for(&m.mod_type, &dests);
            globs_changed = !globs.is_empty();
            m.generated_globs = globs.into();
        }
        let mut harvested = std::collections::BTreeMap::new();
        let mut winners: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for (idx, files) in walked.iter().enumerate() {
            for rel in files {
                if harvested.contains_key(rel) {
                    continue;
                }
                let name = rel.rsplit('/').next().unwrap_or(rel);
                if m.generated_globs.iter().any(|g| glob_match(g, name)) {
                    if let Ok(sha) = sha256_file(&roots[idx].join(rel)) {
                        harvested.insert(rel.clone(), sha);
                        winners.insert(rel.clone(), idx);
                    }
                }
            }
        }
        if globs_changed || harvested != m.harvested {
            m.harvested = harvested;
            write_manifest(data_dir, m)?;
        }
        found.extend(
            m.harvested
                .keys()
                .map(|rel| roots[*winners.get(rel).unwrap_or(&0)].join(rel)),
        );
    }
    tracing::info!(game = %game_id, harvested = found.len(), "harvested");
    Ok(found)
}

/// Harvest one game's runtime-generated files into its manifests.
/// Backfills empty `generated_globs` from the mod type so pre-E18
/// installs harvest on next scan. Returns absolute paths found.
pub fn harvest_game(data_dir: &Path, game_id: &str, root: &Path) -> Result<Vec<PathBuf>> {
    harvest_game_roots(data_dir, game_id, &[root.to_path_buf()])
}

/// Harvest every game with manifests. Best-effort per game (a missing
/// game dir warns, never fails the scan). Returns games with hits.
pub async fn harvest_all(
    pool: &sqlx::SqlitePool,
    data_dir: &Path,
) -> Result<Vec<(String, Vec<PathBuf>)>> {
    let games_root = data_dir.join("games");
    let Ok(l1) = fs::read_dir(&games_root) else {
        return Ok(Vec::new());
    };
    let mut games = std::collections::BTreeSet::new();
    for a in l1.flatten() {
        let Ok(l2) = fs::read_dir(a.path()) else {
            continue;
        };
        for b in l2.flatten() {
            let mdir = b.path().join(MANIFESTS_DIR);
            let Ok(rd) = fs::read_dir(&mdir) else {
                continue;
            };
            for ent in rd.flatten() {
                let p = ent.path();
                if p.extension().map_or(true, |e| e != "toml") {
                    continue;
                }
                let Ok(text) = fs::read_to_string(&p) else {
                    continue;
                };
                let Ok(v): std::result::Result<toml::Value, _> = toml::from_str(&text) else {
                    continue;
                };
                if let Some(game) = v.get("game").and_then(|g| g.as_str()) {
                    games.insert(game.to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    for game in games {
        let roots = harvest_roots(pool, data_dir, &game).await;
        if roots.is_empty() {
            tracing::warn!(game = %game, "harvest skipped");
            continue;
        }
        match harvest_game_roots(data_dir, &game, &roots) {
            Ok(files) if !files.is_empty() => out.push((game, files)),
            Ok(_) => {}
            Err(e) => tracing::warn!(game = %game, error = %e, "harvest failed"),
        }
    }
    Ok(out)
}
