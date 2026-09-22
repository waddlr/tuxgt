//! Library snapshot for the Settings Mods Details disclosure: installed
//! game names, AppID → display name, and the last catalog-check row.
//! Loaded on the UI thread (`rt_block`), never from inside another
//! `rt_block` (that deadlocks the shared runtime).

use std::collections::HashMap;

use tuxgt_core::{data_dir, game_manifests, list_games, mod_cache_rows, open_db_shared};

use super::load::rt_block;

#[derive(Clone, Debug, Default)]
pub(crate) struct CatalogMeta {
    pub installed: HashMap<String, Vec<String>>,
    pub appids: HashMap<u32, String>,
    pub cache: HashMap<String, CacheSnap>,
}

#[derive(Clone, Debug)]
pub(crate) struct CacheSnap {
    pub last_check: Option<i64>,
    pub status: Option<String>,
    pub detail: Option<String>,
}

pub(crate) fn load_catalog_meta() -> CatalogMeta {
    let loaded = rt_block(async {
        let data = data_dir();
        let pool = open_db_shared(&data).await?;
        let games = list_games(&pool, None, None, None).await?;
        let rows = mod_cache_rows(&pool).await?;
        Ok((games, rows))
    });
    let Ok((games, rows)) = loaded else {
        return CatalogMeta::default();
    };
    let data = data_dir();
    let mut installed: HashMap<String, Vec<String>> = HashMap::new();
    let mut appids = HashMap::new();
    for g in &games {
        if let Some(id) = g.resolved_appid().and_then(|s| s.parse::<u32>().ok()) {
            appids
                .entry(id)
                .or_insert_with(|| g.display_name().to_string());
        }
        for m in game_manifests(&data, &g.id).unwrap_or_default() {
            installed
                .entry(m.instance)
                .or_default()
                .push(g.display_name().to_string());
        }
    }
    for names in installed.values_mut() {
        names.sort();
        names.dedup();
    }
    let cache = rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                CacheSnap {
                    last_check: r.last_check,
                    status: r.update_status,
                    detail: r.update_detail,
                },
            )
        })
        .collect();
    CatalogMeta {
        installed,
        appids,
        cache,
    }
}
