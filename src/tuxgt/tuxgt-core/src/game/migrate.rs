use std::collections::HashSet;
use std::path::Path;

use sqlx::SqlitePool;

use super::*;
use crate::Result;

/// E42: per-game overlay wrappers. Sidecar keyed by game id, no FK, so rows
/// survive game-row prune/re-add (E12 rule).
pub async fn migrate_wrappers(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS game_wrappers (
            game_id TEXT NOT NULL,
            wrapper TEXT NOT NULL,
            PRIMARY KEY(game_id, wrapper)
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// R62: true when `e` is SQLite's "duplicate column" error, i.e. a
/// concurrent `migrate_games` added the column after our PRAGMA snapshot.
pub(crate) fn is_duplicate_column(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .map(|d| d.message().to_lowercase().contains("duplicate column"))
        .unwrap_or(false)
}

pub async fn migrate_games(pool: &SqlitePool, data_dir: &Path) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS games (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT
        )",
    )
    .execute(pool)
    .await?;

    let cols: Vec<(i32, String, String, i32, Option<String>, i32)> =
        sqlx::query_as("PRAGMA table_info(games)")
            .fetch_all(pool)
            .await?;
    let have: HashSet<String> = cols.into_iter().map(|r| r.1).collect();
    for (name, decl) in [
        ("manager", "TEXT NOT NULL DEFAULT ''"),
        ("store", "TEXT NOT NULL DEFAULT ''"),
        ("game_id", "TEXT NOT NULL DEFAULT ''"),
        ("install_dir", "TEXT"),
        ("cover_path", "TEXT"),
        ("header_path", "TEXT"),
        ("exe_path", "TEXT"),
        ("prefix_path", "TEXT"),
        ("proton", "TEXT"),
        ("build", "TEXT"),
        ("launch_options", "TEXT"),
        ("env", "TEXT"),
        ("wrapper", "TEXT"),
        ("detected_exe_path", "TEXT"),
        ("detected_platform", "TEXT"),
        ("detected_bitness", "TEXT"),
        ("detected_api", "TEXT"),
        ("detected_extra_apis", "TEXT"),
        ("detected_engine", "TEXT"),
        ("detected_prefix_path", "TEXT"),
        ("detected_proton", "TEXT"),
        ("detected_build", "TEXT"),
        ("detected_exe_version", "TEXT"),
        ("detected_hidden", "INTEGER"),
        ("override_exe_path", "TEXT"),
        ("override_platform", "TEXT"),
        ("override_bitness", "TEXT"),
        ("override_api", "TEXT"),
        ("override_extra_apis", "TEXT"),
        ("override_engine", "TEXT"),
        ("override_prefix_path", "TEXT"),
        ("override_proton", "TEXT"),
        ("override_build", "TEXT"),
        ("override_exe_version", "TEXT"),
        ("override_hidden", "INTEGER"),
        ("fingerprint", "TEXT"),
        // R01: last-played timestamp (unix seconds, NULL = never played).
        // Scan upsert never writes it; only touch_last_played does.
        ("last_played", "INTEGER"),
        // E43: user Steam-AppID overlay for Heroic/manual rows (ProtonDB /
        // SteamGridDB). Scan never writes it; see `set_steam_appid`.
        ("steam_appid", "TEXT"),
    ] {
        if !have.contains(name) {
            let sql = format!("ALTER TABLE games ADD COLUMN {name} {decl}");
            // A concurrent migrate may have added the column after our
            // PRAGMA snapshot; that is the schema we want, so tolerate it
            // and keep the single-connection path identical. Anything else
            // still errors.
            match sqlx::query(&sql).execute(pool).await {
                Ok(_) => {}
                Err(e) if is_duplicate_column(&e) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS games_fts USING fts5(
            name,
            content='games',
            content_rowid='rowid'
        )",
    )
    .execute(pool)
    .await?;

    migrate_wrappers(pool).await?;

    // B02: one `standalone` store id. Steam owned rows keep their empty
    // store; only the standalone class moves.
    let legacy: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, manager, store, game_id FROM games
         WHERE (manager = 'steam' AND store = 'shortcut')
            OR (manager = 'heroic' AND store = 'sideload')
            OR (manager = 'manual' AND store = '')",
    )
    .fetch_all(pool)
    .await?;
    let mut renames: Vec<(String, String)> = Vec::new();
    for (id, manager, store, game) in &legacy {
        let Some(new) = standalone_id(manager, store, game) else {
            continue;
        };
        let taken: bool = sqlx::query_as::<_, (String,)>("SELECT id FROM games WHERE id = ?")
            .bind(&new)
            .fetch_optional(pool)
            .await?
            .is_some();
        if taken {
            sqlx::query("DELETE FROM games WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?;
        } else {
            sqlx::query("UPDATE games SET id = ?, store = ? WHERE id = ?")
                .bind(&new)
                .bind(STANDALONE)
                .bind(id)
                .execute(pool)
                .await?;
        }
        // Sidecar game ids: metadata, per-game env, per-game wrappers.
        // Tables may not exist yet on a fresh db (their own migrations run
        // after this one); the sqlite_master guard skips them.
        for table in [
            "metadata_cache",
            "env_knobs",
            "env_custom",
            "game_wrappers",
            "game_handle",
        ] {
            let here: bool = sqlx::query_as::<_, (String,)>(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_optional(pool)
            .await?
            .is_some();
            if here {
                let sql = format!("UPDATE {table} SET game_id = ? WHERE game_id = ?");
                sqlx::query(&sql).bind(&new).bind(id).execute(pool).await?;
            }
        }
        renames.push((id.clone(), new));
    }
    migrate_standalone_files(data_dir, &renames);
    migrate_art_cache(pool, data_dir).await;
    Ok(())
}

/// Rename per-game dirs for B02-migrated rows and rewrite the game id
/// inside them (manifests, staging tomls, apply records). Missing paths
/// are skipped; an existing target wins, never destroy user data.
pub(crate) fn migrate_standalone_files(data_dir: &Path, renames: &[(String, String)]) {
    for (old, new) in renames {
        let (Ok(oid), Ok(nid)) = (GameId::parse(old), GameId::parse(new)) else {
            continue;
        };
        let (ofrom, oto) = (game_dir(data_dir, &oid), game_dir(data_dir, &nid));
        if ofrom == oto {
            continue;
        }
        if ofrom.exists() && !oto.exists() {
            if let Some(parent) = oto.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::rename(&ofrom, &oto) {
                tracing::warn!(error = %e, "standalone migrate game dir");
                continue;
            }
        }
        if let Ok(rd) = std::fs::read_dir(oto.join("manifests")) {
            for e in rd.flatten() {
                let p = e.path();
                if !p.extension().is_some_and(|x| x == "toml") {
                    continue;
                }
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                match crate::download::read_manifest(data_dir, new, stem) {
                    Ok(Some(mut m)) => {
                        if m.game == *old {
                            m.game = new.clone();
                            if let Err(e) = crate::download::write_manifest(data_dir, &m) {
                                tracing::warn!(error = %e, "standalone migrate manifest body");
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "standalone migrate manifest body"),
                    Ok(None) => {}
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(oto.join("stage")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "toml") {
                    rewrite_toml_game(&p, old, new);
                }
            }
        }
        match crate::apply::read_record(data_dir, new) {
            Ok(Some(mut r)) => {
                if r.game == *old {
                    r.game = new.clone();
                    if let Err(e) = crate::apply::write_record(data_dir, &r) {
                        tracing::warn!(error = %e, "standalone migrate apply body");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, "standalone migrate apply body"),
            Ok(None) => {}
        }
    }
}

/// One-shot: covers in `downloads/` → `config/cache/art/` when the game
/// still exists; drop leftover art dirs and spent download hashes.
pub(crate) async fn migrate_art_cache(pool: &SqlitePool, data_dir: &Path) {
    let rows: Vec<(String, Option<String>, Option<String>)> =
        match sqlx::query_as("SELECT id, cover_path, header_path FROM games")
            .fetch_all(pool)
            .await
        {
            Ok(r) => r,
            Err(_) => return,
        };
    let live: std::collections::BTreeSet<String> = rows
        .iter()
        .map(|(id, _, _)| id.replace([':', '/'], "_"))
        .collect();
    if let Ok(rd) = std::fs::read_dir(crate::download::art_dir(data_dir)) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !live.contains(&name) {
                let _ = std::fs::remove_dir_all(&p);
            }
        }
    }
    for (id, cover, header) in rows {
        let url = [cover.as_deref(), header.as_deref()]
            .into_iter()
            .flatten()
            .find(|s| s.starts_with("http://") || s.starts_with("https://"));
        let Some(url) = url else {
            continue;
        };
        let src = crate::download::cached_file(data_dir, url);
        if src.is_file() && !crate::download::art_file(data_dir, &id).is_file() {
            crate::download::land_art(data_dir, &id, &src);
        }
    }
    // One-shot: leftover hash dirs from the old cache-forever layout.
    // Later opens must not wipe an in-flight acquire.
    let mark = data_dir.join("config").join(".layout-mods");
    if !mark.is_file() {
        crate::download::drop_spent_downloads(data_dir);
        let _ = std::fs::create_dir_all(data_dir.join("config"));
        let _ = std::fs::write(&mark, "2\n");
    }
}

/// Point one migrated TOML's `game` key at the new id. Best effort.
pub(crate) fn rewrite_toml_game(path: &Path, old: &str, new: &str) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(mut v) = toml::from_str::<toml::Value>(&text) else {
        tracing::warn!(file = %path.display(), "standalone migrate toml unreadable");
        return;
    };
    if v.get("game").and_then(|g| g.as_str()) != Some(old) {
        return;
    }
    v["game"] = toml::Value::String(new.to_string());
    let Ok(text) = toml::to_string(&v) else {
        return;
    };
    if let Err(e) = std::fs::write(path, text) {
        tracing::warn!(error = %e, "standalone migrate toml write");
    }
}

pub async fn rebuild_fts(pool: &SqlitePool) -> Result<()> {
    sqlx::query("INSERT INTO games_fts(games_fts) VALUES('rebuild')")
        .execute(pool)
        .await?;
    Ok(())
}
