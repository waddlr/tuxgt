use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::{Error, Result};

use super::{list_mods, user_mods_dir, valid_id, Mod};

/// Serialized minted family recipe (E63). GitHub source with the picked
/// literal asset name; `appids` or `games` per the AppID choice; the family
/// drop list only when non-empty (an all-empty rule would write an empty
/// `[[payload]]` table).
#[derive(Serialize)]
pub(crate) struct FamilyRecipeToml<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    mod_type: &'a str,
    label: &'a str,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    games: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    appids: Option<&'a [u32]>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    payload: &'a [PayloadRule],
    source: FamilySourceToml<'a>,
}

#[derive(Serialize)]
pub(crate) struct FamilySourceToml<'a> {
    #[serde(rename = "type")]
    source_type: &'static str,
    owner: &'a str,
    repo: &'a str,
    asset_glob: &'a str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    prerelease: bool,
}

/// One minted-recipe spec (T13): everything `mint_recipe` needs beyond the
/// dirs. Two constructors — `family` (E63) and `reshade_package` (E96) —
/// replace the separate mint fns; slug/collision/dest/write below is shared.
/// `drops` unifies the `family.drop` vs `deny_drop_globs` paths: each
/// constructor supplies plain globs, the shared build wraps them in one
/// payload rule.
pub struct RecipeSpec {
    slug_input: String,
    mod_type: String,
    label: String,
    source: SourceRef,
    drops: Vec<String>,
    games: Vec<String>,
    appids: Vec<u32>,
    shader_dir: Option<String>,
    texture_dir: Option<String>,
    effect_files: Vec<String>,
    kind: MintKind,
}

enum MintKind {
    /// E63 family mint: `asset` names the typed file (slug-error context)
    /// and probes same-asset collisions for -2/-3 suffixing.
    Family { asset: String },
    /// E96 extras mint: `pkg` feeds the repo+asset already-in-catalog check.
    Reshade {
        pkg: crate::download::ReshadePackage,
    },
}

impl RecipeSpec {
    /// E63 family spec: id slug from the asset stem; AppID given → appids,
    /// else a games glob from the title. Mode is the kind's default
    /// single-file addon flow (matching the old per-game rows).
    pub fn family(
        tpl: &ModTemplate,
        family: &TemplateFamily,
        asset: &str,
        title: &str,
        label: &str,
        appid: Option<u32>,
    ) -> Result<RecipeSpec> {
        if appid == Some(0) {
            return Err(Error::InvalidInstance("appid must be nonzero".into()));
        }
        let stem = std::path::Path::new(asset)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (games, appids): (Vec<String>, Vec<u32>) = match appid {
            Some(a) => (Vec::new(), vec![a]),
            None => (vec![format!("*{title}*")], Vec::new()),
        };
        Ok(RecipeSpec {
            slug_input: stem,
            mod_type: tpl.mod_type.clone(),
            label: label.to_string(),
            source: SourceRef::Github {
                owner: family.owner.clone(),
                repo: family.repo.clone(),
                asset_glob: asset.to_string(),
                tag: None,
                prerelease: family.prerelease,
            },
            drops: family.drop.to_vec(),
            games,
            appids,
            shader_dir: None,
            texture_dir: None,
            effect_files: Vec::new(),
            kind: MintKind::Family {
                asset: asset.to_string(),
            },
        })
    }

    /// E96 ReShade extras spec: id slug from the package name; no `games` /
    /// `appids`. Uses the listed snapshot; does not re-fetch the INIs.
    /// Display-only EffectFiles list; `DenyEffectFiles` already rides the
    /// drop globs, so the preview applies the recipe's own drops.
    pub fn reshade_package(pkg: &crate::download::ReshadePackage) -> Result<RecipeSpec> {
        Self::reshade_package_for_arch(pkg, "64")
    }

