mod platform;

pub(crate) use platform::*;

use std::collections::HashMap;

use super::*;
use tuxgt_core::{cached_metadata, data_dir, open_db_shared, FluentArgs, GameRow, Strings};

use super::super::widgets;
use super::super::{rt_block, Shell};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FilterKind {
    Store,
    Platform,
    ProtonDb,
}

impl FilterKind {
    pub(crate) fn apply(self, shell: &mut Shell, v: Option<String>) {
        tracing::debug!(action = "set-filter", kind = ?self, value = ?v);
        match self {
            FilterKind::Store => shell.filters.store = v,
            FilterKind::Platform => shell.filters.platform = v,
            FilterKind::ProtonDb => shell.filters.protondb = v,
        }
        shell.recompute_base();
        tracing::debug!(action = "set-filter", kind = ?self, count = shell.base_filtered.len());
    }
}

/// Filter value: `{manager}:{store}` (store may be empty).
pub(crate) fn store_key(manager: &str, store: &str) -> String {
    format!("{manager}:{store}")
}

pub(crate) fn parse_store_key(key: &str) -> Option<(&str, &str)> {
    key.split_once(':')
}

/// Store-first label: empty store → manager name; unique store id → store
/// name; same store under multiple managers → `{store} - {manager}`.
pub(crate) fn store_label(
    manager: &str,
    store: &str,
    multi_store: bool,
    strings: &Strings,
) -> String {
    if store.is_empty() {
        return widgets::id_label(widgets::ValKind::Manager, manager, strings);
    }
    let store_l = widgets::id_label(widgets::ValKind::Store, store, strings);
    if multi_store {
        let mut args = FluentArgs::new();
        args.set("store", store_l);
        args.set(
            "manager",
            widgets::id_label(widgets::ValKind::Manager, manager, strings),
        );
        strings.get_args("gui-filter-store-mgr", Some(&args))
    } else {
        store_l
    }
}

/// Unique `manager:store` options for the Store filter, labeled per
/// `store_label`. Counts which non-empty store ids appear under >1 manager.
pub(crate) fn store_options(games: &[GameRow], strings: &Strings) -> Vec<(String, String)> {
    let mut pairs: Vec<String> = Vec::new();
    for g in games {
        let key = store_key(&g.manager, &g.store);
        if !pairs.iter().any(|k| k == &key) {
            pairs.push(key);
        }
    }
    pairs.sort();
    let mut store_mgrs: HashMap<String, Vec<String>> = HashMap::new();
    for key in &pairs {
        if let Some((m, s)) = parse_store_key(key) {
            if !s.is_empty() {
                let v = store_mgrs.entry(s.to_string()).or_default();
                if !v.iter().any(|x| x == m) {
                    v.push(m.to_string());
                }
            }
        }
    }
    pairs
        .into_iter()
        .map(|key| {
            let (m, s) = parse_store_key(&key).unwrap_or(("", ""));
            let multi = !s.is_empty() && store_mgrs.get(s).map(|v| v.len() > 1).unwrap_or(false);
            let label = store_label(m, s, multi, strings);
            (key, label)
        })
        .collect()
}

/// Label for one game's store chip (same rules as the Store filter).
pub(crate) fn game_store_label(g: &GameRow, games: &[GameRow], strings: &Strings) -> String {
    let multi = !g.store.is_empty() && {
        let mut mgrs = Vec::new();
        for x in games {
            if x.store == g.store && !mgrs.iter().any(|m| m == &x.manager) {
                mgrs.push(x.manager.clone());
            }
        }
        mgrs.len() > 1
    };
    store_label(&g.manager, &g.store, multi, strings)
}

/// Cache-only AWACY flags for Library cards. Reads the single ("", "awacy")
/// metadata row (the whole games.json array) once per call site
/// (startup/rescan). Unflagged and "supported" games are absent from the map.
/// Never touches the network.
pub(crate) fn load_awacy(games: &[GameRow]) -> HashMap<String, AwacyFlag> {
    let rows = rt_block(async {
        let pool = open_db_shared(&data_dir()).await?;
        cached_metadata(&pool).await
    })
    .unwrap_or_default();
    awacy_flags(games, &rows)
}

