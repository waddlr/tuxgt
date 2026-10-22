//! Load `share/args` and merge the rows that match one game.

use std::fs;
use std::path::{Path, PathBuf};

use super::rank::{check_same_rank, exe_stem};
use super::schema::{parse_catalog, parse_fixes, CatalogArg, CatalogFile, Trust};
use crate::game::{GameId, GameRow};
use crate::Result;

pub fn catalog_root(data_dir: &Path) -> PathBuf {
    data_dir.join("share").join("args")
}

/// Parse errors are logged and the file is skipped whole.
pub fn load_catalog(root: &Path) -> Vec<CatalogFile> {
    let mut out = Vec::new();
    for (rel, trust) in [
        ("curated/games", Trust::CuratedGame),
        ("curated/engines", Trust::CuratedEngine),
        ("pcgw/games", Trust::Pcgw),
        ("protondb/games", Trust::Protondb),
        ("lutris/games", Trust::Lutris),
    ] {
        let dir = root.join(rel);
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let label = path.to_string_lossy().into_owned();
            match parse_catalog(&text, &label, trust) {
                Ok(file) => out.push(file),
                Err(e) => tracing::warn!(path = %label, error = %e, "skipping arg catalog file"),
            }
        }
    }
    out
}

/// Validator entry for `args-check`: fail on the first schema or same-rank error.
pub fn check_tree(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    for (rel, trust) in [
        ("curated/games", Trust::CuratedGame),
        ("curated/engines", Trust::CuratedEngine),
        ("pcgw/games", Trust::Pcgw),
        ("protondb/games", Trust::Protondb),
        ("lutris/games", Trust::Lutris),
    ] {
        collect_args(&root.join(rel), trust, &mut files)?;
    }
    check_same_rank(&files)?;
    let dir = root.join("protonfixes").join("games");
    if dir.is_dir() {
        for path in toml_files(&dir) {
            let text = fs::read_to_string(&path)?;
            parse_fixes(&text, &path.to_string_lossy())?;
        }
    }
    Ok(())
}

fn collect_args(dir: &Path, trust: Trust, out: &mut Vec<CatalogFile>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for path in toml_files(dir) {
        let text = fs::read_to_string(&path)?;
        out.push(parse_catalog(&text, &path.to_string_lossy(), trust)?);
    }
    Ok(())
}

fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    paths
}

#[derive(Clone, Debug)]
pub struct GameIdentity {
    pub manager: String,
    pub store: String,
    pub game: String,
    pub steam_appid: Option<String>,
    pub exe_stem: Option<String>,
    pub engine: Option<String>,
}

impl GameIdentity {
    pub fn from_row(row: &GameRow) -> Self {
        let parsed = GameId::parse(&row.id).ok();
        let game = parsed.as_ref().map(|g| g.game.clone()).unwrap_or_default();
        let store = parsed
            .as_ref()
            .map(|g| g.store.clone())
            .unwrap_or_else(|| row.store.clone());
        Self {
            manager: row.manager.clone(),
            store,
            game,
            steam_appid: row.resolved_appid().map(|s| s.to_string()),
            exe_stem: row.exe_path.as_deref().map(exe_stem),
            engine: row.engine.clone().filter(|s| !s.is_empty()),
        }
    }
}

/// Highest trust wins each stem. Catalog order is curated game, then engine, then the rest.
pub fn matching_args(files: &[CatalogFile], id: &GameIdentity) -> Vec<CatalogArg> {
    let mut ranked: Vec<&CatalogFile> = files.iter().filter(|f| file_matches(f, id)).collect();
    ranked.sort_by_key(|f| f.trust);
    let mut out: Vec<CatalogArg> = Vec::new();
    for file in ranked {
        for arg in &file.args {
            if out.iter().any(|e| e.stem == arg.stem) {
                continue;
            }
            out.push(arg.clone());
        }
    }
    out
}

fn file_matches(file: &CatalogFile, id: &GameIdentity) -> bool {
    if let Some(name) = &file.engine {
        return id.engine.as_deref() == Some(name.as_str());
    }
    let Some(g) = &file.game else {
        return false;
    };
    if g.steam_appid
        .as_deref()
        .is_some_and(|a| id.steam_appid.as_deref() == Some(a))
    {
        return true;
    }
    if id.manager == "heroic" && id.store == "gog" && g.gog_id.as_deref() == Some(id.game.as_str())
    {
        return true;
    }
    if id.manager == "heroic"
        && matches!(id.store.as_str(), "legendary" | "epic" | "egs")
        && g.epic_id.as_deref() == Some(id.game.as_str())
    {
        return true;
    }
    if let (Some(engine), Some(exe)) = (&g.engine, &id.exe_stem) {
        if id.engine.as_deref() == Some(engine.as_str())
            && g.exe_names.iter().any(|n| exe_stem(n) == *exe)
        {
            return true;
        }
    }
    false
}

/// One fix-file stem for this row: steam appid on Steam, else the store game id.
pub fn fix_key(row: &GameRow) -> Option<(&'static str, String)> {
    let id = GameIdentity::from_row(row);
    match (id.manager.as_str(), id.store.as_str()) {
        ("steam", _) => id.steam_appid.map(|a| ("gamefixes-steam", a)),
        ("heroic", "gog") => Some(("gamefixes-gog", id.game)),
        ("heroic", "legendary" | "epic" | "egs") => Some(("gamefixes-egs", id.game)),
        _ => id.steam_appid.map(|a| ("gamefixes-steam", a)),
    }
}
