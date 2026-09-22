use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::*;
use crate::apply::{
    drop_record, ensure_backup, now_unix, read_record, write_record, ApplyCtx, ApplyFile,
    ApplyRecord,
};
use crate::{atomic_write, Error, Result};

pub(crate) fn games_config(roots: &[PathBuf], app: &str) -> Option<PathBuf> {
    let rel = format!("GamesConfig/{app}.json");
    roots.iter().map(|r| r.join(&rel)).find(|p| p.is_file())
}

/// Resolve the settings object Apply edits, mirroring `launch_snap`'s
/// shapes: an object keyed by app id, else a flat top-level object.
/// Creates the keyed entry when the doc is an object without it.
pub(crate) fn heroic_target<'d>(
    doc: &'d mut Value,
    app: &str,
) -> Result<&'d mut Map<String, Value>> {
    if doc.get(app).is_some_and(|v| v.is_object()) {
        return doc
            .get_mut(app)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| Error::Apply("heroic config app entry is not an object".into()));
    }
    if doc.is_object() {
        if doc.get("winePrefix").is_some()
            || doc.get("wineVersion").is_some()
            || doc.get("targetExe").is_some()
        {
            return doc
                .as_object_mut()
                .ok_or_else(|| Error::Apply("heroic config is not an object".into()));
        }
        if let Some(obj) = doc.as_object_mut() {
            obj.insert(app.into(), Value::Object(Map::new()));
        }
        return doc
            .get_mut(app)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| Error::Apply("heroic config is not an object".into()));
    }
    Err(Error::Apply("heroic config is not a JSON object".into()))
}

pub(crate) fn launcher_present(wrappers: &[Value], launcher: &Path) -> bool {
    let want = launcher.file_name();
    wrappers.iter().any(|w| {
        w.get("exe").and_then(Value::as_str).is_some_and(|exe| {
            Path::new(exe) == launcher || (want.is_some() && Path::new(exe).file_name() == want)
        })
    })
}

/// Ensure the launcher wrapper in the settings object.
/// Returns the previous `wrapperOptions` snapshot (null = the key was
/// absent) for the apply record. Env lives in the launch session, not here.
pub(crate) fn heroic_ensure(target: &mut Map<String, Value>, ctx: &ApplyCtx) -> Value {
    let snapshot = Value::Object(
        [(
            "wrapperOptions".into(),
            target.get("wrapperOptions").cloned().unwrap_or(Value::Null),
        )]
        .into_iter()
        .collect(),
    );
    let wrappers = target
        .entry("wrapperOptions")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(arr) = wrappers.as_array_mut() {
        if !launcher_present(arr, &ctx.launcher) {
            arr.push(Value::Object(
                [
                    (
                        "exe".to_string(),
                        Value::String(ctx.launcher.to_string_lossy().into_owned()),
                    ),
                    ("args".to_string(), Value::String(String::new())),
                ]
                .into_iter()
                .collect(),
            ));
        }
    }
    snapshot
}

/// Restore keys present in the snapshot. Null = key was absent → remove.
/// Old records may still carry `enviromentOptions`.
pub(crate) fn heroic_unensure(target: &mut Map<String, Value>, snapshot: &Value) {
    let Some(obj) = snapshot.as_object() else {
        return;
    };
    for (key, prev) in obj {
        match prev {
            Value::Null => {
                target.remove(key);
            }
            prev => {
                target.insert(key.clone(), prev.clone());
            }
        }
    }
}

/// Strip our wrapper + managed env back out of an already-applied
/// snapshot (lost-record re-apply reconstruction, mirrors steam's
/// `strip_fragment`): storing the wrapped state as `previous` would make
/// restore a no-op. Empty-after-strip becomes `Null` (key was absent).
pub(crate) fn strip_managed(snapshot: &Value, ctx: &ApplyCtx) -> Value {
    let managed: Vec<String> = ctx.env_pairs().into_iter().map(|(k, _)| k).collect();
    let mut out = Map::new();
    match snapshot.get("wrapperOptions") {
        None | Some(Value::Null) => {
            out.insert("wrapperOptions".into(), Value::Null);
        }
        Some(Value::Array(arr)) => {
            let kept: Vec<Value> = arr
                .iter()
                .filter(|w| {
                    let exe = w.get("exe").and_then(Value::as_str).unwrap_or("");
                    !(Path::new(exe) == ctx.launcher
                        || (ctx.launcher.file_name().is_some()
                            && Path::new(exe).file_name() == ctx.launcher.file_name()))
                })
                .cloned()
                .collect();
            if kept.is_empty() {
                out.insert("wrapperOptions".into(), Value::Null);
            } else {
                out.insert("wrapperOptions".into(), Value::Array(kept));
            }
        }
        Some(other) => {
            out.insert("wrapperOptions".into(), other.clone());
        }
    }
    match snapshot.get("enviromentOptions") {
        None => {}
        Some(Value::Null) => {
            out.insert("enviromentOptions".into(), Value::Null);
        }
        Some(Value::Array(arr)) => {
            let kept: Vec<Value> = arr
                .iter()
                .filter(|e| {
                    !e.get("key")
                        .and_then(Value::as_str)
                        .is_some_and(|k| managed.iter().any(|m| m == k))
                })
                .cloned()
                .collect();
            if kept.is_empty() {
                out.insert("enviromentOptions".into(), Value::Null);
            } else {
                out.insert("enviromentOptions".into(), Value::Array(kept));
            }
        }
        Some(other) => {
            out.insert("enviromentOptions".into(), other.clone());
        }
    }
    Value::Object(out)
}

