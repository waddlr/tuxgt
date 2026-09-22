use std::collections::BTreeMap;
use std::path::Path;

use crate::game::GameId;
use crate::prewire::managed_ini;
use crate::{game_manifests, Result};

/// Managed-dir exports: the per-game ini prewire writes, the game runtime
/// dir, and the depot (game stage dir). Env wins in the loader, so these
/// keep Play and prewire on the same files.
pub(crate) fn add_managed_env(
    env: &mut BTreeMap<String, String>,
    data_dir: &Path,
    id: &str,
) -> Result<()> {
    let gid = GameId::parse(id)?;
    let gdir = crate::game::game_dir(data_dir, &gid);
    env.insert(
        "TUXGT_LAUNCHER_INI".into(),
        managed_ini(&gdir).to_string_lossy().into_owned(),
    );
    env.insert(
        "TUXGT_GAME_DIR".into(),
        gdir.join("runtime").to_string_lossy().into_owned(),
    );
    env.insert(
        "TUXGT_DEPOT".into(),
        gdir.join("stage").to_string_lossy().into_owned(),
    );
    if crate::debug_log_enabled() {
        env.insert("TUXGT_DEBUG".into(), "1".into());
    }
    Ok(())
}

/// Append-only `PRESSURE_VESSEL_FILESYSTEMS_RW` entry (mirrors the wrapper
/// script's `pv_rw_add`): skip empties and dupes.
pub(crate) fn pv_rw_add(env: &mut BTreeMap<String, String>, p: &str) {
    if p.is_empty() {
        return;
    }
    let cur = env
        .get("PRESSURE_VESSEL_FILESYSTEMS_RW")
        .cloned()
        .unwrap_or_default();
    let mut parts: Vec<&str> = cur.split(':').filter(|s| !s.is_empty()).collect();
    if !parts.contains(&p) {
        parts.push(p);
    }
    env.insert("PRESSURE_VESSEL_FILESYSTEMS_RW".into(), parts.join(":"));
}

pub(crate) fn proton_optiscaler_flavor(proton: Option<&str>) -> bool {
    proton.is_some_and(|p| {
        let l = p.to_ascii_lowercase();
        l.contains("cachy") || l.contains("ge-proton")
    })
}

/// Proton's built-in OptiScaler grant: any enabled manifest whose instance
/// recipe allows the `proton_env` plan (the official optiscaler recipe
/// does). Nothing is hardcoded to an instance id.
pub(crate) fn has_proton_env_plan(data_dir: &Path, id: &str) -> Result<bool> {
    for m in game_manifests(data_dir, id)? {
        if m.enabled && instance_allows_proton_env(data_dir, &m.instance)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Resolve one manifest instance to its recipe; unknown ids grant nothing.
pub(crate) fn instance_allows_proton_env(data_dir: &Path, instance: &str) -> Result<bool> {
    let listed = crate::instance::official_mods(&crate::instance::official_mods_dir(data_dir))?;
    Ok(listed
        .into_iter()
        .any(|i| i.id == instance && i.plans_allowed.contains(&crate::Plan::ProtonEnv)))
}

/// Merge one `stem=n,b` entry into `WINEDLLOVERRIDES`, leaving existing
/// entries (including an existing override for the stem) alone.
pub(crate) fn wine_dll_override(env: &mut BTreeMap<String, String>, stem: &str) {
    if stem.is_empty() {
        return;
    }
    let cur = env.get("WINEDLLOVERRIDES").cloned().unwrap_or_default();
    let mut parts: Vec<String> = cur
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .collect();
    let want = format!("{stem}=");
    if !parts.iter().any(|p| {
        p.to_ascii_lowercase()
            .starts_with(&want.to_ascii_lowercase())
    }) {
        parts.push(format!("{stem}=n,b"));
    }
    env.insert("WINEDLLOVERRIDES".into(), parts.join(";"));
}

/// Normalize one `WINEDLLOVERRIDES` dll token for stem comparison: trim,
/// strip a trailing `.dll` (any case), lowercase.
pub(crate) fn dll_stem(token: &str) -> String {
    let lower = token.trim().to_ascii_lowercase();
    lower.strip_suffix(".dll").unwrap_or(&lower).to_string()
}

/// Split a `WINEDLLOVERRIDES` value into `(stem, mode)` pairs, one per dll
/// token (`entry[;entry]`, each `dll[,dll]=mode`; entries without `=` are
/// skipped).
pub(crate) fn split_override_entries(value: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in value.split(';') {
        let Some((dlls, mode)) = entry.split_once('=') else {
            continue;
        };
        for dll in dlls.split(',') {
            let stem = dll_stem(dll);
            if !stem.is_empty() {
                out.push((stem, mode.to_string()));
            }
        }
    }
    out
}

/// Enabled manifest `[[env]]` rows for one game in (`load_order`, instance-id) order.
pub(crate) fn mod_env_rows(data_dir: &Path, game_id: &str) -> Result<Vec<(String, String)>> {
    let mut manifests = crate::game_manifests(data_dir, game_id)?;
    manifests.sort_by(|a, b| (a.load_order, &a.instance).cmp(&(b.load_order, &b.instance)));
    let mut out = Vec::new();
    for m in &manifests {
        if !m.enabled {
            continue;
        }
        for e in &m.env {
            if e.enabled {
                out.push((e.key.clone(), e.value.clone()));
            }
        }
    }
    Ok(out)
}

/// Merge manifest env into the launch env (E74). Generic keys last-wins;
/// `WINEDLLOVERRIDES` merges per stem — existing stems win, missing stems
/// append as `stem=mode`, first manifest wins per stem.
pub fn apply_mod_env(
    env: &mut BTreeMap<String, String>,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    let mut seen: Vec<String> = split_override_entries(
        env.get("WINEDLLOVERRIDES")
            .map(String::as_str)
            .unwrap_or(""),
    )
    .into_iter()
    .map(|(stem, _)| stem)
    .collect();
    for (k, v) in mod_env_rows(data_dir, game_id)? {
        if k != "WINEDLLOVERRIDES" {
            env.insert(k, v);
            continue;
        }
        let mut parts: Vec<String> = env
            .get("WINEDLLOVERRIDES")
            .cloned()
            .unwrap_or_default()
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .collect();
        for (stem, mode) in split_override_entries(&v) {
            if seen.iter().any(|s| s == &stem) {
                continue;
            }
            seen.push(stem.clone());
            parts.push(format!("{stem}={mode}"));
        }
        env.insert("WINEDLLOVERRIDES".into(), parts.join(";"));
    }
    Ok(())
}
