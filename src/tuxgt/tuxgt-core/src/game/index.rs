use std::collections::BTreeMap;
use std::collections::BTreeSet;

use sqlx::SqlitePool;

use super::*;
use crate::detect::{detect_all, DetectOpts};
use crate::provider::{GameProvider, GameRecord, GAME_PROVIDERS};
use crate::{Error, PluginHost, Result};

pub(crate) const GAME_ROW_COLS: &str = "id, name, cover_path, manager, store, header_path,
    COALESCE(NULLIF(override_platform, ''), detected_platform) AS platform,
    COALESCE(NULLIF(override_api, ''), detected_api) AS api,
    install_dir,
    COALESCE(NULLIF(override_exe_path, ''), detected_exe_path, exe_path) AS exe_path,
    COALESCE(NULLIF(override_prefix_path, ''), detected_prefix_path, prefix_path) AS prefix_path,
    COALESCE(NULLIF(override_proton, ''), detected_proton, proton) AS proton,
    COALESCE(NULLIF(override_bitness, ''), detected_bitness) AS bitness,
    COALESCE(NULLIF(override_engine, ''), detected_engine) AS engine,
    COALESCE(override_hidden, detected_hidden, 0) AS hidden, last_played, steam_appid";

pub async fn list_games(
    pool: &SqlitePool,
    manager: Option<&str>,
    store: Option<&str>,
    query: Option<&str>,
) -> Result<Vec<GameRow>> {
    let q = query.map(str::trim).filter(|s| !s.is_empty());
    if let Some(q) = q {
        Ok(sqlx::query_as::<_, GameRow>(&format!(
"SELECT g.id, g.name, g.cover_path, g.manager, g.store, g.header_path,
                    COALESCE(NULLIF(g.override_platform, ''), g.detected_platform) AS platform,
                    COALESCE(NULLIF(g.override_api, ''), g.detected_api) AS api,
                    g.install_dir,
                    COALESCE(NULLIF(g.override_exe_path, ''), g.detected_exe_path, g.exe_path) AS exe_path,
                    COALESCE(NULLIF(g.override_prefix_path, ''), g.detected_prefix_path, g.prefix_path) AS prefix_path,
                    COALESCE(NULLIF(g.override_proton, ''), g.detected_proton, g.proton) AS proton,
                    COALESCE(NULLIF(g.override_bitness, ''), g.detected_bitness) AS bitness,
                    COALESCE(NULLIF(g.override_engine, ''), g.detected_engine) AS engine,
                    COALESCE(g.override_hidden, g.detected_hidden, 0) AS hidden,
                    g.last_played, g.steam_appid
             FROM games g
             JOIN games_fts f ON f.rowid = g.rowid
             WHERE games_fts MATCH ?
               AND (? IS NULL OR g.manager = ?)
               AND (? IS NULL OR g.store = ?)
             ORDER BY g.name COLLATE NOCASE, g.id"
        ))
        .bind(q)
        .bind(manager)
        .bind(manager)
        .bind(store)
        .bind(store)
        .fetch_all(pool)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, GameRow>(&format!(
            "SELECT {GAME_ROW_COLS}
             FROM games
             WHERE (? IS NULL OR manager = ?)
               AND (? IS NULL OR store = ?)
             ORDER BY name COLLATE NOCASE, id"
        ))
        .bind(manager)
        .bind(manager)
        .bind(store)
        .bind(store)
        .fetch_all(pool)
        .await?)
    }
}

/// Slim library index: same order as `list_games` so positions align.
/// The GUI holds this always; full rows load per page.
pub async fn list_game_index(pool: &SqlitePool) -> Result<Vec<GameIndexRow>> {
    Ok(sqlx::query_as::<_, GameIndexRow>(
        "SELECT id, name, manager,
            COALESCE(override_hidden, detected_hidden, 0) AS hidden
         FROM games
         ORDER BY name COLLATE NOCASE, id",
    )
    .fetch_all(pool)
    .await?)
}

