use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;
use crate::game::{GameId, STANDALONE};
use crate::provider::{GameRecord, StoreSnap};

pub(crate) fn config_roots() -> Vec<PathBuf> {
    default_roots()
}

pub(crate) fn default_roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    vec![
        home.join(".config/heroic"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/heroic"),
    ]
}

pub fn scan_roots(roots: &[PathBuf]) -> Vec<GameRecord> {
    let mut out: BTreeMap<String, GameRecord> = BTreeMap::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        scan_one(root, &mut out);
    }
    out.into_values().collect()
}

pub(crate) fn scan_one(root: &Path, out: &mut BTreeMap<String, GameRecord>) {
    let mut meta: BTreeMap<(String, String), Meta> = BTreeMap::new();
    merge_library(
        &mut meta,
        "gog",
        &read_json(&root.join("store_cache/gog_library.json")),
    );
    merge_library(
        &mut meta,
        "epic",
        &read_json(&root.join("store_cache/legendary_library.json")),
    );
    merge_library(
        &mut meta,
        "amazon",
        &read_json(&root.join("store_cache/nile_library.json")),
    );
    // `sideload` is Heroic's runner name here (store_of maps it); entries
    // without a runner in these files are sideloads by location.
    merge_library(
        &mut meta,
        "sideload",
        &read_json(&root.join("sideload_apps/library.json")),
    );
    merge_library(
        &mut meta,
        "sideload",
        &read_json(&root.join("store/sideload_apps/library.json")),
    );
    // Authoritative hidden list overlays every store by appName.
    let hidden_set = heroic_hidden_set(root);
    if !hidden_set.is_empty() {
        for ((_, app), m) in meta.iter_mut() {
            if hidden_set.contains(app) {
                m.hidden = true;
            }
        }
    }
    ingest_installed(
        out,
        &meta,
        "gog",
        &[
            root.join("gog_store/installed.json"),
            root.join("store/gog_store/installed.json"),
        ],
        root,
        &hidden_set,
    );
    ingest_installed(
        out,
        &meta,
        "epic",
        &[
            root.join("../legendary/installed.json"),
            root.parent()
                .unwrap_or(root)
                .join("legendary/installed.json"),
        ],
        root,
        &hidden_set,
    );
    ingest_installed(
        out,
        &meta,
        "amazon",
        &[
            root.join("nile_config/nile/installed.json"),
            root.join("store/nile/installed.json"),
        ],
        root,
        &hidden_set,
    );

    for ((store, app), m) in &meta {
        if *store == STANDALONE && m.installed {
            push_game(out, store, app, m, root);
        }
        if m.installed && (*store == "gog" || *store == "epic" || *store == "amazon") {
            let key = match GameId::new("heroic", store.as_str(), app.as_str()) {
                Ok(id) => id.to_string(),
                Err(_) => continue,
            };
            if !out.contains_key(&key) {
                push_game(out, store, app, m, root);
            }
        }
    }
}

pub(crate) struct Meta {
    title: String,
    install_dir: Option<PathBuf>,
    cover: Option<PathBuf>,
    header: Option<PathBuf>,
    installed: bool,
    build: Option<String>,
    hidden: bool,
}

/// Per-game hidden in Heroic JSON: `hidden` / `isHidden` / `is_hidden`
/// (bool, "1"/"true", or non-zero number). The authoritative source is
/// `store/config.json` `games.hidden[]`.
pub(crate) fn json_hidden(g: &serde_json::Value) -> bool {
    for key in ["hidden", "isHidden", "is_hidden", "isHiddenGame"] {
        let Some(v) = g.get(key) else {
            continue;
        };
        if v.as_bool().is_some_and(|b| b) {
            return true;
        }
        if v.as_str()
            .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        {
            return true;
        }
        if v.as_u64().is_some_and(|n| n != 0) {
            return true;
        }
        if v.as_i64().is_some_and(|n| n != 0) {
            return true;
        }
    }
    false
}

