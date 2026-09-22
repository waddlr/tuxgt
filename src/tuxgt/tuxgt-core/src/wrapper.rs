//! Wrapper plugins (E42): per-game argv wrappers contributed by the
//! first-party `wrapper` plugin. A wrapper is argv, never env — the Game Env
//! tab knobs are the env-side equivalent (`env-knob.md`).

use sqlx::SqlitePool;

use crate::game::game_exists;
use crate::{Error, PluginHost, Result};

/// One registered wrapper: a program prepended to the launch argv plus its
/// prepended args. Table order is outer → inner.
pub struct WrapperDef {
    pub id: &'static str,
    /// GUI switch label (E44).
    pub label: &'static str,
    /// One line incl. the command it prepends.
    pub help: &'static str,
    /// Looked up on PATH at launch.
    pub program: &'static str,
    /// Prepended args; may be empty.
    pub args: &'static [&'static str],
}

/// v1 first-party set, outer → inner.
pub static WRAPPERS: &[WrapperDef] = &[
    WrapperDef {
        id: "gamescope",
        label: "Gamescope",
        help: "SteamOS session compositor; wraps the whole launch",
        program: "gamescope",
        args: &[],
    },
    WrapperDef {
        id: "gamemode",
        label: "GameMode",
        help: "Feral GameMode; CPU governor + scheduling",
        program: "gamemoderun",
        args: &[],
    },
    // MangoHud lives in two systems: this wrapper preloads via argv
    // (OpenGL and Vulkan); the Env tab knob sets MANGOHUD=1 (Vulkan
    // layer only). Both set is harmless — one process, overlay once.
    // Neither wins; they compose.
    WrapperDef {
        id: "mangohud",
        label: "MangoHud",
        help: "MangoHud overlay (argv preload, OpenGL + Vulkan); composes with the Env tab knob",
        program: "mangohud",
        args: &[],
    },
];

/// Whether the first-party `wrapper` plugin is enabled.
pub fn wrapper_enabled(host: &PluginHost) -> bool {
    host.is_enabled("wrapper")
}

/// Defs when the `wrapper` plugin is enabled, table order; a disabled
/// `wrapper` plugin offers none.
pub fn wrapper_defs(host: &PluginHost) -> &'static [WrapperDef] {
    if wrapper_enabled(host) {
        WRAPPERS
    } else {
        &[]
    }
}

/// Table lookup, enabled or not.
pub fn find_wrapper(id: &str) -> Option<&'static WrapperDef> {
    WRAPPERS.iter().find(|d| d.id == id)
}

/// Resolve one def when the `wrapper` plugin is enabled; a disabled
/// plugin's defs are not offered.
pub fn find_enabled_wrapper(host: &PluginHost, id: &str) -> Option<&'static WrapperDef> {
    wrapper_defs(host).iter().find(|d| d.id == id)
}

/// Stored wrapper ids for one game, table order (unknown ids last, by id).
/// A pure read: rows of a disabled plugin stay.
pub async fn game_wrappers(pool: &SqlitePool, game_id: &str) -> Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT wrapper FROM game_wrappers WHERE game_id = ?")
            .bind(game_id)
            .fetch_all(pool)
            .await?;
    let mut stored: Vec<String> = rows.into_iter().map(|(w,)| w).collect();
    stored.sort();
    stored.dedup();
    let mut out = Vec::with_capacity(stored.len());
    for def in WRAPPERS {
        if stored.iter().any(|w| w == def.id) {
            out.push(def.id.to_string());
        }
    }
    for w in stored {
        if !out.contains(&w) {
            out.push(w);
        }
    }
    Ok(out)
}

