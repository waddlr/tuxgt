use crate::{parse_mod_type, parse_slot, Error, Result};
use std::collections::BTreeSet;

use super::{infer_slot, valid_id, Mod, Plan, RecipeFile, SourceRef};

pub(crate) fn parse_recipe(text: &str, official: bool) -> Result<Mod> {
    let recipe: RecipeFile =
        toml::from_str(text).map_err(|e| Error::InvalidInstance(e.to_string()))?;
    if !valid_id(&recipe.id) {
        return Err(Error::InvalidInstance(format!("bad id: {}", recipe.id)));
    }
    parse_mod_type(&recipe.mod_type)?;
    if recipe.plans_allowed.is_empty() {
        return Err(Error::InvalidInstance("empty plans_allowed".into()));
    }
    let mut plans = Vec::with_capacity(recipe.plans_allowed.len());
    for p in &recipe.plans_allowed {
        let plan = Plan::parse(p)?;
        if plan == Plan::ProtonEnv && !official {
            return Err(Error::InvalidInstance(format!(
                "{}: proton_env is official-only",
                recipe.id
            )));
        }
        if !plans.contains(&plan) {
            plans.push(plan);
        }
    }
    let source = match recipe.source.source_type.as_str() {
        "github" => {
            let (Some(owner), Some(repo), Some(asset_glob)) = (
                recipe.source.owner,
                recipe.source.repo,
                recipe.source.asset_glob,
            ) else {
                return Err(Error::InvalidInstance(
                    "github source needs owner, repo, asset_glob".into(),
                ));
            };
            if let Some(tag) = recipe.source.tag.as_deref() {
                if tag.is_empty()
                    || !tag
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
                {
                    return Err(Error::InvalidInstance(format!("bad github tag: {tag}")));
                }
            }
            // E63: follow-latest contradicts a pin; a vanished pin would 404.
            if recipe.source.prerelease && recipe.source.tag.is_some() {
                return Err(Error::InvalidInstance(format!(
                    "{}: prerelease with a pinned tag; drop one of the two",
                    recipe.id
                )));
            }
            if recipe.source.prerelease && recipe.sha256.is_some() {
                return Err(Error::InvalidInstance(format!(
                    "{}: prerelease with a sha256 pin; a vanished pin would 404",
                    recipe.id
                )));
            }
            SourceRef::Github {
                owner,
                repo,
                asset_glob,
                tag: recipe.source.tag,
                prerelease: recipe.source.prerelease,
            }
        }
        "local" => {
            let Some(path) = recipe.source.path else {
                return Err(Error::InvalidInstance("local source needs path".into()));
            };
            if recipe.source.prerelease {
                return Err(Error::InvalidInstance(
                    "prerelease is a github-source key".into(),
                ));
            }
            SourceRef::Local { path }
        }
        "manual_url" => {
            let Some(url) = recipe.source.url else {
                return Err(Error::InvalidInstance("manual_url source needs url".into()));
            };
            if recipe.source.prerelease {
                return Err(Error::InvalidInstance(
                    "prerelease is a github-source key".into(),
                ));
            }
            SourceRef::ManualUrl { url }
        }
        other => {
            return Err(Error::InvalidInstance(format!(
                "unknown source type: {other}"
            )));
        }
    };
    for a in &recipe.appids {
        if *a == 0 {
            return Err(Error::InvalidInstance(format!(
                "{}: appids entries must be nonzero",
                recipe.id
            )));
        }
    }
    for rule in &recipe.payload {
        if let Some(arch) = rule.arch.as_deref() {
            if arch != "32" && arch != "64" {
                return Err(Error::InvalidInstance(format!(
                    "payload arch must be 32 or 64, got {arch}"
                )));
            }
        }
        if let Some(api) = rule.api.as_deref() {
            if api.is_empty() {
                return Err(Error::InvalidInstance(
                    "payload api must not be empty".into(),
                ));
            }
        }
    }
    let mut games = Vec::with_capacity(recipe.games.len());
    for g in &recipe.games {
        let g = g.trim();
        if g.is_empty() {
            return Err(Error::InvalidInstance(
                "games entries must not be empty".into(),
            ));
        }
        games.push(g.to_string());
    }
    for r in &recipe.requires {
        if !valid_id(r) {
            return Err(Error::InvalidInstance(format!("bad requires id: {r}")));
        }
        if *r == recipe.id {
            return Err(Error::InvalidInstance(format!(
                "{}: requires itself",
                recipe.id
            )));
        }
    }
    if let Some(slot) = recipe.slot.as_deref() {
        parse_slot(slot)?;
    }
    for inc in &recipe.include {
        if inc.is_empty() {
            return Err(Error::InvalidInstance(
                "include entries must not be empty".into(),
            ));
        }
    }
    for (src, dest) in &recipe.dests {
        if src.is_empty() || dest.is_empty() {
            return Err(Error::InvalidInstance(
                "dests entries must not be empty".into(),
            ));
        }
    }
    for (k, v) in &recipe.env {
        if k.trim().is_empty() || v.is_empty() {
            return Err(Error::InvalidInstance(
                "env keys and values must not be empty".into(),
            ));
        }
        if !crate::env::valid_env_key(k) {
            return Err(Error::InvalidInstance(format!("bad env key: {k}")));
        }
    }
    if recipe.mod_type != "effect" && recipe.mod_type != "texture" {
        if recipe.shader_dir.is_some() || recipe.texture_dir.is_some() {
            return Err(Error::InvalidInstance(format!(
                "{}: shader_dir/texture_dir are effect/texture only",
                recipe.id
            )));
        }
    }
    for (key, val) in [
        ("shader_dir", recipe.shader_dir.as_deref()),
        ("texture_dir", recipe.texture_dir.as_deref()),
    ] {
        if let Some(v) = val {
            if v.is_empty() || v.contains('/') || v.contains('\\') {
                return Err(Error::InvalidInstance(format!(
                    "{}: {key} must be one path component",
                    recipe.id
                )));
            }
        }
    }
    for f in &recipe.effect_files {
        if f.is_empty() || f.contains('/') || f.contains('\\') {
            return Err(Error::InvalidInstance(format!(
                "{}: effect_files entries are file names",
                recipe.id
            )));
        }
    }
    let slot = match recipe.slot {
        Some(s) => Some(s),
        None => infer_slot(recipe.dests.values(), &recipe.include),
    };
    Ok(Mod {
        id: recipe.id,
        mod_type: recipe.mod_type,
        label: recipe.label,
        source,
        plans_allowed: plans.into_boxed_slice(),
        sha256: recipe.sha256,
        payload: recipe.payload,
        games: games.into_boxed_slice(),
        appids: recipe.appids,
        requires: recipe.requires,
        dests: recipe.dests,
        slot,
        include: recipe.include,
        env: recipe.env,
        shader_dir: recipe.shader_dir,
        texture_dir: recipe.texture_dir,
        effect_files: recipe.effect_files,
        official,
        registry: None,
        enabled: true,
    })
}

/// Every `requires` id must exist in the catalog (officials + listed users).
pub(crate) fn check_requires_known(
    id: &str,
    requires: &[String],
    known: &BTreeSet<String>,
) -> Result<()> {
    for r in requires {
        if !known.contains(r) {
            return Err(Error::InvalidInstance(format!(
                "{id}: unknown requires {r}"
            )));
        }
    }
    Ok(())
}
