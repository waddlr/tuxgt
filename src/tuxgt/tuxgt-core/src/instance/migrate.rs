use std::fs;
use std::path::Path;

use crate::Result;

use super::{official_mods, official_mods_dir, user_mods_dir, valid_id};

pub(crate) fn official_ids(data_dir: &Path) -> Result<Vec<String>> {
    Ok(official_mods(&official_mods_dir(data_dir))?
        .into_iter()
        .map(|i| i.id)
        .collect())
}

pub(crate) fn valid_registry(name: &str) -> bool {
    name != "official" && name != "user" && valid_id(name)
}

/// One-shot PREFIX layout move: share/mods → mods/official, config/mods →
/// mods/user, packages/<id> → mods/user/<id>, unpack/<hash> → payload dir.
pub fn migrate_prefix(data_dir: &Path, config_dir: &Path) {
    let official = official_mods_dir(data_dir);
    let user = user_mods_dir(data_dir);
    let _ = fs::create_dir_all(&official);
    let _ = fs::create_dir_all(&user);
    move_tomls(&data_dir.join("share").join("mods"), &official);
    // Old user recipes lived under `<config>/mods/`. That path is the
    // new catalog when tests pass the same dir for config and data.
    let old_user = config_dir.join("mods");
    if old_user != data_dir.join("mods") {
        move_tomls(&old_user, &user);
    }
    let packages = data_dir.join("packages");
    if packages.is_dir() {
        if let Ok(rd) = fs::read_dir(&packages) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let dest = user.join(e.file_name());
                    if !dest.exists() {
                        let _ = fs::rename(&p, &dest);
                    }
                }
            }
        }
        let _ = fs::remove_dir_all(&packages);
    }
    migrate_unpack(data_dir);
}

pub(crate) fn move_tomls(from: &Path, to: &Path) {
    if !from.is_dir() {
        return;
    }
    if let Ok(rd) = fs::read_dir(from) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "toml") {
                let dest = to.join(e.file_name());
                if !dest.exists() {
                    let _ = fs::rename(&p, &dest).or_else(|_| {
                        fs::copy(&p, &dest).map(|_| {
                            let _ = fs::remove_file(&p);
                        })
                    });
                }
            }
        }
    }
    let _ = fs::remove_dir_all(from);
}

pub(crate) fn migrate_unpack(data_dir: &Path) {
    let unpack = data_dir.join("unpack");
    if !unpack.is_dir() {
        return;
    }
    let games = data_dir.join("games");
    if games.is_dir() {
        if let Ok(walk) = walk_manifests(&games) {
            for (id, key) in walk {
                let src = unpack.join(&key);
                if !src.is_dir() {
                    continue;
                }
                let official = official_mods_dir(data_dir)
                    .join(format!("{id}.toml"))
                    .is_file();
                let kind = if official { "official" } else { "user" };
                let dest = if official {
                    official_mods_dir(data_dir).join(&id)
                } else {
                    user_mods_dir(data_dir).join(&id)
                };
                if !dir_has_any(&dest) {
                    let _ = crate::download::copy_tree(&src, &dest);
                }
                rewrite_manifest_sources(data_dir, &id, kind, &key);
            }
        }
    }
    let _ = fs::remove_dir_all(&unpack);
}

pub(crate) fn rewrite_manifest_sources(data_dir: &Path, instance: &str, kind: &str, key: &str) {
    let games = data_dir.join("games");
    if !games.is_dir() {
        return;
    }
    fn walk(dir: &Path, instance: &str, kind: &str, key: &str) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, instance, kind, key);
            } else if p.extension().is_some_and(|x| x == "toml")
                && p.parent()
                    .and_then(|d| d.file_name())
                    .is_some_and(|n| n == "manifests")
            {
                let Ok(text) = fs::read_to_string(&p) else {
                    continue;
                };
                let Ok(mut m) = toml::from_str::<crate::FileManifest>(&text) else {
                    continue;
                };
                if m.instance != instance {
                    continue;
                }
                let mut changed = false;
                for f in &mut m.files {
                    if let Some(rest) = f.source.strip_prefix("cache/") {
                        if let Some((k, tail)) = rest.split_once('/') {
                            if k == key {
                                if let Some((_, rel)) = tail.split_once('#') {
                                    f.source = format!("mods/{kind}/{instance}/{rel}");
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                if changed {
                    if let Ok(out) = toml::to_string(&m) {
                        let _ = fs::write(&p, out);
                    }
                }
            }
        }
    }
    walk(&games, instance, kind, key);
}

pub(crate) fn dir_has_any(dir: &Path) -> bool {
    dir.is_dir()
        && fs::read_dir(dir)
            .ok()
            .is_some_and(|mut d| d.next().is_some())
}

pub(crate) fn walk_manifests(games: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) -> Result<()> {
        let rd = match fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.extension().is_some_and(|x| x == "toml")
                && p.parent()
                    .and_then(|d| d.file_name())
                    .is_some_and(|n| n == "manifests")
            {
                if let Ok(text) = fs::read_to_string(&p) {
                    if let Ok(m) = toml::from_str::<crate::FileManifest>(&text) {
                        for f in &m.files {
                            if let Some(rest) = f.source.strip_prefix("cache/") {
                                if let Some((key, _)) = rest.split_once('/') {
                                    out.push((m.instance.clone(), key.to_string()));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    walk(games, &mut out)?;
    Ok(out)
}
