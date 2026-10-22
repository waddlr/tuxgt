//! Maintainer harvest. The app never calls this.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::schema::stem_of;
use super::suppress;
use crate::{Error, Result};

pub fn harvest_protonfixes(src: &Path, out: &Path, pin: &str) -> Result<usize> {
    let dirs = fix_dirs(src);
    if dirs.is_empty() {
        return Err(Error::Args(format!(
            "no gamefixes-* under {}",
            src.display()
        )));
    }
    let games = out.join("games");
    fs::create_dir_all(&games)?;
    let date = utc_date();
    let mut n = 0;
    for dir in dirs {
        let label = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            if path.extension().is_none_or(|e| e != "py") {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let lits = suppress::append_literals(&text);
            if lits.is_empty() {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut body = format!("# protonfixes {pin} {date}\n# {label}/{stem}.py\n");
            for lit in lits {
                body.push_str(&format!(
                    "\n[[fix]]\nstem = \"{}\"\nappended_value = \"{}\"\nfix_file = \"{label}/{stem}.py\"\n",
                    toml_escape(&stem_of(&lit)),
                    toml_escape(&lit),
                ));
            }
            fs::write(games.join(format!("{stem}.toml")), body)?;
            n += 1;
        }
    }
    Ok(n)
}

/// `Ok(false)` means the sample has no structured launch field and nothing was written.
pub fn protondb_has_launch_field(dump: &Path) -> Result<bool> {
    let text = fs::read_to_string(dump)?;
    let v: Value =
        serde_json::from_str(&text).map_err(|e| Error::Args(format!("protondb: {e}")))?;
    let sample: Vec<&Value> = match &v {
        Value::Array(a) => a.iter().take(2).collect(),
        Value::Object(m) => m.values().take(2).collect(),
        other => vec![other],
    };
    Ok(sample.iter().any(|v| has_key(v)))
}

fn has_key(v: &Value) -> bool {
    match v {
        Value::Object(m) => {
            m.keys().any(|k| {
                matches!(
                    k.as_str(),
                    "arguments" | "launchOptions" | "cmdline" | "flags"
                )
            }) || m.values().any(has_key)
        }
        Value::Array(a) => a.iter().any(has_key),
        _ => false,
    }
}

fn fix_dirs(src: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for base in [src.to_path_buf(), src.join("protonfixes")] {
        let Ok(rd) = fs::read_dir(&base) else {
            continue;
        };
        for ent in rd.flatten() {
            let name = ent.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("gamefixes-") && ent.path().is_dir() {
                out.push(ent.path());
            }
        }
    }
    out.sort();
    out
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn utc_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Civil from days since 1970-01-01 (Howard Hinnant).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