/// One full game row, for page-scoped loads (selected game, mint targets).
pub async fn game_row_by_id(pool: &SqlitePool, id: &str) -> Result<Option<GameRow>> {
    Ok(
        sqlx::query_as::<_, GameRow>(&format!("SELECT {GAME_ROW_COLS} FROM games WHERE id = ?"))
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Cached store launch config for the About read-only reference
/// (scan-time row, not a live store read).
#[derive(Clone, Debug, Default, sqlx::FromRow)]
pub struct GameLaunchConfig {
    pub launch_options: Option<String>,
    pub env: Option<String>,
    pub wrapper: Option<String>,
}

impl GameLaunchConfig {
    /// Stored env as sorted `KEY=VALUE` display lines (the Heroic JSON
    /// object, key-sorted). Unparseable rows fall back to the raw string
    /// as one line; empty when unset.
    pub fn env_lines(&self) -> Vec<String> {
        let Some(raw) = self.env.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
            return vec![];
        };
        match serde_json::from_str::<BTreeMap<String, serde_json::Value>>(raw) {
            Ok(map) => map
                .into_iter()
                .map(|(k, v)| match v {
                    serde_json::Value::String(s) => format!("{k}={s}"),
                    serde_json::Value::Null => format!("{k}="),
                    other => format!("{k}={other}"),
                })
                .collect(),
            Err(_) => vec![raw.to_string()],
        }
    }
}

/// Cached launch config for one game. Unknown id → `Error::UnknownGame`.
pub async fn game_launch_config(pool: &SqlitePool, id: &str) -> Result<GameLaunchConfig> {
    let row: Option<GameLaunchConfig> =
        sqlx::query_as("SELECT launch_options, env, wrapper FROM games WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    row.ok_or_else(|| Error::UnknownGame(id.into()))
}

pub async fn game_exists(pool: &SqlitePool, id: &str) -> Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM games WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

pub async fn scan_games(pool: &SqlitePool, host: &PluginHost) -> Result<Vec<GameRow>> {
    scan_games_opts(pool, host, DetectOpts::default()).await
}

pub async fn scan_games_opts(
    pool: &SqlitePool,
    host: &PluginHost,
    opts: DetectOpts,
) -> Result<Vec<GameRow>> {
    let before: BTreeSet<String> =
        sqlx::query_as::<_, (String,)>("SELECT id FROM games")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(id,)| id)
            .collect();
    let games = scan_with_opts(pool, host, GAME_PROVIDERS, opts).await?;
    let after: BTreeSet<String> = games.iter().map(|g| g.id.clone()).collect();
    tracing::info!(
        found = games.len(),
        added = after.difference(&before).count(),
        removed = before.difference(&after).count(),
        "scan done"
    );
    if after != before {
        crate::db::mark_cache_dirty();
    }
    Ok(games)
}

pub async fn scan_with(
    pool: &SqlitePool,
    host: &PluginHost,
    providers: &[&dyn GameProvider],
) -> Result<Vec<GameRow>> {
    scan_with_opts(pool, host, providers, DetectOpts::default()).await
}

pub async fn scan_with_opts(
    pool: &SqlitePool,
    host: &PluginHost,
    providers: &[&dyn GameProvider],
    opts: DetectOpts,
) -> Result<Vec<GameRow>> {
    for p in providers {
        if !host.is_enabled(p.plugin_id()) {
            continue;
        }
        let recs = match p.scan() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(plugin = p.plugin_id(), error = %e, "game provider scan failed");
                continue;
            }
        };
        match p.plugin_id() {
            "manual" => {}
            manager => upsert_and_prune(pool, manager, &recs).await?,
        }
    }
    rebuild_fts(pool).await?;
    detect_all(pool, opts).await?;
    list_games(pool, None, None, None).await
}

pub(crate) async fn upsert_and_prune(
    pool: &SqlitePool,
    manager: &str,
    recs: &[GameRecord],
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for rec in recs {
        let id = rec.id.to_string();
        seen.insert(id.clone());
        upsert_record(pool, rec).await?;
    }
    let existing: Vec<(String,)> = sqlx::query_as("SELECT id FROM games WHERE manager = ?")
        .bind(manager)
        .fetch_all(pool)
        .await?;
    for (id,) in existing {
        if !seen.contains(&id) {
            sqlx::query("DELETE FROM games WHERE id = ?")
                .bind(&id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn upsert_record(pool: &SqlitePool, rec: &GameRecord) -> Result<()> {
    let id = rec.id.to_string();
    // detected_hidden refreshes every scan; override_hidden is never touched
    // here so a user override survives rescan (override wins in GAME_ROW_COLS).
    // E43: `steam_appid` is INSERT-only (always NULL from a scan record) and
    // deliberately absent from the DO UPDATE SET list, so the user overlay
    // survives rescan.
    sqlx::query(
        "INSERT INTO games (id, manager, store, game_id, name, install_dir, cover_path, header_path,
            exe_path, prefix_path, proton, build, launch_options, env, wrapper, detected_hidden,
            steam_appid)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            manager = excluded.manager,
            store = excluded.store,
            game_id = excluded.game_id,
            name = excluded.name,
            install_dir = excluded.install_dir,
            cover_path = excluded.cover_path,
            header_path = excluded.header_path,
            exe_path = excluded.exe_path,
            prefix_path = excluded.prefix_path,
            proton = excluded.proton,
            build = excluded.build,
            launch_options = excluded.launch_options,
            env = excluded.env,
            wrapper = excluded.wrapper,
            detected_hidden = excluded.detected_hidden",
    )
    .bind(&id)
    .bind(&rec.id.manager)
    .bind(&rec.id.store)
    .bind(&rec.id.game)
    .bind(&rec.name)
    .bind(rec.install_dir.as_ref().map(|p| p.to_string_lossy().into_owned()))
    .bind(rec.cover_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    .bind(rec.header_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    .bind(rec.exe_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    .bind(rec.prefix_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    .bind(&rec.proton)
    .bind(&rec.build)
    .bind(&rec.launch_options)
    .bind(&rec.env)
    .bind(&rec.wrapper)
    .bind(if rec.hidden { 1 } else { 0 })
    .bind(None::<String>)
    .execute(pool)
    .await?;
    Ok(())
}

/// Per-game hidden override: `Some(true)` hides, `Some(false)` forces visible,
/// `None` clears back to the detected value. Never touches `detected_hidden`,
/// so rescans cannot clobber it.
pub async fn set_hidden_override(pool: &SqlitePool, id: &str, hidden: Option<bool>) -> Result<()> {
    GameId::parse(id)?;
    if !game_exists(pool, id).await? {
        return Err(Error::UnknownGame(id.into()));
    }
    sqlx::query("UPDATE games SET override_hidden = ? WHERE id = ?")
        .bind(hidden.map(|h| if h { 1 } else { 0 }))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
