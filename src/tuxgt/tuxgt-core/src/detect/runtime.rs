use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use keyvalues_parser::{parse, Value};

use super::binary::BinaryKind;
use super::{DetectCtx, Detected, Detector};
use crate::provider::heroic;
use crate::provider::StoreSnap;

struct SteamIndex {
    proton: BTreeMap<String, String>,
    launch: BTreeMap<String, String>,
    prefix: BTreeMap<String, PathBuf>,
}

fn steam_index() -> &'static SteamIndex {
    static INDEX: OnceLock<SteamIndex> = OnceLock::new();
    INDEX.get_or_init(build_steam_index)
}

fn build_steam_index() -> SteamIndex {
    let mut idx = SteamIndex {
        proton: BTreeMap::new(),
        launch: BTreeMap::new(),
        prefix: BTreeMap::new(),
    };
    let dirs = match steamlocate::locate_all() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = %e, "steam not found for detect");
            return idx;
        }
    };
    let mut launch_mtime: BTreeMap<String, SystemTime> = BTreeMap::new();
    for dir in &dirs {
        let root = dir.path();
        ingest_compat(&mut idx.prefix, root);
        if let Ok(libs) = dir.libraries() {
            for lib in libs.flatten() {
                ingest_compat(&mut idx.prefix, lib.path());
            }
        }
        let cfg = root.join("config").join("config.vdf");
        if let Some(v) = parse_vdf(&cfg) {
            if let Some(mapping) = find_obj(&v, "CompatToolMapping") {
                for (k, vals) in mapping.iter() {
                    if let Some(name) = vals
                        .first()
                        .and_then(|c| c.get_obj())
                        .and_then(|o| obj_child_str(o, "name"))
                    {
                        idx.proton.entry(k.to_string()).or_insert(name);
                    }
                }
            }
        }
        let userdata = root.join("userdata");
        if let Ok(users) = std::fs::read_dir(&userdata) {
            for user in users.flatten() {
                let lp = user.path().join("config").join("localconfig.vdf");
                let mtime = std::fs::metadata(&lp).and_then(|m| m.modified()).ok();
                if let Some(v) = parse_vdf(&lp) {
                    if let Some(apps) = find_obj(&v, "apps") {
                        for (k, vals) in apps.iter() {
                            let Some(obj) = vals.first().and_then(|c| c.get_obj()) else {
                                continue;
                            };
                            let opt = obj_child(obj, "LaunchOptions")
                                .and_then(|v| v.get_str())
                                .map(|s| s.to_string());
                            let newer = match (mtime, launch_mtime.get(k.as_ref())) {
                                (Some(t), Some(old)) => t >= *old,
                                (Some(_), None) => true,
                                (None, Some(_)) => false,
                                (None, None) => !idx.launch.contains_key(k.as_ref()),
                            };
                            if newer {
                                match opt.filter(|s| !s.is_empty()) {
                                    Some(s) => {
                                        idx.launch.insert(k.to_string(), s);
                                    }
                                    None => {
                                        idx.launch.remove(k.as_ref());
                                    }
                                }
                                if let Some(t) = mtime {
                                    launch_mtime.insert(k.to_string(), t);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    idx
}

fn ingest_compat(map: &mut BTreeMap<String, PathBuf>, root: &Path) {
    let dir = root.join("steamapps").join("compatdata");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            let name = ent.file_name().to_string_lossy().into_owned();
            map.entry(name).or_insert(p);
        }
    }
}

pub fn steam_store(app: &str) -> StoreSnap {
    let idx = steam_index();
    let mut snap = StoreSnap::default();
    snap.prefix_path = idx.prefix.get(app).cloned();
    snap.proton = idx.proton.get(app).cloned();
    snap.launch_options = idx.launch.get(app).cloned();
    if snap.proton.is_none() {
        if let Some(p) = &snap.prefix_path {
            if let Some(ver) = std::fs::read_to_string(p.join("version"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                snap.proton = Some(ver);
            }
        }
    }
    snap
}

pub struct Runtime;

impl Detector for Runtime {
    fn id(&self) -> &'static str {
        "runtime"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        match ctx.id.manager.as_str() {
            "steam" => steam(ctx, out, self.id()),
            "heroic" => heroic_rt(ctx, out, self.id()),
            _ => {}
        }
    }
}

fn steam(ctx: &DetectCtx, out: &mut Detected, src: &'static str) {
    let s = steam_store(&ctx.id.game);
    let pe = ctx
        .binary
        .map(|b| b.kind == BinaryKind::Pe)
        .unwrap_or(false);
    let elf = ctx
        .binary
        .map(|b| b.kind == BinaryKind::Elf)
        .unwrap_or(false);
    if elf {
        return;
    }
    if out.platform.as_deref() == Some("native") {
        return;
    }
    if pe || s.prefix_path.is_some() || s.proton.is_some() {
        out.set_platform("proton", src);
    }
}

fn heroic_rt(ctx: &DetectCtx, out: &mut Detected, src: &'static str) {
    let app = &ctx.id.game;
    let mut snap = None;
    for root in heroic::config_roots() {
        if let Some(s) = heroic::launch_snap(&root, app, ctx.install_dir) {
            snap = Some(s);
            break;
        }
    }
    let Some(s) = snap else {
        return;
    };
    let wine_type = s.wine_type.clone();
    let elf = ctx
        .binary
        .map(|b| b.kind == BinaryKind::Elf)
        .unwrap_or(false);
    if elf {
        return;
    }
    if out.platform.as_deref() == Some("native") {
        return;
    }
    let plat = wine_type
        .as_deref()
        .map(|t| {
            if t.eq_ignore_ascii_case("proton") {
                "proton"
            } else {
                "wine"
            }
        })
        .unwrap_or("wine");
    out.set_platform(plat, src);
}

fn parse_vdf(path: &Path) -> Option<Value<'static>> {
    let text = std::fs::read_to_string(path).ok()?;
    match parse(&text) {
        Ok(v) => Some(v.into_vdf().into_owned().value),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "vdf unreadable");
            None
        }
    }
}

#[cfg(test)]
pub(super) fn vdf_compat_name(path: &Path, app: &str) -> Option<String> {
    let v = parse_vdf(path)?;
    let mapping = find_obj(&v, "CompatToolMapping")?;
    obj_child(mapping, app)
        .and_then(|c| c.get_obj())
        .and_then(|o| obj_child_str(o, "name"))
}

#[cfg(test)]
pub(super) fn vdf_launch_options(path: &Path, app: &str) -> Option<String> {
    let v = parse_vdf(path)?;
    let apps = find_obj(&v, "apps")?;
    obj_child(apps, app)
        .and_then(|c| c.get_obj())
        .and_then(|o| obj_child_str(o, "LaunchOptions"))
}

fn find_obj<'a>(v: &'a Value<'a>, key: &str) -> Option<&'a keyvalues_parser::Obj<'a>> {
    match v {
        Value::Obj(obj) => {
            if let Some(c) = obj_child(obj, key) {
                if let Some(o) = c.get_obj() {
                    return Some(o);
                }
            }
            for (_, vals) in obj.iter() {
                for val in vals {
                    if let Some(o) = find_obj(val, key) {
                        return Some(o);
                    }
                }
            }
            None
        }
        Value::Str(_) => None,
    }
}

fn obj_child<'a>(obj: &'a keyvalues_parser::Obj<'a>, key: &str) -> Option<&'a Value<'a>> {
    obj.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .and_then(|(_, v)| v.first())
}

fn obj_child_str(obj: &keyvalues_parser::Obj, key: &str) -> Option<String> {
    obj_child(obj, key)
        .and_then(|v| v.get_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}
