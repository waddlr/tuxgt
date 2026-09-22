use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

use super::{
    official_mods, official_mods_dir, parse_recipe, user_mods_dir, valid_registry, Mod, MODS_TOML,
};

pub(crate) fn disabled_path(config_dir: &Path) -> PathBuf {
    config_dir.join(MODS_TOML)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ModsFile {
    #[serde(default)]
    pub(crate) disabled: Box<[String]>,
}

pub(crate) fn read_disabled(config_dir: &Path) -> Result<BTreeSet<String>> {
    let path = disabled_path(config_dir);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let text = fs::read_to_string(path)?;
    let file: ModsFile = toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?;
    Ok(file.disabled.into_vec().into_iter().collect())
}

pub(crate) fn write_disabled(config_dir: &Path, disabled: &BTreeSet<String>) -> Result<()> {
    fs::create_dir_all(config_dir)?;
    let file = ModsFile {
        disabled: disabled
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    };
    let text = toml::to_string(&file).map_err(|e| Error::Toml(e.to_string()))?;
    fs::write(disabled_path(config_dir), text)?;
    Ok(())
}

/// Enable or disable a listed mod (including official). Unknown id errors.
pub fn set_mod_offered(config_dir: &Path, id: &str, enabled: bool, data_dir: &Path) -> Result<()> {
    let listed = list_mods(config_dir, data_dir)?;
    if !listed.mods.iter().any(|i| i.id == id) {
        return Err(Error::UnknownInstance(id.into()));
    }
    let mut disabled = read_disabled(config_dir)?;
    if enabled {
        disabled.remove(id);
    } else {
        disabled.insert(id.to_string());
    }
    write_disabled(config_dir, &disabled)
}

pub fn enable_mod(config_dir: &Path, id: &str, data_dir: &Path) -> Result<()> {
    set_mod_offered(config_dir, id, true, data_dir)
}

pub fn disable_mod(config_dir: &Path, id: &str, data_dir: &Path) -> Result<()> {
    set_mod_offered(config_dir, id, false, data_dir)
}

/// One user file that could not be listed. The rest of the list still stands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModProblem {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct ModList {
    pub mods: Vec<Mod>,
    pub problems: Vec<ModProblem>,
}

/// Officials first, then user recipes, then registry dirs. A bad file
/// never breaks the list. A user/registry file claiming an official id is
/// ignored — officials win.
pub fn list_mods(config_dir: &Path, data_dir: &Path) -> Result<ModList> {
    migrate_prefix(data_dir, config_dir);
    let officials = official_mods(&official_mods_dir(data_dir))?;
    let taken: BTreeSet<String> = officials.iter().map(|i| i.id.clone()).collect();
    let mut out = ModList {
        mods: officials,
        problems: Vec::new(),
    };
    load_recipe_dir(&user_mods_dir(data_dir), None, &taken, &mut out)?;
    let mut taken: BTreeSet<String> = out.mods.iter().map(|i| i.id.clone()).collect();
    let root = data_dir.join("mods");
    if root.is_dir() {
        let mut regs: Vec<PathBuf> = fs::read_dir(&root)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(valid_registry)
            })
            .collect();
        regs.sort();
        for dir in regs {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            load_recipe_dir(&dir, Some(&name), &taken, &mut out)?;
            taken.extend(out.mods.iter().map(|i| i.id.clone()));
        }
    }
    let disabled = read_disabled(config_dir)?;
    for m in &mut out.mods {
        m.enabled = !disabled.contains(&m.id);
    }
    Ok(out)
}

pub(crate) fn load_recipe_dir(
    dir: &Path,
    registry: Option<&str>,
    taken: &BTreeSet<String>,
    out: &mut ModList,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    files.sort();
    let mut loaded: Vec<(String, Mod)> = Vec::new();
    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                out.problems.push(ModProblem {
                    file: name,
                    reason: format!("unreadable: {e}"),
                });
                continue;
            }
        };
        let mut inst = match parse_recipe(&text, false) {
            Ok(i) => i,
            Err(e) => {
                out.problems.push(ModProblem {
                    file: name,
                    reason: e.to_string(),
                });
                continue;
            }
        };
        inst.registry = registry.map(str::to_string);
        if taken.contains(&inst.id) || out.mods.iter().any(|o| o.id == inst.id) {
            out.problems.push(ModProblem {
                file: name,
                reason: format!("{} shadows an official; ignoring", inst.id),
            });
            continue;
        }
        if loaded.iter().any(|(_, u)| u.id == inst.id) {
            out.problems.push(ModProblem {
                file: name,
                reason: format!("{} duplicates another file; ignoring", inst.id),
            });
            continue;
        }
        loaded.push((name, inst));
    }
    let known: BTreeSet<String> = out
        .mods
        .iter()
        .map(|o| o.id.clone())
        .chain(loaded.iter().map(|(_, u)| u.id.clone()))
        .collect();
    let mut kept: Vec<Mod> = Vec::with_capacity(loaded.len());
    for (file, inst) in loaded {
        match check_requires_known(&inst.id, &inst.requires, &known) {
            Ok(()) => kept.push(inst),
            Err(e) => out.problems.push(ModProblem {
                file,
                reason: e.to_string(),
            }),
        }
    }
    kept.sort_by(|a, b| a.id.cmp(&b.id));
    out.mods.extend(kept);
    Ok(())
}

/// `list_mods` filtered to the recipes applicable to a game and enabled.
/// A recipe with no `games` and no `appids` applies to every game;
/// otherwise it applies when its `games` globs match the display name
/// (case-insensitive) or any of its `appids` equals the game's resolved
/// Steam AppID (E63). Broken user files still surface as problems.
pub fn mods_for_game(
    config_dir: &Path,
    game_name: &str,
    steam_appid: Option<u32>,
    data_dir: &Path,
) -> Result<ModList> {
    let mut list = list_mods(config_dir, data_dir)?;
    let name = game_name.to_lowercase();
    list.mods.retain(|i| {
        let any_game = i.games.is_empty() && i.appids.is_empty();
        i.enabled
            && (any_game
                || i.games
                    .iter()
                    .any(|g| crate::download::glob_match(&g.to_lowercase(), &name))
                || (steam_appid.is_some() && i.appids.iter().any(|a| Some(*a) == steam_appid)))
    });
    Ok(list)
}
