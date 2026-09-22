use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use super::*;
use crate::apply::ApplyCtx;
use crate::game::GameId;
use crate::provider::{GameProvider, GameRecord};
use crate::{Error, Result};

pub struct SteamProvider;

pub(crate) const TOOL_APPIDS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    250820,  // SteamVR
    1070560, // Steam Linux Runtime
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1628350, // Steam Linux Runtime 3.0 (sniper)
    2180100, // Steam Linux Runtime 3.0 (sniper) extra / Proton Hotfix family
    1493710, // Proton Experimental
    1887720, // Proton 7.0
    2348590, // Proton 8.0
    2805730, // Proton 9.0
    3658110, // Proton 10
];

impl GameProvider for SteamProvider {
    fn plugin_id(&self) -> &'static str {
        "steam"
    }

    fn scan(&self) -> Result<Vec<GameRecord>> {
        let dirs = match steamlocate::locate_all() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "steam not found");
                return Ok(Vec::new());
            }
        };
        let roots: Vec<PathBuf> = dirs.iter().map(|d| d.path().to_path_buf()).collect();
        let owned_hidden = load_owned_hidden(&roots);
        let shortcut_hidden = load_shortcut_hidden(&roots);
        let ucollections_hidden = load_ucollections_hidden(&roots);
        let mut out: BTreeMap<String, GameRecord> = BTreeMap::new();
        for dir in dirs {
            scan_dir(
                &dir,
                &mut out,
                &owned_hidden,
                &shortcut_hidden,
                &ucollections_hidden,
            );
        }
        Ok(out.into_values().collect())
    }

    fn apply(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let gid = GameId::parse(game_id)?;
        if gid.manager != "steam" {
            return Err(Error::ApplyUnsupported("steam".into()));
        }
        let files = localconfig_files(&steam_roots());
        if files.is_empty() {
            return Err(Error::Apply(
                "no Steam login config (localconfig.vdf) found".into(),
            ));
        }
        apply_files(ctx, game_id, &gid.game, &files)
    }

    fn restore(&self, ctx: &ApplyCtx, game_id: &str) -> Result<String> {
        let gid = GameId::parse(game_id)?;
        if gid.manager != "steam" {
            return Err(Error::ApplyUnsupported("steam".into()));
        }
        restore_files(ctx, game_id, &gid.game)
    }
}

pub(crate) fn scan_dir(
    dir: &steamlocate::SteamDir,
    out: &mut BTreeMap<String, GameRecord>,
    owned_hidden: &HashSet<String>,
    shortcut_hidden: &HashMap<u32, bool>,
    ucollections_hidden: &HashSet<u32>,
) {
    match dir.libraries() {
        Ok(libs) => {
            for lib in libs {
                let lib = match lib {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::warn!(error = %e, "steam library unreadable");
                        continue;
                    }
                };
                for app in lib.apps() {
                    let app = match app {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::warn!(error = %e, "appmanifest unreadable");
                            continue;
                        }
                    };
                    let name = app.name.clone().unwrap_or_else(|| app.install_dir.clone());
                    if skip_steam_app(app.app_id, &name) {
                        continue;
                    }
                    let id = match GameId::new("steam", "", app.app_id.to_string()) {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    let key = id.to_string();
                    if out.contains_key(&key) {
                        continue;
                    }
                    let (cover, header) = steam_art(dir.path(), app.app_id);
                    let mut rec = GameRecord::new(id, name);
                    rec.install_dir = Some(lib.resolve_app_dir(&app));
                    rec.cover_path = cover;
                    rec.header_path = header;
                    rec.build = app.build_id.map(|b| b.to_string());
                    rec.hidden = owned_hidden.contains(&app.app_id.to_string())
                        || ucollections_hidden.contains(&app.app_id);
                    apply_steam_snap(&mut rec, &app.app_id.to_string());
                    out.insert(key, rec);
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "steam libraries unreadable"),
    }

    match dir.shortcuts() {
        Ok(iter) => {
            for sc in iter {
                let sc = match sc {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "shortcuts.vdf unreadable");
                        continue;
                    }
                };
                if sc.app_id == 0 {
                    tracing::warn!(name = %sc.app_name, "shortcut missing appid");
                    continue;
                }
                let id = match GameId::new("steam", crate::game::STANDALONE, sc.app_id.to_string())
                {
                    Ok(id) => id,
                    Err(_) => continue,
                };
                let display = id.to_string();
                if out.contains_key(&display) {
                    continue;
                }
                let install_dir = shortcut_dir(&sc.start_dir, &sc.executable);
                let cover = shortcut_art(dir.path(), sc.app_id);
                let mut rec = GameRecord::new(id, sc.app_name);
                rec.install_dir = install_dir;
                rec.cover_path = cover;
                rec.exe_path = shortcut_exe(&sc.executable);
                rec.hidden = shortcut_hidden.get(&sc.app_id).copied().unwrap_or(false)
                    || ucollections_hidden.contains(&sc.app_id);
                apply_steam_snap(&mut rec, &sc.app_id.to_string());
                out.insert(display, rec);
            }
        }
        Err(e) => tracing::warn!(error = %e, "steam shortcuts unreadable"),
    }
}

pub fn skip_steam_app(app_id: u32, name: &str) -> bool {
    if TOOL_APPIDS.contains(&app_id) {
        return true;
    }
    let n = name.to_ascii_lowercase();
    n.starts_with("proton")
        || n.starts_with("steam linux runtime")
        || n.contains("steamworks common redistributables")
        || n.starts_with("steamworks ")
        || n.contains("creation kit")
}
