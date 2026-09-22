use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use sqlx::SqlitePool;

use super::*;
use crate::{PluginHost, Result};

/// `$XDG_CONFIG_HOME/environment.d/90-tuxgt.conf`, else `~/.config/environment.d/90-tuxgt.conf`.
pub(crate) fn environment_d_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::dirs_home().join(".config"));
    base.join("environment.d/90-tuxgt.conf")
}

pub(crate) const ENV_D_HEADER: &str =
    "# Written by TuxGT. Do not edit; TuxGT rewrites this file.\n";

/// Delete a leftover TuxGT-owned `90-tuxgt.conf`. Foreign files stay.
/// Missing path is ok. Io errors propagate.
pub fn retire_environment_d() -> Result<()> {
    let path = environment_d_path();
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if bytes.starts_with(ENV_D_HEADER.as_bytes()) {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Env pairs for enabled+set globals. `host` None skips the plugin-disabled filter
/// (launch merge passes a host).
pub async fn enabled_global_env_pairs(
    pool: &SqlitePool,
    host: Option<&PluginHost>,
) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for row in global_knobs(pool).await? {
        if !row.enabled {
            continue;
        }
        let def = match host {
            Some(h) => find_enabled_knob(h, &row.knob),
            None => find_knob(&row.knob),
        };
        let Some(def) = def else {
            continue;
        };
        if let Some(pairs) = def.env_pairs(&row.value) {
            for (k, v) in pairs {
                out.insert(k, v);
            }
        }
    }
    Ok(out)
}
