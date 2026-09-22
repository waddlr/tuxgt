use sqlx::SqlitePool;

use super::*;
use crate::{Error, Result};

/// The three env k=v tables behind one storage owner. Schemas stay as they
/// are: game knobs and globals carry an `enabled` flag, custom pairs are
/// always on, globals have no game column. Table/column names below are the
/// only SQL identifiers built by format; all values are bound.
#[derive(Clone, Copy)]
pub(crate) enum KvTable {
    GameKnobs,
    Custom,
    GlobalKnobs,
}

impl KvTable {
    const ALL: [KvTable; 3] = [KvTable::GameKnobs, KvTable::Custom, KvTable::GlobalKnobs];

    fn name(self) -> &'static str {
        match self {
            KvTable::GameKnobs => "env_knobs",
            KvTable::Custom => "env_custom",
            KvTable::GlobalKnobs => "env_knobs_global",
        }
    }

    fn ddl(self) -> &'static str {
        match self {
            KvTable::GameKnobs => {
                "CREATE TABLE IF NOT EXISTS env_knobs (
                    game_id TEXT NOT NULL,
                    knob TEXT NOT NULL,
                    value TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    PRIMARY KEY(game_id, knob)
                )"
            }
            KvTable::Custom => {
                "CREATE TABLE IF NOT EXISTS env_custom (
                    game_id TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    PRIMARY KEY(game_id, key)
                )"
            }
            KvTable::GlobalKnobs => {
                "CREATE TABLE IF NOT EXISTS env_knobs_global (
                    knob TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1
                )"
            }
        }
    }

    fn key_col(self) -> &'static str {
        match self {
            KvTable::Custom => "key",
            KvTable::GameKnobs | KvTable::GlobalKnobs => "knob",
        }
    }

    fn has_game(self) -> bool {
        !matches!(self, KvTable::GlobalKnobs)
    }

    fn has_enabled(self) -> bool {
        !matches!(self, KvTable::Custom)
    }

    fn predicate(self) -> String {
        if self.has_game() {
            format!("game_id = ? AND {} = ?", self.key_col())
        } else {
            format!("{} = ?", self.key_col())
        }
    }
}

pub async fn migrate_env(pool: &SqlitePool) -> Result<()> {
    for table in KvTable::ALL {
        sqlx::query(table.ddl()).execute(pool).await?;
    }
    let cols: Vec<(i32, String, String, i32, Option<String>, i32)> =
        sqlx::query_as("PRAGMA table_info(env_knobs)")
            .fetch_all(pool)
            .await?;
    if !cols.iter().any(|c| c.1 == "enabled") {
        sqlx::query("ALTER TABLE env_knobs ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1")
            .execute(pool)
            .await?;
    }
    super::global::retire_environment_d()?;
    Ok(())
}

fn check_scope(table: KvTable, game_id: Option<&str>) {
    debug_assert_eq!(table.has_game(), game_id.is_some());
}

/// Upsert one k=v pair, enabling the row. `game_id` is `Some` except for
/// globals, where it is `None`.
pub(crate) async fn kv_store(
    pool: &SqlitePool,
    table: KvTable,
    game_id: Option<&str>,
    key: &str,
    value: &str,
) -> Result<()> {
    check_scope(table, game_id);
    let k = table.key_col();
    let game_col = if table.has_game() { "game_id, " } else { "" };
    let flag_col = if table.has_enabled() { ", enabled" } else { "" };
    let flag_val = if table.has_enabled() { ", 1" } else { "" };
    let flag_set = if table.has_enabled() {
        ", enabled = 1"
    } else {
        ""
    };
    let conflict = format!("{game_col}{k}");
    let holes = if table.has_game() { "?, ?, ?" } else { "?, ?" };
    let sql = format!(
        "INSERT INTO {t} ({game_col}{k}, value{flag_col}) VALUES ({holes}{flag_val}) \
         ON CONFLICT({conflict}) DO UPDATE SET value = excluded.value{flag_set}",
        t = table.name(),
    );
    let mut q = sqlx::query(&sql);
    if let Some(g) = game_id {
        q = q.bind(g);
    }
    q.bind(key).bind(value).execute(pool).await?;
    Ok(())
}

