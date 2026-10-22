//! Merge the argument tail into Heroic `launcherArgs` without moving `wrapperOptions`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::*;
use crate::apply::{read_record, write_record, ApplyCtx};
use crate::{atomic_write, Error, Result};

/// Set live `launcherArgs` from the user baseline plus `tail`. The first Save
/// records the pre-Save `launcherArgs` (null when the key was absent). Later
/// Saves do not replace that baseline.
pub(crate) fn publish_launcher_args(
    ctx: &ApplyCtx,
    game_id: &str,
    app: &str,
    tail: &str,
) -> Result<String> {
    let mut rec = read_record(&ctx.data_dir, game_id)?.ok_or_else(|| {
        Error::Apply(format!(
            "{game_id} has no apply record; save arms Apply first"
        ))
    })?;
    if rec.manager != "heroic" {
        return Err(Error::Apply(format!(
            "{game_id} was applied via {}",
            rec.manager
        )));
    }
    if rec.files.is_empty() {
        return Err(Error::Apply(format!("{game_id} apply record has no files")));
    }
    for entry in &rec.files {
        if !Path::new(&entry.path).is_file() {
            return Err(Error::Apply(format!(
                "apply target missing on arg save: {}",
                entry.path
            )));
        }
    }
    let mut wrote = 0;
    for entry in &mut rec.files {
        let path = PathBuf::from(&entry.path);
        let text = fs::read_to_string(&path)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        let mut doc: Value = serde_json::from_str(&text)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        let mut snap: Value = entry
            .previous
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| Value::Object(Map::new()));
        let added = snap.get("launcherArgs").is_none();
        {
            let target = heroic_target(&mut doc, app)?;
            if added {
                let prev = target.get("launcherArgs").cloned().unwrap_or(Value::Null);
                if let Some(obj) = snap.as_object_mut() {
                    obj.insert("launcherArgs".into(), prev);
                }
            }
            let base = snap
                .get("launcherArgs")
                .and_then(Value::as_str)
                .unwrap_or("");
            let joined = join_args(base, tail);
            target.insert("launcherArgs".into(), Value::String(joined));
        }
        if added {
            entry.previous = Some(snap.to_string());
        }
        let text = serde_json::to_string_pretty(&doc)
            .map_err(|e| Error::Apply(format!("{}: {e}", path.display())))?;
        atomic_write(&path, text.as_bytes())?;
        wrote += 1;
    }
    write_record(&ctx.data_dir, &rec)?;
    Ok(format!("{game_id} arguments saved ({wrote} file(s))"))
}

fn join_args(base: &str, tail: &str) -> String {
    let base = base.trim();
    let tail = tail.trim();
    if base.is_empty() {
        tail.to_string()
    } else if tail.is_empty() {
        base.to_string()
    } else {
        format!("{base} {tail}")
    }
}
