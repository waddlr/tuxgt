use crate::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{ConnectOptions, SqlitePool};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

pub async fn open_db(data_dir: &Path) -> Result<SqlitePool> {
    let cfg = data_dir.join("config");
    tokio::fs::create_dir_all(&cfg).await?;
    let db_path = cfg.join("tuxgt.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        // Statement bodies stay out of the logs: sqlx 0.8 logs every
        // statement at DEBUG by default, which buries debug runs (one GUI
        // touch fans out to dozens of statements). Slow queries still warn.
        .log_statements(log::LevelFilter::Off)
        .log_slow_statements(log::LevelFilter::Warn, std::time::Duration::from_secs(1))
        .to_owned();
    let pool = SqlitePoolOptions::new().connect_with(opts).await?;
    crate::game::migrate_games(&pool, data_dir).await?;
    crate::env::migrate_env(&pool).await?;
    crate::session::migrate_handle(&pool).await?;
    crate::session::migrate_extra_exes(&pool).await?;
    crate::metadata::migrate_metadata(&pool).await?;
    crate::mods::migrate_mod_cache(&pool).await?;
    reconcile_at_open(&pool, data_dir).await?;
    // The dirty flag stays: a concurrent shared-pool user may own it, and a
    // redundant rebuild on the next hit is cheaper than a lost one.
    note_reconciled();
    Ok(pool)
}

/// E103 (`docs/dev/app/plugin/instances.md`): the open reconciles the mod cache
/// and is its only writer. A fresh open always rebuilds; a shared hit rebuilds
/// when marked, so `poll` (which opens first) reads fresh rows after any
/// marked mutation.
async fn reconcile_at_open(pool: &SqlitePool, data_dir: &Path) -> Result<()> {
    crate::mods::reconcile_mod_cache(pool, data_dir, &data_dir.join("config")).await
}

/// Serializes the shared-open tests: both touch the process-global dirty
/// flag, so they must not interleave with each other.
#[cfg(test)]
static TEST_SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Clear any dirty mark other tests set: lets one test assert a clean hit
/// skips without depending on suite-wide mutator traffic. Tests only.
#[cfg(test)]
pub(crate) fn drain_dirty_for_test() {
    CACHE_DIRTY.store(false, Ordering::SeqCst);
}

/// Set by writers of what [`reconcile_at_open`] reads (recipes, payloads,
/// manifests, game rows). A marked shared-pool hit rebuilds; a clean hit
/// skips unless the 60s backstop elapsed. SeqCst pairs the store with the
/// file writes that precede it.
static CACHE_DIRTY: AtomicBool = AtomicBool::new(false);
/// When a reconcile last ran (monotonic clock). 60s backstop so hand-placed
/// files still surface in a long-lived GUI without a mutator in between.
static LAST_RECONCILE: LazyLock<Mutex<std::time::Instant>> =
    LazyLock::new(|| Mutex::new(std::time::Instant::now()));
const RECONCILE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(60);

/// Mark the mod cache stale; the next shared-pool hit that observes the flag
/// rebuilds. Call on success from writers of reconcile inputs.
pub(crate) fn mark_cache_dirty() {
    CACHE_DIRTY.store(true, Ordering::SeqCst);
}

fn note_reconciled() {
    *LAST_RECONCILE.lock().expect("reconcile clock") = std::time::Instant::now();
}

/// One pool per data dir for long-lived callers (the GUI), so a page load
/// stops respawning the sqlx worker thread and re-running the six migrations.
/// A fresh open still reconciles ([`reconcile_at_open`], E103); a cached hit
/// rebuilds only when [`mark_cache_dirty`] ran since (or the 60s backstop
/// elapsed), so mutators keep the "stale until the next open" contract. The
/// CLI and other tests stay on [`open_db`]: they outlive no data dir, and
/// each of their opens must see the file as it is now.
static SHARED: LazyLock<Mutex<Option<(PathBuf, SqlitePool)>>> = LazyLock::new(|| Mutex::new(None));

pub async fn open_db_shared(data_dir: &Path) -> Result<SqlitePool> {
    let cached = SHARED
        .lock()
        .expect("db pool slot")
        .as_ref()
        .filter(|(dir, _)| dir.as_path() == data_dir)
        .map(|(_, pool)| pool.clone());
    match cached {
        Some(pool) => {
            // Reconcile only when a mutator dirtied the inputs (or the
            // backstop elapsed for hand-placed files): a clean hit is just
            // the pooled handle. Keeps the E103 stale-until-next-open
            // contract without rebuilding 47 rows per DB touch.
            let dirty = CACHE_DIRTY.swap(false, Ordering::SeqCst);
            let stale = LAST_RECONCILE
                .lock()
                .expect("reconcile clock")
                .elapsed()
                >= RECONCILE_MAX_AGE;
            if dirty || stale {
                reconcile_at_open(&pool, data_dir).await?;
                note_reconciled();
            }
            Ok(pool)
        }
        None => {
            let fresh = open_db(data_dir).await?;
            let mut slot = SHARED.lock().expect("db pool slot");
            match slot.as_ref().filter(|(dir, _)| dir.as_path() == data_dir) {
                // Lost a first-open race: keep the winner's pool (its own
                // `open_db` reconciled), not a second one.
                Some((_, existing)) => Ok(existing.clone()),
                None => {
                    *slot = Some((data_dir.to_path_buf(), fresh.clone()));
                    Ok(fresh)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::mod_cache_rows;

    /// E103 through the shared open: a mutator only leaves files on disk, so
    /// the *open* reconciles — also when it hands back the cached pool.
    #[tokio::test]
    async fn shared_open_reconciles_when_the_pool_is_reused() {
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "tuxgt-shared-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data = root.join("data");
        let cfg = root.join("config");
        std::fs::create_dir_all(&cfg).unwrap();
        let pkg = root.join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("plug.dll"), b"plug").unwrap();

        let first = open_db_shared(&data).await.unwrap();
        crate::add_mod_from(
            &cfg,
            "custom",
            "sharedplug",
            &pkg,
            None,
            None,
            &data,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let second = open_db_shared(&data).await.unwrap();
        let ids: Vec<String> = mod_cache_rows(&second)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert!(ids.iter().any(|id| id == "sharedplug"), "{ids:?}");
        // One cached pool backs both handles: closing either closes the inner
        // pool, so a second open that built a fresh pool would leave this open.
        first.close().await;
        assert!(second.is_closed(), "second open must reuse the cached pool");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Dirty-flag arms via cache content: a marked hit rebuilds, a clean hit
    /// preserves planted staleness. Content is per data dir, so other tests'
    /// rebuilds cannot move it; the flag is drained right before the clean
    /// hit so suite-wide mutator traffic cannot mark it.
    #[tokio::test]
    async fn shared_open_skips_rebuild_when_clean() {
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "tuxgt-shared-skip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data = root.join("data");
        let cfg = root.join("config");
        std::fs::create_dir_all(&cfg).unwrap();
        let pkg = root.join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(pkg.join("plug.dll"), b"plug").unwrap();
        let pool = open_db_shared(&data).await.unwrap();
        crate::add_mod_from(
            &cfg,
            "custom",
            "skipplug",
            &pkg,
            None,
            None,
            &data,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        // The add marked: this hit rebuilds and the row appears.
        open_db_shared(&data).await.unwrap();
        assert!(cache_ids(&pool).await.iter().any(|id| id == "skipplug"));
        // Plant staleness behind the cache's back and prove a clean hit
        // preserves it. Drained right before each open; retried because a
        // parallel suite test can mark in the drain-to-swap microsecond
        // window. A repeat landing across attempts is vanishingly unlikely;
        // rebuilding on every attempt means the skip is really broken.
        for _ in 0..3 {
            sqlx::query("DELETE FROM mods_cache WHERE id = ?")
                .bind("skipplug")
                .execute(&pool)
                .await
                .unwrap();
            drain_dirty_for_test();
            open_db_shared(&data).await.unwrap();
            if !cache_ids(&pool).await.iter().any(|id| id == "skipplug") {
                break;
            }
        }
        assert!(
            !cache_ids(&pool).await.iter().any(|id| id == "skipplug"),
            "clean hit must skip the rebuild"
        );
        // A fresh mark rebuilds and the row comes back.
        mark_cache_dirty();
        open_db_shared(&data).await.unwrap();
        assert!(cache_ids(&pool).await.iter().any(|id| id == "skipplug"));
        pool.close().await;
        let _ = std::fs::remove_dir_all(&root);
    }

    async fn cache_ids(pool: &sqlx::SqlitePool) -> Vec<String> {
        mod_cache_rows(pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect()
    }
}