pub async fn set_wrapper(pool: &SqlitePool, game_id: &str, wrapper: &str) -> Result<()> {
    if find_wrapper(wrapper).is_none() {
        return Err(Error::UnknownWrapper(wrapper.into()));
    }
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    sqlx::query(
        "INSERT INTO game_wrappers (game_id, wrapper) VALUES (?, ?)
         ON CONFLICT(game_id, wrapper) DO NOTHING",
    )
    .bind(game_id)
    .bind(wrapper)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unset_wrapper(pool: &SqlitePool, game_id: &str, wrapper: &str) -> Result<()> {
    if find_wrapper(wrapper).is_none() {
        return Err(Error::UnknownWrapper(wrapper.into()));
    }
    if !game_exists(pool, game_id).await? {
        return Err(Error::UnknownGame(game_id.into()));
    }
    let res = sqlx::query("DELETE FROM game_wrappers WHERE game_id = ? AND wrapper = ?")
        .bind(game_id)
        .bind(wrapper)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(Error::WrapperNotSet(wrapper.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{seed_game, SeedGame};
    use std::path::Path;

    fn host(dir: &Path) -> PluginHost {
        PluginHost::load_with(crate::FIRST_PARTY, dir).unwrap()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tuxgt-e42-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn defs_are_well_formed_and_ordered() {
        let dir = temp_dir("defs");
        let h = host(&dir);
        let defs = wrapper_defs(&h);
        assert_eq!(
            defs.iter().map(|d| d.id).collect::<Vec<_>>(),
            ["gamescope", "gamemode", "mangohud"]
        );
        assert_eq!(
            defs.iter().map(|d| d.program).collect::<Vec<_>>(),
            ["gamescope", "gamemoderun", "mangohud"]
        );
        let mut seen = std::collections::BTreeSet::new();
        for d in defs {
            assert!(!d.label.is_empty(), "empty label: {}", d.id);
            assert!(!d.help.is_empty(), "empty help: {}", d.id);
            assert!(!d.program.is_empty(), "empty program: {}", d.id);
            let mut chars = d.id.chars();
            assert!(chars.next().is_some_and(|c| c.is_ascii_lowercase()));
            assert!(
                d.id.len() <= 32
                    && d.id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            );
            assert!(seen.insert(d.id), "duplicate wrapper id: {}", d.id);
        }
        assert!(find_wrapper("gamemode").is_some());
        assert!(find_wrapper("nope-wrapper").is_none());
        assert_eq!(WRAPPERS.len(), 3);
        assert!(wrapper_enabled(&h));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_plugin_offers_none() {
        let dir = temp_dir("disabled");
        let mut h = host(&dir);
        assert_eq!(wrapper_defs(&h).len(), 3);
        assert!(find_enabled_wrapper(&h, "gamemode").is_some());

        h.set_enabled("wrapper", false).unwrap();
        assert!(wrapper_defs(&h).is_empty());
        assert!(find_enabled_wrapper(&h, "gamemode").is_none());
        // table lookup stays intact; stored rows stay (checked below)
        assert!(find_wrapper("gamemode").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rows_roundtrip_and_survive_prune() {
        let dir = temp_dir("rows");
        let pool = crate::open_db(&dir).await.unwrap();
        let id = "manual:standalone:abcd1234";
        seed_game(
            &pool,
            SeedGame {
                id,
                ..Default::default()
            },
        )
        .await;

        // insert order does not matter; a repeat set upserts
        set_wrapper(&pool, id, "mangohud").await.unwrap();
        set_wrapper(&pool, id, "gamescope").await.unwrap();
        set_wrapper(&pool, id, "gamemode").await.unwrap();
        set_wrapper(&pool, id, "gamemode").await.unwrap();
        assert_eq!(
            game_wrappers(&pool, id).await.unwrap(),
            ["gamescope", "gamemode", "mangohud"]
        );

        assert!(matches!(
            set_wrapper(&pool, id, "nope").await.unwrap_err(),
            Error::UnknownWrapper(_)
        ));
        assert!(matches!(
            set_wrapper(&pool, "manual:standalone:nope0000", "gamemode")
                .await
                .unwrap_err(),
            Error::UnknownGame(_)
        ));

        // prune the game row, then re-add the same id: rows stay (E12 rule)
        sqlx::query("DELETE FROM games WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(game_wrappers(&pool, id).await.unwrap().len(), 3);
        seed_game(
            &pool,
            SeedGame {
                id,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(game_wrappers(&pool, id).await.unwrap().len(), 3);

        // a disabled plugin hides the defs but keeps the rows
        let mut h = host(&dir);
        h.set_enabled("wrapper", false).unwrap();
        assert!(wrapper_defs(&h).is_empty());
        assert_eq!(game_wrappers(&pool, id).await.unwrap().len(), 3);

        unset_wrapper(&pool, id, "gamemode").await.unwrap();
        assert!(matches!(
            unset_wrapper(&pool, id, "gamemode").await.unwrap_err(),
            Error::WrapperNotSet(_)
        ));
        assert_eq!(
            game_wrappers(&pool, id).await.unwrap(),
            ["gamescope", "mangohud"]
        );

        // rows are per game
        assert!(game_wrappers(&pool, "manual:standalone:zzzz9999")
            .await
            .unwrap()
            .is_empty());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