    /// Arch-aware variant: picks `DownloadUrl32` for 32-bit games (R38).
    pub fn reshade_package_for_arch(
        pkg: &crate::download::ReshadePackage,
        arch: &str,
    ) -> Result<RecipeSpec> {
        let url = pkg.url_for_arch(arch);
        let Some(url) = url else {
            return Err(Error::InvalidInstance(format!(
                "{}: no download URL",
                pkg.name
            )));
        };
        let url = url.as_str();
        let source = if let Some((owner, repo, tag, asset)) =
            crate::download::parse_github_release_url(url)
        {
            SourceRef::Github {
                owner,
                repo,
                asset_glob: asset,
                tag,
                prerelease: false,
            }
        } else {
            SourceRef::ManualUrl {
                url: url.to_string(),
            }
        };
        let effect = pkg.kind == crate::download::ReshadePackageKind::Effect;
        Ok(RecipeSpec {
            slug_input: pkg.name.clone(),
            mod_type: pkg.kind.mod_type().to_string(),
            label: pkg.name.clone(),
            source,
            drops: crate::download::deny_drop_globs(&pkg.deny_files),
            games: Vec::new(),
            appids: Vec::new(),
            shader_dir: if effect { pkg.shader_dir.clone() } else { None },
            texture_dir: if effect {
                pkg.texture_dir.clone()
            } else {
                None
            },
            effect_files: if effect {
                pkg.effect_files.to_vec()
            } else {
                Vec::new()
            },
            kind: MintKind::Reshade { pkg: pkg.clone() },
        })
    }
}

/// Look up one family template + its family table (mint preamble).
pub fn family_template(
    data_dir: &Path,
    template_id: &str,
) -> Result<(ModTemplate, TemplateFamily)> {
    let tpl = list_templates(data_dir)?
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| Error::InvalidInstance(format!("unknown template: {template_id}")))?;
    let family = tpl
        .family
        .clone()
        .ok_or_else(|| Error::InvalidInstance(format!("{template_id}: not a family template")))?;
    Ok((tpl, family))
}

/// Best-effort live case-snap of a family asset name (E63 pick 3): a listed
/// asset snaps to its real casing; the API failing or the name being
/// unlisted mints the typed name, and install surfaces an absent/renamed
/// asset itself.
pub async fn snap_family_asset(data_dir: &Path, template_id: &str, asset_name: &str) -> String {
    match crate::download::list_family_assets(data_dir, template_id).await {
        Ok(assets) => assets
            .into_iter()
            .find(|a| a.name.eq_ignore_ascii_case(asset_name))
            .map(|a| a.name)
            .unwrap_or_else(|| asset_name.to_string()),
        Err(e) => {
            tracing::warn!(
                template = template_id,
                error = %e,
                "family list unavailable; minting the typed asset name"
            );
            asset_name.to_string()
        }
    }
}

/// Mint one user Mod from a spec: slug → no-shadow → collision → dest →
/// write. Family collisions disambiguate distinct assets with -2/-3
/// suffixes; extras collisions refuse. The TOML schema stays per-kind
/// (family minimal recipe vs full export with shader dirs/effect lists),
/// so minted bytes are unchanged — only the pipeline is shared.
pub fn mint_recipe(config_dir: &Path, data_dir: &Path, spec: RecipeSpec) -> Result<Mod> {
    // Family slugs derive from the asset stem, so the error names the
    // typed asset; extras slugs derive from the package name itself.
    let slug0 = match &spec.kind {
        MintKind::Family { asset } => package_slug(&spec.slug_input).map_err(|_| {
            Error::InvalidInstance(format!("{asset}: id slug is not [a-z][a-z0-9-]{{0,31}}"))
        })?,
        MintKind::Reshade { .. } => package_slug(&spec.slug_input)?,
    };
    if official_ids(data_dir)?.iter().any(|o| o == &slug0) {
        return Err(Error::InvalidInstance(format!(
            "{slug0}: id shadows an official mod"
        )));
    }
    let listed = list_mods(config_dir, data_dir)?;
    if let MintKind::Reshade { pkg } = &spec.kind {
        if crate::download::package_in_catalog(pkg, &listed.mods) {
            return Err(Error::InvalidInstance(format!(
                "{}: already in catalog",
                pkg.name
            )));
        }
    }
    let slug = match &spec.kind {
        MintKind::Family { asset } => family_slug(&listed, &slug0, asset)?,
        MintKind::Reshade { .. } => {
            if listed.mods.iter().any(|i| i.id == slug0) {
                return Err(Error::InvalidInstance(format!(
                    "{slug0}: id already listed; no shadowing"
                )));
            }
            slug0.clone()
        }
    };
    let dest = user_mods_dir(data_dir).join(format!("{slug}.toml"));
    if dest.exists() {
        return Err(Error::InvalidInstance(format!(
            "{slug}: mod already exists"
        )));
    }
    let inst = minted_mod(&spec, &slug);
    let text = match &spec.kind {
        MintKind::Family { .. } => family_recipe_text(&inst, &spec)?,
        MintKind::Reshade { .. } => export_recipe_text(&inst, data_dir, None)?,
    };
    fs::create_dir_all(user_mods_dir(data_dir))?;
    fs::write(&dest, text)?;
    crate::db::mark_cache_dirty();
    Ok(inst)
}