/// One stored row. Custom pairs report `enabled` (they have no flag).
pub(crate) async fn kv_get(
    pool: &SqlitePool,
    table: KvTable,
    game_id: Option<&str>,
    key: &str,
) -> Result<Option<KnobRow>> {
    check_scope(table, game_id);
    // Custom pairs have no enable flag; selecting a literal `1` keeps one
    // row shape for all three tables.
    let cols = if table.has_enabled() {
        format!("{}, value, enabled", table.key_col())
    } else {
        format!("{}, value, 1", table.key_col())
    };
    let sql = format!(
        "SELECT {cols} FROM {t} WHERE {p}",
        t = table.name(),
        p = table.predicate(),
    );
    let mut q = sqlx::query_as::<_, (String, String, i64)>(&sql);
    if let Some(g) = game_id {
        q = q.bind(g);
    }
    let row: Option<(String, String, i64)> = q.bind(key).fetch_optional(pool).await?;
    Ok(row.map(|(knob, value, enabled)| KnobRow {
        knob,
        value,
        enabled: enabled != 0,
    }))
}

/// All rows in key order.
pub(crate) async fn kv_list(
    pool: &SqlitePool,
    table: KvTable,
    game_id: Option<&str>,
) -> Result<Vec<KnobRow>> {
    check_scope(table, game_id);
    // Same literal-`1` row shape as `kv_get`.
    let cols = if table.has_enabled() {
        format!("{}, value, enabled", table.key_col())
    } else {
        format!("{}, value, 1", table.key_col())
    };
    let sql = format!(
        "SELECT {cols} FROM {t}{w} ORDER BY {k}",
        t = table.name(),
        w = if table.has_game() {
            " WHERE game_id = ?"
        } else {
            ""
        },
        k = table.key_col(),
    );
    let mut q = sqlx::query_as::<_, (String, String, i64)>(&sql);
    if let Some(g) = game_id {
        q = q.bind(g);
    }
    let rows: Vec<(String, String, i64)> = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(knob, value, enabled)| KnobRow {
            knob,
            value,
            enabled: enabled != 0,
        })
        .collect())
}

/// Delete one row; false when unset.
pub(crate) async fn kv_remove(
    pool: &SqlitePool,
    table: KvTable,
    game_id: Option<&str>,
    key: &str,
) -> Result<bool> {
    check_scope(table, game_id);
    let sql = format!(
        "DELETE FROM {t} WHERE {p}",
        t = table.name(),
        p = table.predicate()
    );
    let mut q = sqlx::query(&sql);
    if let Some(g) = game_id {
        q = q.bind(g);
    }
    let res = q.bind(key).execute(pool).await?;
    Ok(res.rows_affected() > 0)
}

/// Flip the enable flag, keeping the value; false when unset. Custom pairs
/// have no flag — callers never pass `KvTable::Custom` here.
pub(crate) async fn kv_set_enabled(
    pool: &SqlitePool,
    table: KvTable,
    game_id: Option<&str>,
    key: &str,
    on: bool,
) -> Result<bool> {
    check_scope(table, game_id);
    debug_assert!(table.has_enabled());
    let sql = format!(
        "UPDATE {t} SET enabled = {on} WHERE {p}",
        t = table.name(),
        on = if on { 1 } else { 0 },
        p = table.predicate(),
    );
    let mut q = sqlx::query(&sql);
    if let Some(g) = game_id {
        q = q.bind(g);
    }
    let res = q.bind(key).execute(pool).await?;
    Ok(res.rows_affected() > 0)
}

/// One storage owner for game-scoped k=v with inherit: game knob first,
/// then game custom pair, then global knob. Knob ids and custom `VAR`
/// names live in different key spaces, so the order only matters on
/// collision, where the game row wins. Reports storage, not effective
/// policy — a disabled game row still returns `Game`; see `knob_source`.
pub async fn env_keys(pool: &SqlitePool, game_id: &str, key: &str) -> Result<Option<EnvKey>> {
    if let Some(row) = kv_get(pool, KvTable::GameKnobs, Some(game_id), key).await? {
        return Ok(Some(EnvKey {
            value: row.value,
            enabled: row.enabled,
            scope: EnvScope::Game,
        }));
    }
    if let Some(row) = kv_get(pool, KvTable::Custom, Some(game_id), key).await? {
        return Ok(Some(EnvKey {
            value: row.value,
            enabled: true,
            scope: EnvScope::Custom,
        }));
    }
    if let Some(row) = kv_get(pool, KvTable::GlobalKnobs, None, key).await? {
        return Ok(Some(EnvKey {
            value: row.value,
            enabled: row.enabled,
            scope: EnvScope::Global,
        }));
    }
    Ok(None)
}

pub async fn knob_rows(pool: &SqlitePool, game_id: &str) -> Result<Vec<KnobRow>> {
    kv_list(pool, KvTable::GameKnobs, Some(game_id)).await
}

