use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::catalog::{read_disabled, write_disabled};
use super::{
    arch_slug, export_recipe_text, list_mods, package_slug, parse_recipe, user_mods_dir, Mod,
    SourceRef,
};

/// R38 legacy rename: an un-suffixed 64-bit user recipe minted from `pkg`
/// (id `B`, source matching the package 64-bit URL) becomes `B-x64`.
///
/// The rename covers the recipe file, the sibling payload dir, the
/// `mods.toml` disabled entry, every manifest with `instance == B`, and
/// every user-recipe `requires` entry `== B`. Everything is validated
/// before anything is written: if `B-x64` is taken anywhere (listed id,
/// recipe file, payload dir, manifest) the rename refuses and all files
/// stay unchanged. Returns the new id, or `None` when no legacy recipe
/// is present. A same-slug recipe from another source is not legacy and
/// is left alone.
pub fn migrate_reshade_legacy(
    config_dir: &Path,
    data_dir: &Path,
    pkg: &crate::download::ReshadePackage,
) -> Result<Option<String>> {
    let Ok(base) = package_slug(&pkg.name) else {
        return Ok(None);
    };
    let qualified = arch_slug(&base, "64")?;
    if base == qualified {
        return Ok(None);
    }
    let listed = list_mods(config_dir, data_dir)?;
    let legacy = listed.mods.iter().find(|m| {
        m.id == base && !m.official && m.registry.is_none() && legacy_source_matches(pkg, m)
    });
    let Some(legacy) = legacy.cloned() else {
        return Ok(None);
    };
    if listed.mods.iter().any(|m| m.id == qualified) {
        return Err(conflict(&base, &qualified));
    }
    let user_dir = user_mods_dir(data_dir);
    if user_dir.join(format!("{qualified}.toml")).exists() || user_dir.join(&qualified).exists() {
        return Err(conflict(&base, &qualified));
    }
    // Validate every write before the first one: manifests, then the
    // recipe text, then the requires edges.
    let all = all_manifests(data_dir);
    if all.iter().any(|(_, m)| m.instance == qualified) {
        return Err(conflict(&base, &qualified));
    }
    let recipe_text = export_recipe_text(&renamed(&legacy, &base, &qualified), data_dir, None)?;
    let mut requires: Vec<(PathBuf, Mod)> = Vec::new();
    for (path, m) in user_recipes(&user_dir) {
        if m.id != legacy.id && m.requires.iter().any(|r| r == &base) {
            let mut fixed = m.clone();
            fixed.requires = fixed
                .requires
                .iter()
                .map(|r| {
                    if r == &base {
                        qualified.clone()
                    } else {
                        r.clone()
                    }
                })
                .collect();
            requires.push((path, fixed));
        }
    }
    for (_, m) in &requires {
        export_recipe_text(m, data_dir, None)?;
    }
    // Manifests first: the new file lands before the old one drops.
    for (path, m) in all.iter().filter(|(_, m)| m.instance == base) {
        let mut next = m.clone();
        next.instance.clone_from(&qualified);
        let wrote = crate::download::write_manifest(data_dir, &next)?;
        if wrote != *path {
            fs::remove_file(path)?;
        }
    }
    let new_recipe = user_dir.join(format!("{qualified}.toml"));
    fs::write(&new_recipe, recipe_text)?;
    fs::remove_file(user_dir.join(format!("{base}.toml")))?;
    let old_payload = user_dir.join(&base);
    if old_payload.is_dir() {
        fs::rename(&old_payload, user_dir.join(&qualified))?;
    }
    let mut disabled = read_disabled(config_dir)?;
    if disabled.remove(&base) {
        disabled.insert(qualified.clone());
        write_disabled(config_dir, &disabled)?;
    }
    for (path, m) in &requires {
        let text = export_recipe_text(m, data_dir, None)?;
        fs::write(path, text)?;
    }
    crate::db::mark_cache_dirty();
    Ok(Some(qualified))
}

/// Legacy source check: does `m` hold the package's 64-bit payload?
/// Same URL rules as the per-arch lock, but without the base-id skip —
/// the candidate IS the base id.
fn legacy_source_matches(pkg: &crate::download::ReshadePackage, m: &Mod) -> bool {
    let Some(url) = pkg.url.as_deref() else {
        return false;
    };
    match &m.source {
        SourceRef::ManualUrl { url: held } => held == url,
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            ..
        } => {
            let gh = pkg
                .repository_url
                .as_deref()
                .and_then(crate::download::github_owner_repo)
                .or_else(|| crate::download::github_owner_repo(url));
            let asset = crate::download::parse_github_release_url(url);
            match (gh, asset) {
                (Some((o, r)), Some((_, _, _, a))) => {
                    o.eq_ignore_ascii_case(owner)
                        && r.eq_ignore_ascii_case(repo)
                        && (crate::download::glob_match(asset_glob, &a) || asset_glob == &a)
                }
                (Some((o, r)), None) => {
                    o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(repo)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn conflict(base: &str, qualified: &str) -> Error {
    Error::InvalidInstance(format!(
        "{qualified}: id already listed; cannot migrate {base}"
    ))
}

/// The legacy recipe re-identified, with its own `requires` edge (a
/// self-require) following the id.
fn renamed(legacy: &Mod, base: &str, qualified: &str) -> Mod {
    let mut next = legacy.clone();
    next.id = qualified.to_string();
    next.requires = next
        .requires
        .iter()
        .map(|r| {
            if r == base {
                qualified.to_string()
            } else {
                r.clone()
            }
        })
        .collect();
    next
}

/// Every parseable manifest under `$PREFIX/games/**/manifests/*.toml`.
/// Unreadable files warn-and-skip like `game_manifests`.
fn all_manifests(data_dir: &Path) -> Vec<(PathBuf, crate::download::FileManifest)> {
    let mut out = Vec::new();
    let mut stack = vec![data_dir.join("games")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let in_manifests = p
                .parent()
                .and_then(|d| d.file_name())
                .is_some_and(|n| n == crate::download::MANIFESTS_DIR);
            if !in_manifests || !p.extension().is_some_and(|x| x == "toml") {
                continue;
            }
            match fs::read_to_string(&p)
                .ok()
                .and_then(|t| toml::from_str::<crate::download::FileManifest>(&t).ok())
            {
                Some(m) => out.push((p, m)),
                None => tracing::warn!(file = %p.display(), "bad manifest"),
            }
        }
    }
    out
}

/// Every parseable user recipe. Broken files stay catalog problems;
/// they cannot carry a reliable `requires` edge.
fn user_recipes(user_dir: &Path) -> Vec<(PathBuf, Mod)> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(user_dir) else {
        return out;
    };
    let mut names: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    names.sort();
    for path in names {
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(file = %path.display(), "unreadable recipe: {e}");
                continue;
            }
        };
        match parse_recipe(&text, false) {
            Ok(m) => out.push((path, m)),
            Err(e) => tracing::warn!(file = %path.display(), "bad recipe: {e}"),
        }
    }
    out
}
