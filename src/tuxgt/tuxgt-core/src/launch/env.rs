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
/// recipe allows the `proton_env` plan. Shipped OptiScaler recipes do not.
/// Nothing is hardcoded to an instance id.
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
    let (listed, _) =
        crate::instance::official_mods(&crate::instance::official_mods_dir(data_dir))?;
    Ok(listed
        .into_iter()
        .any(|i| i.id == instance && i.plans_allowed.contains(&crate::Plan::ProtonEnv)))
}

/// Merge one `stem=n,b` entry into `WINEDLLOVERRIDES`. An existing stem
/// wins, including `dll[,dll]=mode` and a `.dll` suffix on the token.
pub(crate) fn wine_dll_override(env: &mut BTreeMap<String, String>, stem: &str) {
    let stem = dll_stem(stem);
    if stem.is_empty() {
        return;
    }
    let cur = env.get("WINEDLLOVERRIDES").cloned().unwrap_or_default();
    if split_override_entries(&cur).iter().any(|(s, _)| s == &stem) {
        return;
    }
    let mut parts: Vec<String> = cur
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .collect();
    parts.push(format!("{stem}=n,b"));
    env.insert("WINEDLLOVERRIDES".into(), parts.join(";"));
}

/// True when this enabled manifest writes a `WINEDLLOVERRIDES` stem
/// (install-adapter companion DLL or a proxy slot).
pub(crate) fn manifest_has_dll_override(m: &crate::FileManifest) -> bool {
    if !m.enabled {
        return false;
    }
    if crate::is_install(&m.adapter) {
        for f in &m.files {
            if crate::prewire::is_dll(&f.dest) && !crate::mods::is_named_injector(&f.dest) {
                return true;
            }
        }
    }
    let Some(idx) = crate::mods::claiming_slot_index(&m.files, &m.include) else {
        return false;
    };
    let dest = m.files[idx].dest.as_str();
    let base = dest.rsplit(['/', '\\']).next().unwrap_or(dest);
    crate::parse_slot(base).is_ok()
}

/// Install-adapter `*.dll` dests get `stem=n,b`. Stock `<self>` names do
/// not: Wine is not how the game loads `OptiScaler.dll` or `ReShade64.dll`.
pub(crate) fn apply_install_dll_overrides(
    env: &mut BTreeMap<String, String>,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    for m in crate::game_manifests(data_dir, game_id)? {
        if !m.enabled || !crate::is_install(&m.adapter) {
            continue;
        }
        for f in &m.files {
            if !crate::prewire::is_dll(&f.dest) || crate::mods::is_named_injector(&f.dest) {
                continue;
            }
            if let Some(stem) = std::path::Path::new(&f.dest)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                wine_dll_override(env, stem);
            }
        }
    }
    Ok(())
}

/// Proxy slot on any adapter: `dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`.
/// `<self>` does not parse as a proxy stem, so it adds nothing. Existing
/// stems win.
pub(crate) fn apply_proxy_slot_overrides(
    env: &mut BTreeMap<String, String>,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    for m in crate::game_manifests(data_dir, game_id)? {
        if !m.enabled {
            continue;
        }
        let Some(idx) = crate::mods::claiming_slot_index(&m.files, &m.include) else {
            continue;
        };
        let dest = m.files[idx].dest.as_str();
        let base = dest.rsplit(['/', '\\']).next().unwrap_or(dest);
        if let Ok(slot) = crate::parse_slot(base) {
            wine_dll_override(env, slot.as_str());
        }
    }
    Ok(())
}

/// Store env, then a launch-option assignment if the key is still absent.
/// Call this after knobs and custom env (those replace the whole value)
/// and before manifest `[[env]]`, matching Play.
pub(crate) fn seed_store_winedll(
    env: &mut BTreeMap<String, String>,
    store_env: Option<&str>,
    launch_options: Option<&str>,
) -> Result<()> {
    let store = super::runners::parse_env(store_env.unwrap_or(""))?;
    if let Some(v) = store.get("WINEDLLOVERRIDES") {
        env.entry("WINEDLLOVERRIDES".to_string())
            .or_insert_with(|| v.clone());
    }
    let (_, _, opt) = super::runners::parse_launch_options(launch_options);
    if let Some((_, v)) = opt.into_iter().find(|(k, _)| k == "WINEDLLOVERRIDES") {
        env.entry("WINEDLLOVERRIDES".to_string()).or_insert(v);
    }
    Ok(())
}

/// Install-adapter companions, then the proxy slot. The session file
/// replaces `WINEDLLOVERRIDES` wholesale, so this runs on the same base
/// Play uses.
pub(crate) fn apply_session_dll_overrides(
    env: &mut BTreeMap<String, String>,
    data_dir: &Path,
    game_id: &str,
) -> Result<()> {
    apply_install_dll_overrides(env, data_dir, game_id)?;
    apply_proxy_slot_overrides(env, data_dir, game_id)
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