pub(crate) fn heroic_apply(ctx: &ApplyCtx, game_id: &str, app: &str) -> Result<String> {
    let roots = config_roots();
    if roots.is_empty() {
        return Err(Error::Apply("no Heroic config found".into()));
    }
    let path = games_config(&roots, app)
        .unwrap_or_else(|| roots[0].join("GamesConfig").join(format!("{app}.json")));
    let mut doc: Value = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?,
        Err(_) => Value::Object(Map::new()),
    };
    let snapshot = {
        let target = heroic_target(&mut doc, app)?;
        heroic_ensure(target, ctx)
    };
    let snapshot_text = snapshot.to_string();
    // Already applied when the pre-ensure state already names our wrapper.
    let pre: Value = serde_json::from_str(&snapshot_text)
        .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
    let already = pre
        .get("wrapperOptions")
        .and_then(Value::as_array)
        .is_some_and(|w| launcher_present(w, &ctx.launcher));
    let mut rec = read_record(&ctx.data_dir, game_id)?.unwrap_or(ApplyRecord {
        game: game_id.into(),
        manager: "heroic".into(),
        launcher: ctx.launcher.to_string_lossy().into_owned(),
        applied_at: now_unix(),
        files: Vec::new(),
    });
    let rel = path.to_string_lossy().into_owned();
    let known = rec.files.iter().any(|f| f.path == rel);
    if already && known {
        return Ok(format!("{game_id} already applied (1 file)"));
    }
    let mut backup: Option<String> = rec
        .files
        .iter()
        .find(|f| f.path == rel)
        .and_then(|f| f.backup.clone());
    if !already {
        if path.exists() {
            backup = Some(ensure_backup(
                &ctx.data_dir,
                game_id,
                &path,
                backup.as_deref(),
            )?);
        }
        let text = serde_json::to_string_pretty(&doc)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        atomic_write(&path, text.as_bytes())?;
    }
    match rec.files.iter_mut().find(|f| f.path == rel) {
        Some(entry) => {
            if entry.backup.is_none() {
                entry.backup = backup;
            }
        }
        None if already => {
            // Lost-record re-apply: `snapshot_text` already holds our
            // wrapper, so strip it back out (steam.rs `strip_fragment`
            // approach) — storing the wrapped state as `previous`
            // would make restore a no-op.
            let clean = strip_managed(&pre, ctx);
            let backup = ensure_backup(&ctx.data_dir, game_id, &path, None).ok();
            rec.files.push(ApplyFile {
                path: rel,
                backup,
                previous: Some(clean.to_string()),
            });
        }
        None => {
            rec.files.push(ApplyFile {
                path: rel,
                backup,
                previous: Some(snapshot_text),
            });
        }
    }
    write_record(&ctx.data_dir, &rec)?;
    if already {
        Ok(format!("{game_id} already applied (1 file)"))
    } else {
        Ok(format!("{game_id} applied (1 file)"))
    }
}

pub(crate) fn heroic_restore(ctx: &ApplyCtx, game_id: &str, app: &str) -> Result<String> {
    let rec = read_record(&ctx.data_dir, game_id)?.ok_or_else(|| {
        Error::Apply(format!("{game_id} has no apply record; nothing to restore"))
    })?;
    if rec.manager != "heroic" {
        return Err(Error::Apply(format!(
            "{game_id} was applied via {}",
            rec.manager
        )));
    }
    for entry in &rec.files {
        let path = PathBuf::from(&entry.path);
        let Ok(text) = fs::read_to_string(&path) else {
            tracing::warn!(path = %entry.path, "apply target missing on restore; skipping");
            continue;
        };
        let mut doc: Value = serde_json::from_str(&text)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        let snapshot: Value = entry
            .previous
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        {
            let target = heroic_target(&mut doc, app)?;
            heroic_unensure(target, &snapshot);
        }
        let text = serde_json::to_string_pretty(&doc)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        atomic_write(&path, text.as_bytes())?;
    }
    drop_record(&ctx.data_dir, game_id)?;
    Ok(format!("{game_id} restored ({} file(s))", rec.files.len()))
}
