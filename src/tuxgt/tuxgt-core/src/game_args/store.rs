//! Working copy, custom text, and the Save snapshot. Rows survive game deletion.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredArg {
    pub arg: String,
    pub value: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub args: Vec<StoredArg>,
    pub custom: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Published {
    pub tail: String,
    pub snapshot: Snapshot,
}

pub async fn migrate_game_args(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_args (
            game_id TEXT NOT NULL,
            arg TEXT NOT NULL,
            value TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (game_id, arg)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_args_custom (
            game_id TEXT PRIMARY KEY,
            args TEXT
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_args_published (
            game_id TEXT PRIMARY KEY,
            tail TEXT NOT NULL,
            snapshot TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn stored_args(pool: &SqlitePool, game_id: &str) -> Result<Vec<StoredArg>> {
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT arg, value, enabled FROM game_args WHERE game_id = ? ORDER BY arg")
            .bind(game_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(arg, value, enabled)| StoredArg {
            arg,
            value,
            enabled: enabled != 0,
        })
        .collect())
}

pub async fn upsert_arg(
    pool: &SqlitePool,
    game_id: &str,
    arg: &str,
    value: &str,
    enabled: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO game_args (game_id, arg, value, enabled) VALUES (?, ?, ?, ?)
         ON CONFLICT(game_id, arg) DO UPDATE SET value = excluded.value, enabled = excluded.enabled",
    )
    .bind(game_id)
    .bind(arg)
    .bind(value)
    .bind(i64::from(enabled))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_arg(pool: &SqlitePool, game_id: &str, arg: &str) -> Result<bool> {
    let r = sqlx::query("DELETE FROM game_args WHERE game_id = ? AND arg = ?")
        .bind(game_id)
        .bind(arg)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn custom_text(pool: &SqlitePool, game_id: &str) -> Result<String> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT args FROM game_args_custom WHERE game_id = ?")
            .bind(game_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0).unwrap_or_default())
}

pub async fn set_custom_text(pool: &SqlitePool, game_id: &str, args: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO game_args_custom (game_id, args) VALUES (?, ?)
         ON CONFLICT(game_id) DO UPDATE SET args = excluded.args",
    )
    .bind(game_id)
    .bind(args)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_custom_text(pool: &SqlitePool, game_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM game_args_custom WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn published(pool: &SqlitePool, game_id: &str) -> Result<Option<Published>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT tail, snapshot FROM game_args_published WHERE game_id = ?")
            .bind(game_id)
            .fetch_optional(pool)
            .await?;
    let Some((tail, snapshot)) = row else {
        return Ok(None);
    };
    let snapshot: Snapshot =
        serde_json::from_str(&snapshot).map_err(|e| Error::Args(format!("snapshot: {e}")))?;
    Ok(Some(Published { tail, snapshot }))
}

pub async fn upsert_published(
    pool: &SqlitePool,
    game_id: &str,
    tail: &str,
    snapshot: &Snapshot,
) -> Result<()> {
    let text =
        serde_json::to_string(snapshot).map_err(|e| Error::Args(format!("snapshot: {e}")))?;
    sqlx::query(
        "INSERT INTO game_args_published (game_id, tail, snapshot) VALUES (?, ?, ?)
         ON CONFLICT(game_id) DO UPDATE SET tail = excluded.tail, snapshot = excluded.snapshot",
    )
    .bind(game_id)
    .bind(tail)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
pub async fn delete_published(pool: &SqlitePool, game_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM game_args_published WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn replace_working(pool: &SqlitePool, game_id: &str, snap: &Snapshot) -> Result<()> {
    sqlx::query("DELETE FROM game_args WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    for a in &snap.args {
        upsert_arg(pool, game_id, &a.arg, &a.value, a.enabled).await?;
    }
    if snap.custom.is_empty() {
        clear_custom_text(pool, game_id).await?;
    } else {
        set_custom_text(pool, game_id, &snap.custom).await?;
    }
    Ok(())
}

pub async fn clear_working(pool: &SqlitePool, game_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM game_args WHERE game_id = ?")
        .bind(game_id)
        .execute(pool)
        .await?;
    clear_custom_text(pool, game_id).await?;
    Ok(())
}

/// Sync delete used from `restore_launch`. `restore_launch` is sync and is
/// often already inside the process runtime, so a nested `block_on` is unsafe.
/// A missing table (old db) is fine. A failed open fails the restore.
pub fn clear_published_sync(data_dir: &Path, game_id: &str) -> Result<()> {
    let path = data_dir.join("config").join("tuxgt.sqlite");
    if !path.is_file() {
        return Ok(());
    }
    let c_path = std::ffi::CString::new(path.to_string_lossy().as_ref())
        .map_err(|e| Error::Args(e.to_string()))?;
    let escaped = game_id.replace('\'', "''");
    let sql = std::ffi::CString::new(format!(
        "DELETE FROM game_args_published WHERE game_id = '{escaped}'"
    ))
    .map_err(|e| Error::Args(e.to_string()))?;
    unsafe {
        let mut db: *mut libsqlite3_sys::sqlite3 = std::ptr::null_mut();
        if libsqlite3_sys::sqlite3_open(c_path.as_ptr(), &mut db) != libsqlite3_sys::SQLITE_OK {
            let msg = cmsg(db);
            libsqlite3_sys::sqlite3_close(db);
            return Err(Error::Args(format!("open game args db: {msg}")));
        }
        let mut err: *mut std::os::raw::c_char = std::ptr::null_mut();
        let rc =
            libsqlite3_sys::sqlite3_exec(db, sql.as_ptr(), None, std::ptr::null_mut(), &mut err);
        if rc != libsqlite3_sys::SQLITE_OK {
            let msg = if err.is_null() {
                cmsg(db)
            } else {
                let s = std::ffi::CStr::from_ptr(err).to_string_lossy().into_owned();
                libsqlite3_sys::sqlite3_free(err.cast());
                s
            };
            libsqlite3_sys::sqlite3_close(db);
            if msg.contains("no such table") {
                return Ok(());
            }
            return Err(Error::Args(msg));
        }
        libsqlite3_sys::sqlite3_close(db);
    }
    Ok(())
}

unsafe fn cmsg(db: *mut libsqlite3_sys::sqlite3) -> String {
    if db.is_null() {
        return "sqlite open failed".into();
    }
    let p = libsqlite3_sys::sqlite3_errmsg(db);
    if p.is_null() {
        return "sqlite error".into();
    }
    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
}