/// Authoritative Heroic hidden set for one config root:
/// `store/config.json` `games.hidden[]` (`{appName, title}` entries).
pub(crate) fn heroic_hidden_set(root: &std::path::Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for rel in ["store/config.json", "config.json"] {
        let Some(v) = read_json(&root.join(rel)) else {
            continue;
        };
        let hidden = v
            .get("games")
            .and_then(|g| g.get("hidden"))
            .and_then(|h| h.as_array());
        let Some(arr) = hidden else {
            continue;
        };
        for e in arr {
            if let Some(app) = e
                .get("appName")
                .or_else(|| e.get("app_name"))
                .and_then(|v| v.as_str())
            {
                if !app.is_empty() {
                    out.insert(app.to_string());
                }
            }
        }
    }
    out
}

pub(crate) fn merge_library(
    meta: &mut BTreeMap<(String, String), Meta>,
    default_store: &str,
    v: &Option<Value>,
) {
    let Some(v) = v else { return };
    for game in games_array(v) {
        let runner = game
            .get("runner")
            .and_then(Value::as_str)
            .unwrap_or(default_store);
        let Some(store) = store_of(runner) else {
            continue;
        };
        let Some(app) = game
            .get("app_name")
            .or_else(|| game.get("appName"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let title = game
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(app)
            .to_string();
        let installed = game
            .get("is_installed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let install_dir = install_path_from(game);
        let cover = art_path(game.get("art_square")).or_else(|| art_path(game.get("art_cover")));
        let header =
            art_path(game.get("art_cover")).or_else(|| art_path(game.get("art_background")));
        meta.insert(
            (store.to_string(), app.to_string()),
            Meta {
                title,
                install_dir,
                cover,
                header,
                installed,
                build: json_build(game),
                hidden: json_hidden(game),
            },
        );
    }
}

pub(crate) fn ingest_installed(
    out: &mut BTreeMap<String, GameRecord>,
    meta: &BTreeMap<(String, String), Meta>,
    store: &str,
    paths: &[PathBuf],
    root: &Path,
    hidden_set: &std::collections::HashSet<String>,
) {
    for p in paths {
        let Some(v) = read_json(p) else { continue };
        for (app, title, dir, build, installed_hidden) in installed_entries(&v) {
            let m = meta.get(&(store.to_string(), app.clone()));
            let rec_meta = Meta {
                title: m
                    .map(|x| x.title.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(title),
                install_dir: dir.or_else(|| m.and_then(|x| x.install_dir.clone())),
                cover: m.and_then(|x| x.cover.clone()),
                header: m.and_then(|x| x.header.clone()),
                installed: true,
                build,
                hidden: m.is_some_and(|x| x.hidden)
                    || installed_hidden
                    || hidden_set.contains(&app),
            };
            push_game(out, store, &app, &rec_meta, root);
        }
    }
}

pub(crate) fn push_game(
    out: &mut BTreeMap<String, GameRecord>,
    store: &str,
    app: &str,
    m: &Meta,
    root: &Path,
) {
    if app == "gog-redist" {
        return;
    }
    let Ok(id) = GameId::new("heroic", store, app) else {
        return;
    };
    let key = id.to_string();
    if out.contains_key(&key) {
        return;
    }
    let mut rec = GameRecord::new(id, m.title.clone());
    rec.install_dir = m.install_dir.clone();
    rec.cover_path = m.cover.clone();
    rec.header_path = m.header.clone();
    rec.build = m.build.clone();
    rec.hidden = m.hidden;
    let snap = launch_snap(root, app, rec.install_dir.as_deref());
    apply_snap(&mut rec, snap);
    out.insert(key, rec);
}

pub(crate) fn apply_snap(rec: &mut GameRecord, snap: Option<StoreSnap>) {
    let Some(s) = snap else {
        return;
    };
    if rec.exe_path.is_none() {
        rec.exe_path = s.exe_path;
    }
    rec.prefix_path = s.prefix_path.or(rec.prefix_path.take());
    rec.proton = s.proton.or(rec.proton.take());
    rec.build = rec.build.take().or(s.build);
    rec.launch_options = s.launch_options.or(rec.launch_options.take());
    rec.env = s.env.or(rec.env.take());
    rec.wrapper = s.wrapper.or(rec.wrapper.take());
}