/// Counts for the game hero without loading rows: set knobs (enabled with a
/// non-empty value) and custom pairs.
pub async fn count_set_env(pool: &SqlitePool, game_id: &str) -> Result<(usize, usize)> {
    let knobs: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM env_knobs WHERE game_id = ? AND enabled != 0 AND value <> ''",
    )
    .bind(game_id)
    .fetch_one(pool)
    .await?;
    let custom: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM env_custom WHERE game_id = ?")
        .bind(game_id)
        .fetch_one(pool)
        .await?;
    Ok((knobs.0.max(0) as usize, custom.0.max(0) as usize))
}

pub async fn knob_values(pool: &SqlitePool, game_id: &str) -> Result<Vec<(String, String)>> {
    Ok(knob_rows(pool, game_id)
        .await?
        .into_iter()
        .map(|r| (r.knob, r.value))
        .collect())
}

pub async fn set_knob(pool: &SqlitePool, game_id: &str, knob: &str, value: &str) -> Result<()> {
    kv_store(pool, KvTable::GameKnobs, Some(game_id), knob, value).await
}

/// Enable a stored game knob. Unset is a no-op (still writes nothing).
pub async fn enable_knob(pool: &SqlitePool, game_id: &str, knob: &str) -> Result<()> {
    kv_set_enabled(pool, KvTable::GameKnobs, Some(game_id), knob, true).await?;
    Ok(())
}

/// Disable a stored game knob (keep value). Unset → `KnobNotSet`.
pub async fn disable_knob(pool: &SqlitePool, game_id: &str, knob: &str) -> Result<()> {
    if !kv_set_enabled(pool, KvTable::GameKnobs, Some(game_id), knob, false).await? {
        return Err(Error::KnobNotSet(knob.into()));
    }
    Ok(())
}

pub async fn unset_knob(pool: &SqlitePool, game_id: &str, knob: &str) -> Result<()> {
    if !kv_remove(pool, KvTable::GameKnobs, Some(game_id), knob).await? {
        return Err(Error::KnobNotSet(knob.into()));
    }
    Ok(())
}

pub async fn custom_env(pool: &SqlitePool, game_id: &str) -> Result<Vec<(String, String)>> {
    Ok(kv_list(pool, KvTable::Custom, Some(game_id))
        .await?
        .into_iter()
        .map(|r| (r.knob, r.value))
        .collect())
}

pub async fn set_custom(pool: &SqlitePool, game_id: &str, key: &str, value: &str) -> Result<()> {
    kv_store(pool, KvTable::Custom, Some(game_id), key, value).await
}

pub async fn remove_custom(pool: &SqlitePool, game_id: &str, key: &str) -> Result<()> {
    if !kv_remove(pool, KvTable::Custom, Some(game_id), key).await? {
        return Err(Error::CustomEnvNotSet(key.into()));
    }
    Ok(())
}

pub async fn global_knobs(pool: &SqlitePool) -> Result<Vec<KnobRow>> {
    kv_list(pool, KvTable::GlobalKnobs, None).await
}

pub async fn set_global_knob(pool: &SqlitePool, knob: &str, value: &str) -> Result<()> {
    refuse_unmanaged_global(pool, knob).await?;
    kv_store(pool, KvTable::GlobalKnobs, None, knob, value).await
}

pub async fn unset_global_knob(pool: &SqlitePool, knob: &str) -> Result<()> {
    refuse_unmanaged_global(pool, knob).await?;
    if !kv_remove(pool, KvTable::GlobalKnobs, None, knob).await? {
        return Err(Error::KnobNotSet(knob.into()));
    }
    Ok(())
}

pub async fn enable_global_knob(pool: &SqlitePool, knob: &str) -> Result<()> {
    refuse_unmanaged_global(pool, knob).await?;
    kv_set_enabled(pool, KvTable::GlobalKnobs, None, knob, true).await?;
    Ok(())
}

pub async fn disable_global_knob(pool: &SqlitePool, knob: &str) -> Result<()> {
    refuse_unmanaged_global(pool, knob).await?;
    if !kv_set_enabled(pool, KvTable::GlobalKnobs, None, knob, false).await? {
        return Err(Error::KnobNotSet(knob.into()));
    }
    Ok(())
}

/// Refuse global writes on unmanaged knobs: process env matches a registered
/// knob and there is no enabled global row. Unknown knobs still belong to the
/// CLI `UnknownKnob` gate, so they pass through here.
async fn refuse_unmanaged_global(pool: &SqlitePool, knob: &str) -> Result<()> {
    let Some(def) = find_knob(knob) else {
        return Ok(());
    };
    let row = kv_get(pool, KvTable::GlobalKnobs, None, knob).await?;
    let enabled_value = row.filter(|r| r.enabled).map(|r| r.value);
    if knob_is_unmanaged(def, enabled_value.as_deref()) {
        return Err(Error::KnobUnmanaged(knob.into()));
    }
    Ok(())
}