/// Family collision rule: same-asset re-mint refuses; distinct assets
/// colliding on a listed id get -2/-3 suffixes (base truncated to fit).
fn family_slug(listed: &ModList, slug: &str, asset: &str) -> Result<String> {
    if !listed.mods.iter().any(|i| i.id == slug) {
        return Ok(slug.to_string());
    }
    // A listed id is either the same asset (already-in-catalog) or a
    // true collision: disambiguate distinct assets with -2/-3 suffixes.
    let same_asset = listed.mods.iter().any(|i| {
        i.id == slug
            && matches!(&i.source, SourceRef::Github { asset_glob, .. } if asset_glob == asset)
    });
    if same_asset {
        return Err(Error::InvalidInstance(format!(
            "{slug}: id already listed; no shadowing"
        )));
    }
    let mut n = 2u32;
    loop {
        let suffix = format!("-{n}");
        let allow = 32usize.saturating_sub(suffix.len());
        let mut base = slug.to_string();
        if base.len() > allow {
            base.truncate(allow);
            while base.ends_with('-') {
                base.pop();
            }
        }
        let cand = format!("{base}{suffix}");
        if valid_id(&cand) && !listed.mods.iter().any(|i| i.id == cand) {
            return Ok(cand);
        }
        n += 1;
        if n > 99 {
            return Err(Error::InvalidInstance(format!(
                "{slug}: id already listed; no shadowing"
            )));
        }
    }
}

/// Shared minted-Mod build: one payload rule from the spec drops (empty →
/// no rule), plans install+preload, user-owned, enabled.
fn minted_mod(spec: &RecipeSpec, slug: &str) -> Mod {
    let payload: Box<[PayloadRule]> = if spec.drops.is_empty() {
        Box::default()
    } else {
        vec![PayloadRule {
            arch: None,
            api: None,
            keep: Box::default(),
            drop: spec.drops.clone().into_boxed_slice(),
        }]
        .into_boxed_slice()
    };
    Mod {
        id: slug.to_string(),
        mod_type: spec.mod_type.clone(),
        label: spec.label.clone(),
        source: spec.source.clone(),
        plans_allowed: vec![Plan::Install, Plan::Preload].into_boxed_slice(),
        sha256: None,
        payload,
        games: spec.games.clone().into_boxed_slice(),
        appids: spec.appids.clone().into_boxed_slice(),
        requires: Box::default(),
        dests: BTreeMap::new(),
        slot: None,
        include: Box::default(),
        env: BTreeMap::new(),
        shader_dir: spec.shader_dir.clone(),
        texture_dir: spec.texture_dir.clone(),
        effect_files: spec.effect_files.clone().into_boxed_slice(),
        official: false,
        registry: None,
        enabled: true,
    }
}

/// Minimal family recipe TOML (E63): id/type/label, games or appids, the
/// drop rule only when non-empty, github source.
fn family_recipe_text(inst: &Mod, spec: &RecipeSpec) -> Result<String> {
    let SourceRef::Github {
        owner,
        repo,
        asset_glob,
        prerelease,
        ..
    } = &spec.source
    else {
        return Err(Error::InvalidInstance(format!(
            "{}: family mint needs a github source",
            inst.id
        )));
    };
    let out = FamilyRecipeToml {
        id: &inst.id,
        mod_type: &inst.mod_type,
        label: &inst.label,
        games: &inst.games,
        appids: (!inst.appids.is_empty()).then_some(&inst.appids[..]),
        payload: &inst.payload,
        source: FamilySourceToml {
            source_type: "github",
            owner,
            repo,
            asset_glob,
            prerelease: *prerelease,
        },
    };
    toml::to_string(&out).map_err(|e| Error::Toml(e.to_string()))
}
