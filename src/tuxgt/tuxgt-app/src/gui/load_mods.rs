use tuxgt_core::{
    config_dir, data_dir, diagnose, enabled_knobs, game_manifests, list_mods, mods_for_game,
    parse_mod_type, parse_slot, EnvKnob, GameRow, ModPackage, PluginHost, Strings,
};

use super::*;

/// Mods tab rows for one game: recipes applicable to that game plus every
/// manifest already installed for it (an installed mod never disappears).
pub(crate) fn load_mods_for(
    game_id: &str,
    game: Option<&GameRow>,
    strings: &Strings,
) -> Vec<ModRow> {
    let data = data_dir();
    let name = game.and_then(|g| g.name.as_deref()).unwrap_or("");
    let appid = game
        .and_then(|g| g.resolved_appid())
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|a| *a != 0);
    let inst = mods_for_game(&config_dir(), name, appid, &data).ok();
    let manifests = game_manifests(&data, game_id).unwrap_or_default();
    let req_owned: Vec<Vec<&str>> = manifests
        .iter()
        .map(|m| {
            inst.as_ref()
                .and_then(|l| l.mods.iter().find(|i| i.id == m.instance))
                .map(|i| i.requires.iter().map(String::as_str).collect())
                .unwrap_or_default()
        })
        .collect();
    let pkgs: Vec<ModPackage<'_>> = manifests
        .iter()
        .zip(req_owned.iter())
        .map(|(m, req)| ModPackage {
            name: &m.instance,
            type_: &m.mod_type,
            slot: proxy_slot(&m.files).and_then(|s| parse_slot(s).ok()),
            requires: &[],
            requires_mods: req,
        })
        .collect();
    let diag = diagnose(&pkgs).ok();
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(list) = inst.as_ref() {
        for i in &list.mods {
            seen.insert(i.id.clone());
            let effect_files = tuxgt_core::effect_names_for(i).into_boxed_slice();
            let asset = source_asset(&i.source);
            let payload_present = tuxgt_core::payload_has_files(&data, i);
            if let Some(m) = manifests.iter().find(|m| m.instance == i.id) {
                rows.push(mod_row_from_manifest(
                    m,
                    &i.label,
                    i.official,
                    effect_files,
                    asset,
                    payload_present,
                    diag.as_ref(),
                    strings,
                    game_id,
                ));
            } else {
                let ids = ModIds::for_instance(&i.id, game_id);
                rows.push(ModRow {
                    instance: i.id.clone(),
                    label: i.label.clone(),
                    mod_type: i.mod_type.clone(),
                    official: i.official,
                    adapter: "preload".into(),
                    enabled: false,
                    files: 0,
                    load_order: 0,
                    installed: false,
                    slot: strings.get("gui-mod-slot-none"),
                    graph: strings.get("gui-state-not-installed"),
                    file_entries: Box::default(),
                    env_entries: Box::default(),
                    effect_files,
                    asset,
                    payload_present,
                    ids,
                });
            }
        }
    }
    for m in &manifests {
        if seen.contains(&m.instance) {
            continue;
        }
        // Recipe gone from the catalog: officialness unknown and no
        // effect/asset metadata to preview, treat as user.
        rows.push(mod_row_from_manifest(
            m,
            &m.instance,
            false,
            Box::default(),
            None,
            false,
            diag.as_ref(),
            strings,
            game_id,
        ));
    }
    rows
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mod_row_from_manifest(
    m: &tuxgt_core::FileManifest,
    label: &str,
    official: bool,
    effect_files: Box<[String]>,
    asset: Option<String>,
    payload_present: bool,
    diag: Option<&tuxgt_core::Diagnosis>,
    strings: &Strings,
    game_id: &str,
) -> ModRow {
    let ids = ModIds::for_instance(&m.instance, game_id);
    ModRow {
        instance: m.instance.clone(),
        label: label.to_string(),
        mod_type: m.mod_type.clone(),
        official,
        adapter: m.adapter.clone(),
        enabled: m.enabled,
        files: m.files.len(),
        load_order: m.load_order,
        installed: true,
        slot: proxy_slot(&m.files)
            .map(|s| s.to_string())
            .unwrap_or_else(|| strings.get("gui-mod-slot-none")),
        graph: graph_note(&m.instance, diag, strings),
        file_entries: m
            .files
            .iter()
            .map(|f| ModFileRow {
                dest: f.dest.clone(),
                source: f.source.clone(),
                enabled: f.enabled,
                required: tuxgt_core::is_required_dest(
                    &m.mod_type,
                    &f.dest,
                    &m.include,
                    m.files.len(),
                ),
                loaddll: tuxgt_core::is_dll(&f.dest)
                    && !tuxgt_core::include_covers(&m.include, &f.dest),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        env_entries: m
            .env
            .iter()
            .map(|e| ModEnvRow {
                key: e.key.clone(),
                value: e.value.clone(),
                enabled: e.enabled,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        effect_files,
        asset,
        payload_present,
        ids,
    }
}

/// Sibling `.dll` dest's file name: the preload/install proxy slot.
/// Only top-level dests (no `/` or `\`, no `pfx:` prefix) sit beside the
/// game exe and can be the proxy; subdir companions never claim it.
/// `None` when no sibling dll dest exists (stock-named ReShade dests pass through as-is).
pub(crate) fn proxy_slot(files: &[tuxgt_core::PlannedFile]) -> Option<&str> {
    files
        .iter()
        .map(|f| f.dest.as_str())
        .filter(|d| {
            d.to_ascii_lowercase().ends_with(".dll")
                && !d.contains('/')
                && !d.contains('\\')
                && !tuxgt_core::is_prefix_dest(d)
        })
        .next()
}
/// E91: types whose dests can claim a preload proxy slot (Slot dropdown in
/// the Add panel + game installed cards). Reuses core `ModType`:
/// OptiScaler via `default_slot`; Custom explicitly (its DLLs take core
/// `apply_package_slot`). All other types claim no proxy: no dropdown.
pub(crate) fn slot_capable(mod_type: &str) -> bool {
    if mod_type == "custom" {
        return true;
    }
    parse_mod_type(mod_type).is_ok_and(|t| t.default_slot().is_some())
}
/// E91: Add-form Requires gate. True when the type carries kind-level
/// Requires (`reshade_addon`/`effect`/`texture` need a ReShade Mod): Save
/// stays blocked until one is picked. Reuses core `ModType::requires`.
pub(crate) fn add_requires_gate(mod_type: &str) -> bool {
    parse_mod_type(mod_type).is_ok_and(|t| t.requires().is_some())
}
/// E78 merged file row: basename of a depot path (`/`, `\`, or the cache
/// `instance#` separator).
pub(crate) fn file_basename(path: &str) -> &str {
    path.rsplit(['/', '\\', '#']).next().unwrap_or(path)
}

/// E78 merged file row label: `basename(source) → dest`, or `dest` alone
/// when both basenames match (case-insensitive: `FOO.dll` is `foo.dll`).
pub(crate) fn file_mapping_label(source: &str, dest: &str) -> String {
    let base = file_basename(source);
    if base.eq_ignore_ascii_case(file_basename(dest)) {
        dest.to_string()
    } else {
        format!("{base} → {dest}")
    }
}

/// Whole-token match so `optiscaler` never matches `optiscaler-xyz`.
pub(crate) fn name_mentioned(line: &str, instance: &str) -> bool {
    line.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .any(|tok| tok == instance)
}
/// Per-row deps/conflicts text. R50: empty when the diagnosis mentions this
/// instance nowhere — the card renders the graph line only for missing
/// requires or slot conflicts. Unavailable diagnosis still says so.
pub(crate) fn graph_note(
    instance: &str,
    diag: Option<&tuxgt_core::Diagnosis>,
    strings: &Strings,
) -> String {
    let Some(d) = diag else {
        return strings.get("gui-mod-graph-unavailable");
    };
    let mut hits: Vec<&str> = Vec::new();
    for line in d.missing_requires.iter().chain(d.slot_conflicts.iter()) {
        if name_mentioned(line, instance) {
            hits.push(line.as_str());
        }
    }
    hits.join(" · ")
}

pub(crate) fn load_plugins(strings: &Strings) -> Vec<PluginRow> {
    PluginHost::load()
        .map(|host| {
            host.list()
                .into_iter()
                .map(|e| PluginRow {
                    id: e.desc.id().to_string(),
                    label: strings.get(e.desc.label_id),
                    enabled: e.enabled,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn load_instances() -> Vec<InstanceRow> {
    let data = data_dir();
    let cfg = config_dir();
    list_mods(&cfg, &data)
        .map(|l| {
            l.mods
                .into_iter()
                .map(|i| {
                    let effect_files = tuxgt_core::effect_names_for(&i).into_boxed_slice();
                    let asset = source_asset(&i.source);
                    let payload_present = tuxgt_core::payload_has_files(&data, &i);
                    let ids = InstanceIds::for_id(&i.id);
                    InstanceRow {
                        id: i.id,
                        label: i.label,
                        mod_type: i.mod_type,
                        source: i.source.type_str().into(),
                        official: i.official,
                        enabled: i.enabled,
                        effect_files,
                        asset,
                        payload_present,
                        ids,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn load_knobs() -> Vec<&'static EnvKnob> {
    PluginHost::load()
        .map(|h| enabled_knobs(&h))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{file_basename, file_mapping_label, proxy_slot, ProtonSummary};
    use tuxgt_core::PlannedFile;

    fn planned(dest: &str) -> PlannedFile {
        PlannedFile {
            source: dest.into(),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn proxy_slot_ignores_subdir_companions() {
        let files = vec![planned("bin/D3D12Core.dll"), planned("dxgi.dll")];
        assert_eq!(proxy_slot(&files), Some("dxgi.dll"));
        let files = vec![planned("bin/dxgi.dll")];
        assert_eq!(proxy_slot(&files), None);
        let files = vec![planned("dxgi.dll")];
        assert_eq!(proxy_slot(&files), Some("dxgi.dll"));
    }

    #[test]
    fn mapping_shows_source_base_when_renamed() {
        assert_eq!(
            file_mapping_label("depot/OptiScaler.dll", "dxgi.dll"),
            "OptiScaler.dll → dxgi.dll"
        );
        assert_eq!(
            file_mapping_label("cache/e567eb2e47363f15/OS-v3#OptiScaler.dll", "dxgi.dll"),
            "OptiScaler.dll → dxgi.dll"
        );
    }

    #[test]
    fn mapping_shows_dest_only_when_basenames_match() {
        assert_eq!(file_mapping_label("depot/dxgi.dll", "dxgi.dll"), "dxgi.dll");
        assert_eq!(
            file_mapping_label("depot/ReShade64.dll", "win32\\ReShade64.dll"),
            "win32\\ReShade64.dll"
        );
    }

    #[test]
    fn basename_matches_case_insensitively() {
        assert_eq!(file_basename("win32\\FOO.DLL"), "FOO.DLL");
        assert_eq!(file_mapping_label("depot/FOO.dll", "foo.DLL"), "foo.DLL");
    }

    /// E84: the Info block parses the whitelisted E83 tier; `pending`
    /// falls back to the provisional tier and a missing cache stays an
    /// honest `none`.
    #[test]
    fn proton_summary_parses_cached_whitelist() {
        let full = serde_json::json!({"tier": "gold"});
        let s = ProtonSummary::parse(&full);
        assert_eq!(s.tier, "gold");

        let pending = serde_json::json!({"tier": "pending", "provisionalTier": "silver"});
        let s = ProtonSummary::parse(&pending);
        assert_eq!(s.tier, "silver");

        let s = ProtonSummary::default();
        assert_eq!(s.tier, "none");
    }
}
