use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::*;
use crate::{parse_mod_type, Error, Result};

pub(crate) fn merge_rescan(
    scanned: Vec<PackageFile>,
    prev_keep: &[String],
    prev_dests: &BTreeMap<String, String>,
) -> Vec<PackageFile> {
    scanned
        .into_iter()
        .map(|mut f| {
            let was_kept = prev_keep.iter().any(|k| k == &f.src);
            if was_kept {
                f.keep = true;
                f.is_new = false;
                if let Some(d) = prev_dests.get(&f.src) {
                    f.dest = d.clone();
                }
            } else {
                // Type-dropped files stay dropped and are not "new".
                f.is_new = f.keep;
            }
            f
        })
        .collect()
}

pub(crate) fn existing_keep(inst: &Mod) -> Vec<String> {
    let mut keep = Vec::new();
    for r in &inst.payload {
        keep.extend(r.keep.iter().cloned());
    }
    keep.sort();
    keep.dedup();
    keep
}

pub(crate) fn files_to_keep_dests(
    mod_type: &str,
    files: &[PackageFile],
) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let mut keep = Vec::new();
    let mut dests = BTreeMap::new();
    let mut dest_seen = BTreeSet::new();
    for f in files {
        if !f.keep {
            continue;
        }
        if f.dest.trim().is_empty() {
            return Err(Error::InvalidInstance(format!("empty dest for {}", f.src)));
        }
        if !dest_seen.insert(f.dest.clone()) {
            return Err(Error::InvalidInstance(format!(
                "duplicate dest: {}",
                f.dest
            )));
        }
        keep.push(f.src.clone());
        if f.dest != f.src || default_dest(mod_type, &f.src) != f.src {
            dests.insert(f.src.clone(), f.dest.clone());
        }
    }
    if keep.is_empty() {
        return Err(Error::InvalidInstance("no files kept".into()));
    }
    Ok((keep, dests))
}

#[derive(Serialize)]
pub(crate) struct RecipeToml {
    id: String,
    #[serde(rename = "type")]
    mod_type: String,
    label: String,
    plans_allowed: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    games: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    payload: Vec<PayloadRule>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    dests: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    include: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    effect_files: Vec<String>,
    source: RecipeSourceOut,
}

#[derive(Serialize)]
pub(crate) struct RecipeSourceOut {
    #[serde(rename = "type")]
    source_type: &'static str,
    path: String,
}

pub(crate) fn write_user_recipe(path: &Path, inst: &Mod) -> Result<()> {
    let SourceRef::Local { path: src_path } = &inst.source else {
        return Err(Error::InvalidInstance(
            "from-package writes local sources only".into(),
        ));
    };
    let out = RecipeToml {
        id: inst.id.clone(),
        mod_type: inst.mod_type.clone(),
        label: inst.label.clone(),
        plans_allowed: inst
            .plans_allowed
            .iter()
            .map(|p| p.as_str().into())
            .collect(),
        games: inst.games.to_vec(),
        requires: inst.requires.to_vec(),
        payload: inst.payload.to_vec(),
        dests: inst.dests.clone(),
        slot: inst.slot.clone(),
        include: inst.include.to_vec(),
        env: inst.env.clone(),
        sha256: inst.sha256.clone(),
        effect_files: inst.effect_files.to_vec(),
        source: RecipeSourceOut {
            source_type: "local",
            path: src_path.clone(),
        },
    };
    let text = toml::to_string(&out).map_err(|e| Error::Toml(e.to_string()))?;
    fs::write(path, text)?;
    // Owner mark: recipe add/rescan funnel through here, so the next shared
    // open rebuilds the mod cache.
    crate::db::mark_cache_dirty();
    Ok(())
}

pub(crate) fn local_abs(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| Error::InvalidInstance(format!("{}: {e}", path.display())))
}

pub(crate) fn mod_from_files(
    id: &str,
    mod_type: &str,
    label: &str,
    abs: &Path,
    files: &[PackageFile],
    games: Vec<String>,
    include: Vec<String>,
    requires: Vec<String>,
) -> Result<Mod> {
    if !valid_id(id) {
        return Err(Error::InvalidInstance(format!("bad id: {id}")));
    }
    parse_mod_type(mod_type)?;
    let (keep, dests) = files_to_keep_dests(mod_type, files)?;
    let final_dests: Vec<String> = keep
        .iter()
        .map(|src| {
            dests
                .get(src)
                .cloned()
                .unwrap_or_else(|| default_dest(mod_type, src))
        })
        .collect();
    let slot = infer_slot(final_dests.iter(), &include);
    Ok(Mod {
        id: id.into(),
        mod_type: mod_type.into(),
        label: label.into(),
        source: SourceRef::Local {
            path: abs.to_string_lossy().into_owned(),
        },
        plans_allowed: vec![Plan::Install, Plan::Preload].into_boxed_slice(),
        sha256: None,
        payload: vec![PayloadRule {
            arch: None,
            api: None,
            keep: keep.into_boxed_slice(),
            drop: Box::default(),
        }]
        .into_boxed_slice(),
        games: games.into_boxed_slice(),
        appids: Box::default(),
        requires: requires.into_boxed_slice(),
        dests,
        slot,
        include: include.into_boxed_slice(),
        env: BTreeMap::new(),
        shader_dir: None,
        texture_dir: None,
        effect_files: Box::default(),
        official: false,
        registry: None,
        enabled: true,
    })
}
