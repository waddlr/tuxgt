use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{parse_mod_type, Error, Result};

use super::{parse_recipe, share_templates_dir, valid_id, Mod};

/// Read every official mod recipe from `mods/official/*.toml`, sorted by
/// file name. Payload dirs (`mods/official/<id>/`) are ignored. A missing
/// dir lists no officials; a corrupt file degrades to a problem row (like
/// user/registry files) so one bad official never breaks the list. Gone
/// files drop. Requires resolve within officials; a mod whose require did
/// not list degrades with it, transitively.
pub(crate) fn official_mods(share_dir: &Path) -> Result<(Vec<Mod>, Vec<ModProblem>)> {
    let mut loaded: Vec<(String, Mod)> = Vec::new();
    let mut problems: Vec<ModProblem> = Vec::new();
    if !share_dir.exists() {
        return Ok((Vec::new(), problems));
    }
    let mut files: Vec<PathBuf> = fs::read_dir(share_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                problems.push(ModProblem {
                    file: name,
                    reason: format!("unreadable: {e}"),
                });
                continue;
            }
        };
        match parse_recipe(&text, true) {
            Ok(i) => loaded.push((name, i)),
            Err(e) => problems.push(ModProblem {
                file: name,
                reason: e.to_string(),
            }),
        }
    }
    loop {
        let known: BTreeSet<String> = loaded.iter().map(|(_, i)| i.id.clone()).collect();
        let before = loaded.len();
        loaded.retain(
            |(name, i)| match check_requires_known(&i.id, &i.requires, &known) {
                Ok(()) => true,
                Err(e) => {
                    problems.push(ModProblem {
                        file: name.clone(),
                        reason: e.to_string(),
                    });
                    false
                }
            },
        );
        if loaded.len() == before {
            break;
        }
    }
    Ok((loaded.into_iter().map(|(_, i)| i).collect(), problems))
}

/// Every id the official dir claims, including degraded files: each file's
/// stem plus its `id` line when the file still parses as TOML (a poisoned
/// recipe usually keeps a valid id line while failing a later rule). A
/// broken official still reserves its id — officials win even when
/// unparseable, so no user or registry file can shadow it while it is broken.
pub(crate) fn official_reserved_ids(share_dir: &Path) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    if !share_dir.exists() {
        return Ok(out);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(share_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    for path in &files {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if valid_id(stem) {
                out.insert(stem.to_string());
            }
        }
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(table) = toml::from_str::<toml::Table>(&text) {
                if let Some(id) = table.get("id").and_then(|v| v.as_str()) {
                    if valid_id(id) {
                        out.insert(id.to_string());
                    }
                }
            }
        }
    }
    Ok(out)
}

/// A pre-filled Mod form (lock 10): Provides (`type`), entry mode, and
/// default Requires. Local file/folder source only; the GitHub-family mint
/// is E63. Packaged TOML, not a Rust table.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModTemplate {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub mod_type: String,
    /// `include` (single file becomes an IncludeFile dest basename),
    /// `type` (type dest rules: OptiScaler/ReShade file-or-folder),
    /// `custom` (blank Custom classify form). Family templates carry no
    /// mode: the mint flow writes the recipe, not a form.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub requires: Box<[String]>,
    /// E63: a per-game addon family (RenoDX, Luma). Only templates with
    /// this table feed the family Add flow; prefill skips them.
    #[serde(default)]
    pub family: Option<TemplateFamily>,
}

/// One addon family source (E63): the release repo, the asset filter and
/// the drop list minted recipes inherit. Families are data, not code.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateFamily {
    pub owner: String,
    pub repo: String,
    pub asset_glob: String,
    /// Follow the newest prerelease release instead of `/releases/latest`.
    #[serde(default)]
    pub prerelease: bool,
    /// Payload drop globs written into minted recipes.
    #[serde(default)]
    pub drop: Box<[String]>,
}

/// Every template in `$PREFIX/share/templates/*.toml`, sorted by file
/// name. A missing dir lists none. Corrupt files and unknown Provides or
/// mode values error (unlike official mods, which degrade to problems).
pub fn list_templates(data_dir: &Path) -> Result<Vec<ModTemplate>> {
    let dir = share_templates_dir(data_dir);
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    for path in &files {
        let text = fs::read_to_string(path)?;
        let t: ModTemplate =
            toml::from_str(&text).map_err(|e| Error::InvalidInstance(e.to_string()))?;
        parse_mod_type(&t.mod_type)?;
        if let Some(f) = &t.family {
            if f.owner.is_empty() || f.repo.is_empty() || f.asset_glob.is_empty() {
                return Err(Error::InvalidInstance(format!(
                    "{}: family needs owner, repo, asset_glob",
                    t.id
                )));
            }
            for d in &f.drop {
                if d.is_empty() {
                    return Err(Error::InvalidInstance(format!(
                        "{}: family drop entries must not be empty",
                        t.id
                    )));
                }
            }
        } else if t.mode != "include" && t.mode != "type" && t.mode != "custom" {
            return Err(Error::InvalidInstance(format!(
                "{}: unknown template mode: {}",
                t.id, t.mode
            )));
        }
        for r in &t.requires {
            if !valid_id(r) {
                return Err(Error::InvalidInstance(format!("bad requires id: {r}")));
            }
        }
        out.push(t);
    }
    Ok(out)
}
