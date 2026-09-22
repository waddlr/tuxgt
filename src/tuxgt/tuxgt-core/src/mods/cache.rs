use std::path::Path;

use sqlx::SqlitePool;

use crate::instance::list_mods;
use crate::Result;

/// T13 verdict: the SQLite mods table is a CACHE, not the truth — renamed
/// `mods_cache` to say so. Evidence: (1) `open_db` rebuilds every row from
/// files on every open (`reconcile_mod_cache`); (2) the old per-mutator
/// touch recomputed rows from files too (never an independent write), and
/// sync recipe mutators never wrote at all; (3) paint reads the TOMLs
/// (`load_instances`, `load_mods_for`, previews), never this table for
/// labels/content — the poll only borrows its sha/installed snapshot;
/// (4) the SQLite-only columns (`last_check`/`update_status`/`update_detail`)
/// are write-only poll scratch (no product reader). Single writer of the
/// derived columns: reconcile-at-open. Mutators (install/uninstall/refresh)
/// must NOT write here; the poll always opens first, so it always reads
/// fresh rows.
pub async fn migrate_mod_cache(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS mods_cache (
            id            TEXT PRIMARY KEY NOT NULL,
            kind          TEXT NOT NULL,
            asset_sha256  TEXT NOT NULL DEFAULT '',
            source        TEXT NOT NULL DEFAULT '',
            installed     INTEGER NOT NULL DEFAULT 0,
            last_check    INTEGER,
            update_status TEXT,
            update_detail TEXT
        )",
    )
    .execute(pool)
    .await?;
    // T13 rename: the stale pre-rename table is never read again.
    sqlx::query("DROP TABLE IF EXISTS mods")
        .execute(pool)
        .await?;
    Ok(())
}

/// Outcome of [`check_catalog_update`]: the catalog-level status of one Mod,
/// without naming any game. Per-game Available stays a local compare
/// (`manifest.provenance.asset_sha256` vs this row); `check_update` stays
/// the CLI per-game path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogStatus {
    /// Catalog payload matches the recorded version (or no version known).
    UpToDate,
    /// Upstream moved on; reinstall picks it up.
    Available { detail: String },
    /// Nothing to compare (never downloaded / unresolvable upstream).
    Unknown { reason: String },
}

/// Rebuild the cache from files: `list_mods` + walk manifests;
/// insert/delete/recount; refresh sha from payload `.provenance.toml`.
/// Runs on every fresh [`open_db`](crate::db::open_db); a cached
/// [`open_db_shared`](crate::db::open_db_shared) hit rebuilds only when a
/// mutator marked the inputs dirty (or the 60s backstop elapsed). Mutators
/// call [`mark_cache_dirty`](crate::db::mark_cache_dirty) on success instead
/// of touching rows (T13).
/// One catalog row the E104 GUI poll reads: id + label source of truth stays
/// in the recipe files; the cache carries counts, payload version, and the
/// last catalog-check outcome.
#[derive(Clone, Debug)]
pub struct ModCacheRow {
    pub id: String,
    pub kind: String,
    pub asset_sha256: String,
    pub source: String,
    pub installed: i64,
    pub last_check: Option<i64>,
    pub update_status: Option<String>,
    pub update_detail: Option<String>,
}

/// All cache rows, by id. Read-only snapshot for the GUI poll; reconcile
/// stays the writer.
pub async fn mod_cache_rows(pool: &SqlitePool) -> Result<Vec<ModCacheRow>> {
    let rows: Vec<(String, String, String, String, i64, Option<i64>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, kind, asset_sha256, source, installed, last_check, update_status, update_detail FROM mods_cache ORDER BY id",
        )
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                kind,
                asset_sha256,
                source,
                installed,
                last_check,
                update_status,
                update_detail,
            )| {
                ModCacheRow {
                    id,
                    kind,
                    asset_sha256,
                    source,
                    installed,
                    last_check,
                    update_status,
                    update_detail,
                }
            },
        )
        .collect())
}

pub async fn reconcile_mod_cache(
    pool: &SqlitePool,
    data_dir: &Path,
    config_dir: &Path,
) -> Result<()> {
    let listed = list_mods(config_dir, data_dir)?;
    let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    if let Ok(games) = crate::game::list_games(pool, None, None, None).await {
        for g in &games {
            for m in crate::game_manifests(data_dir, &g.id).unwrap_or_default() {
                *counts.entry(m.instance.clone()).or_default() += 1;
            }
        }
    }
    let live: std::collections::BTreeSet<String> =
        listed.mods.iter().map(|m| m.id.clone()).collect();
    for m in &listed.mods {
        let kind = crate::instance::mod_kind(m.official, m.registry.as_deref()).to_string();
        let payload =
            crate::instance::payload_dir(data_dir, m.official, m.registry.as_deref(), &m.id);
        let (sha, source) = crate::instance::read_payload_provenance(&payload)
            .map(|p| (p.asset_sha256, p.source))
            .unwrap_or_default();
        let installed = counts.get(&m.id).copied().unwrap_or(0);
        upsert_mod(pool, &m.id, &kind, &sha, &source, installed).await?;
    }
    if live.is_empty() {
        sqlx::query("DELETE FROM mods_cache").execute(pool).await?;
    } else {
        let placeholders = live.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("DELETE FROM mods_cache WHERE id NOT IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for id in &live {
            q = q.bind(id);
        }
        q.execute(pool).await?;
    }
    Ok(())
}

/// Insert or refresh one cache row (id + kind + provenance + use count).
async fn upsert_mod(
    pool: &SqlitePool,
    id: &str,
    kind: &str,
    sha: &str,
    source: &str,
    installed: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO mods_cache (id, kind, asset_sha256, source, installed)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           kind = excluded.kind,
           asset_sha256 = excluded.asset_sha256,
           source = excluded.source,
           installed = excluded.installed",
    )
    .bind(id)
    .bind(kind)
    .bind(sha)
    .bind(source)
    .bind(installed)
    .execute(pool)
    .await?;
    Ok(())
}

/// One network hit per Mod at most (R32 rules): literal github assets and
/// `manual_url` resolve to a direct URL with no API call; glob/prerelease
/// sources take exactly one release-API resolve; `local` sources re-hash
/// with no network. Records `last_check` + status on the row and returns
/// the catalog status. Empty payload (never downloaded) is `Unknown`,
/// never `Available`.
pub(crate) fn catalog_row_parts(status: &CatalogStatus) -> (String, Option<String>) {
    match status {
        CatalogStatus::UpToDate => ("uptodate".into(), None),
        CatalogStatus::Available { detail } => ("available".into(), Some(detail.clone())),
        CatalogStatus::Unknown { reason } => ("unknown".into(), Some(reason.clone())),
    }
}

pub(crate) async fn record_catalog_status(
    pool: &SqlitePool,
    mod_id: &str,
    now: i64,
    status: &CatalogStatus,
) {
    let (st, detail) = catalog_row_parts(status);
    let _ = sqlx::query(
        "UPDATE mods_cache SET last_check = ?, update_status = ?, update_detail = ? WHERE id = ?",
    )
    .bind(now)
    .bind(st)
    .bind(detail)
    .bind(mod_id)
    .execute(pool)
    .await;
}
