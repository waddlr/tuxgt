use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;
use crate::game::{existing_path, STANDALONE};
use crate::provider::StoreSnap;

pub(crate) fn launch_snap(root: &Path, app: &str, install_dir: Option<&Path>) -> Option<StoreSnap> {
    let path = root.join("GamesConfig").join(format!("{app}.json"));
    let v = read_json(&path)?;
    let settings = v.get(app).cloned().or_else(|| {
        if v.get("winePrefix").is_some() || v.get("wineVersion").is_some() {
            Some(v.clone())
        } else {
            v.as_object().and_then(|o| {
                o.values()
                    .find(|x| x.get("winePrefix").is_some() || x.get("targetExe").is_some())
                    .cloned()
            })
        }
    })?;
    let mut snap = StoreSnap::default();
    if let Some(p) = settings.get("winePrefix").and_then(Value::as_str) {
        if !p.is_empty() {
            snap.prefix_path = Some(PathBuf::from(p));
        }
    }
    if let Some(wv) = settings.get("wineVersion") {
        snap.proton = wv
            .get("name")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        snap.wine_type = wv
            .get("type")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
    }
    if let Some(exe) = settings.get("targetExe").and_then(Value::as_str) {
        if !exe.is_empty() {
            let p = PathBuf::from(exe);
            snap.exe_path = if p.is_file() {
                Some(p)
            } else {
                install_dir.map(|dir| dir.join(&p)).filter(|j| j.is_file())
            };
        }
    }
    snap.launch_options = settings
        .get("launcherArgs")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    if let Some(arr) = settings.get("enviromentOptions").and_then(Value::as_array) {
        let mut map = serde_json::Map::new();
        for e in arr {
            let k = e.get("key").and_then(Value::as_str).unwrap_or("");
            let val = e.get("value").and_then(Value::as_str).unwrap_or("");
            if !k.is_empty() {
                map.insert(k.to_string(), Value::String(val.to_string()));
            }
        }
        if !map.is_empty() {
            snap.env = Some(Value::Object(map).to_string());
        }
    }
    if let Some(arr) = settings.get("wrapperOptions").and_then(Value::as_array) {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|w| {
                let exe = w.get("exe").and_then(Value::as_str)?;
                let args = w.get("args").and_then(Value::as_str).unwrap_or("");
                if exe.is_empty() {
                    None
                } else if args.is_empty() {
                    Some(exe.to_string())
                } else {
                    Some(format!("{exe} {args}"))
                }
            })
            .collect();
        if !parts.is_empty() {
            snap.wrapper = Some(parts.join("; "));
        }
    }
    snap.build = settings
        .get("version")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    Some(snap)
}

/// Live store snapshot for About refresh after Apply/Restore: scan-time
/// rows go stale the moment the store file changes. `install_dir` is only
/// needed for exe resolution, so `None` is fine here.
pub fn heroic_live_config(app: &str) -> Option<StoreSnap> {
    config_roots()
        .iter()
        .filter_map(|root| launch_snap(root, app, None))
        .next()
}

pub(crate) fn store_of(runner: &str) -> Option<&'static str> {
    match runner {
        "gog" => Some("gog"),
        "legendary" | "epic" => Some("epic"),
        "nile" | "amazon" => Some("amazon"),
        "sideload" => Some(STANDALONE),
        _ => None,
    }
}

pub(crate) fn games_array(v: &Value) -> Vec<&Value> {
    if let Some(a) = v.get("games").and_then(Value::as_array) {
        return a.iter().collect();
    }
    if let Some(a) = v.get("library").and_then(Value::as_array) {
        return a.iter().collect();
    }
    if let Some(a) = v.as_array() {
        return a.iter().collect();
    }
    Vec::new()
}

pub(crate) fn json_build(g: &Value) -> Option<String> {
    g.get("buildId")
        .or_else(|| g.get("build_id"))
        .or_else(|| g.get("version"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .filter(|s| !s.is_empty())
}

pub(crate) fn installed_entries(
    v: &Value,
) -> Vec<(String, String, Option<PathBuf>, Option<String>, bool)> {
    let mut out = Vec::new();
    if let Some(arr) = v.get("installed").and_then(Value::as_array) {
        for g in arr {
            push_installed(&mut out, g);
        }
        return out;
    }
    if let Some(map) = v.as_object() {
        if map.contains_key("games") || map.contains_key("library") {
            return out;
        }
        for (k, g) in map {
            if k == "version" || k == "__internal__" {
                continue;
            }
            if g.is_object() {
                let app = g
                    .get("app_name")
                    .or_else(|| g.get("appName"))
                    .and_then(Value::as_str)
                    .unwrap_or(k)
                    .to_string();
                let title = g
                    .get("title")
                    .or_else(|| g.get("install_path"))
                    .and_then(Value::as_str)
                    .unwrap_or(&app)
                    .to_string();
                let dir = install_path_from(g);
                out.push((app, title, dir, json_build(g), json_hidden(g)));
            }
        }
    }
    out
}

pub(crate) fn push_installed(
    out: &mut Vec<(String, String, Option<PathBuf>, Option<String>, bool)>,
    g: &Value,
) {
    let Some(app) = g
        .get("app_name")
        .or_else(|| g.get("appName"))
        .and_then(Value::as_str)
    else {
        return;
    };
    let title = g
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(app)
        .to_string();
    out.push((
        app.to_string(),
        title,
        install_path_from(g),
        json_build(g),
        json_hidden(g),
    ));
}

pub(crate) fn install_path_from(g: &Value) -> Option<PathBuf> {
    let s = g
        .get("install_path")
        .and_then(Value::as_str)
        .or_else(|| {
            g.get("install")
                .and_then(Value::as_object)
                .and_then(|o| o.get("install_path").and_then(Value::as_str))
        })
        .or_else(|| g.get("folder_name").and_then(Value::as_str))?;
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    if p.is_dir() || p.is_file() {
        Some(if p.is_file() {
            p.parent().unwrap_or(&p).to_path_buf()
        } else {
            p
        })
    } else {
        Some(p)
    }
}

/// Cover/header source: a local file that exists, or a remote URL the GUI
/// fetches lazily into the download cache (B01). Never fetch here.
pub(crate) fn art_path(v: Option<&Value>) -> Option<PathBuf> {
    let s = v.and_then(Value::as_str)?;
    let s = s.strip_prefix("file://").unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Some(PathBuf::from(s));
    }
    existing_path(PathBuf::from(s))
}

pub(crate) fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "heroic json unreadable");
            None
        }
    }
}