/// Match rule: the row's resolved appid (stored overlay else Steam game
/// segment) is looked up first, then the case-insensitive name. Unflagged and
/// "supported" games stay absent from the map.
pub(crate) fn awacy_flags(
    games: &[GameRow],
    rows: &[(String, String, String)],
) -> HashMap<String, AwacyFlag> {
    let mut out = HashMap::new();
    let mut entries: Vec<(String, Option<String>, String, String)> = Vec::new();
    for (_, source, data) in rows {
        if source != "awacy" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        for e in value.as_array().cloned().unwrap_or_default() {
            let name = e
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let steam_id = e
                .get("steam_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let status = e
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if status.is_empty()
                || matches!(
                    status.to_ascii_lowercase().as_str(),
                    "supported" | "none" | "null"
                )
            {
                continue;
            }
            let providers = e
                .get("anticheats")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            entries.push((name, steam_id, status, providers));
        }
    }
    if entries.is_empty() {
        return out;
    }
    for g in games {
        let by_appid = g.resolved_appid().and_then(|appid| {
            entries
                .iter()
                .find(|(_, sid, _, _)| sid.as_deref() == Some(appid))
        });
        let found = by_appid.or_else(|| {
            let name = g.name.clone().unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            entries
                .iter()
                .find(|(n, _, _, _)| n.eq_ignore_ascii_case(&name))
        });
        if let Some((_, _, status, providers)) = found {
            out.insert(
                g.id.clone(),
                AwacyFlag {
                    status: status.clone(),
                    providers: providers.clone(),
                },
            );
        }
    }
    out
}

pub(crate) fn tier_rank(tier: Option<&String>) -> u8 {
    match tier.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("platinum") => 0,
        Some("native") => 1,
        Some("gold") => 2,
        Some("silver") => 3,
        Some("bronze") => 4,
        Some("borked") => 5,
        _ => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::awacy_flags;
    use super::{store_label, store_options};
    use tuxgt_core::{GameRow, Strings};

    fn row(id: &str, manager: &str, name: &str, appid: Option<&str>) -> GameRow {
        GameRow {
            id: id.into(),
            name: Some(name.into()),
            cover_path: None,
            manager: manager.into(),
            store: String::new(),
            header_path: None,
            platform: None,
            api: None,
            install_dir: None,
            exe_path: None,
            prefix_path: None,
            proton: None,
            bitness: None,
            engine: None,
            hidden: false,
            last_played: None,
            steam_appid: appid.map(str::to_string),
        }
    }

    /// E65: a Heroic row matches on its stored overlay appid even when its
    /// name differs from the dataset entry.
    #[test]
    fn awacy_map_matches_stored_overlay_appid() {
        let data = serde_json::json!([{
            "name": "Halo: The Master Chief Collection",
            "steam_id": "976730",
            "status": "Denied",
            "anticheats": ["Easy Anti-Cheat"],
        }])
        .to_string();
        let rows = vec![(String::new(), "awacy".to_string(), data)];
        let games = vec![
            row("heroic:gog:halo", "heroic", "Halo", Some("976730")),
            row("steam::1", "steam", "Unrelated", None),
        ];
        let flags = awacy_flags(&games, &rows);
        let hit = flags.get("heroic:gog:halo").expect("overlay match");
        assert_eq!(hit.status, "Denied");
        assert_eq!(hit.providers, "Easy Anti-Cheat");
        assert!(!flags.contains_key("steam::1"));
    }

    #[test]
    fn store_label_empty_uses_manager() {
        let s = Strings::en_us().expect("catalog");
        assert_eq!(store_label("steam", "", false, &s), "Steam");
        assert_eq!(store_label("heroic", "", false, &s), "Heroic");
    }

    #[test]
    fn store_label_unique_store_no_suffix() {
        let s = Strings::en_us().expect("catalog");
        assert_eq!(store_label("heroic", "gog", false, &s), "GOG");
    }

    #[test]
    fn store_label_multi_store_adds_manager_suffix() {
        let s = Strings::en_us().expect("catalog");
        assert_eq!(
            store_label("heroic", "standalone", true, &s),
            "Standalone - Heroic"
        );
    }

    #[test]
    fn store_options_suffix_when_store_spans_managers() {
        let s = Strings::en_us().expect("catalog");
        let mut a = row("steam:standalone:1", "steam", "A", None);
        a.store = "standalone".into();
        let mut b = row("heroic:standalone:2", "heroic", "B", None);
        b.store = "standalone".into();
        let mut c = row("heroic:gog:3", "heroic", "C", None);
        c.store = "gog".into();
        let mut d = row("steam::4", "steam", "D", None);
        d.store = String::new();
        let opts = store_options(&[a, b, c, d], &s);
        let map: std::collections::HashMap<_, _> = opts.into_iter().collect();
        assert_eq!(
            map.get("steam:standalone").map(String::as_str),
            Some("Standalone - Steam")
        );
        assert_eq!(
            map.get("heroic:standalone").map(String::as_str),
            Some("Standalone - Heroic")
        );
        assert_eq!(map.get("heroic:gog").map(String::as_str), Some("GOG"));
        assert_eq!(map.get("steam:").map(String::as_str), Some("Steam"));
    }
}
