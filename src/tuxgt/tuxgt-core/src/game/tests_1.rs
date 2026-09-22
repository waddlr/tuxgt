use super::testing::*;
use super::*;
use crate::provider::{GameProvider, GameRecord};
use crate::testing::{seed_game, SeedGame};
use crate::{Error, PluginHost, Result};

#[tokio::test]
async fn steam_appid_overlay_survives_rescan_and_validates() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e43-appid-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();
    let fake = FakeSteam(vec![rec("steam", "", "814380", "Satisfactory")]);
    scan_with(&pool, &host, &[&fake]).await.unwrap();

    assert_eq!(steam_appid_of(&pool, "steam::814380").await.unwrap(), None);

    set_steam_appid(&pool, "steam::814380", Some("570"))
        .await
        .unwrap();
    assert_eq!(
        steam_appid_of(&pool, "steam::814380").await.unwrap(),
        Some("570".to_string())
    );

    // The scan upsert must not clobber the overlay.
    let rescan = FakeSteam(vec![rec("steam", "", "814380", "Satisfactory")]);
    scan_with(&pool, &host, &[&rescan]).await.unwrap();
    assert_eq!(
        steam_appid_of(&pool, "steam::814380").await.unwrap(),
        Some("570".to_string()),
        "overlay survives rescan"
    );

    // Any manager can carry the overlay (Heroic/manual are the point).
    seed_game(
        &pool,
        SeedGame {
            id: "heroic:gog:1",
            manager: "heroic",
            store: "gog",
            game_id: "1",
            name: Some("GOG One"),
            ..Default::default()
        },
    )
    .await;
    set_steam_appid(&pool, "heroic:gog:1", Some("814380"))
        .await
        .unwrap();
    assert_eq!(
        steam_appid_of(&pool, "heroic:gog:1").await.unwrap(),
        Some("814380".to_string())
    );

    // Trimmed before storing; empty or None clears.
    set_steam_appid(&pool, "steam::814380", Some("  42  "))
        .await
        .unwrap();
    assert_eq!(
        steam_appid_of(&pool, "steam::814380").await.unwrap(),
        Some("42".to_string())
    );
    set_steam_appid(&pool, "steam::814380", Some("   "))
        .await
        .unwrap();
    assert_eq!(steam_appid_of(&pool, "steam::814380").await.unwrap(), None);
    set_steam_appid(&pool, "steam::814380", Some("7"))
        .await
        .unwrap();
    set_steam_appid(&pool, "steam::814380", None).await.unwrap();
    assert_eq!(steam_appid_of(&pool, "steam::814380").await.unwrap(), None);

    // Non-digits, too long, embedded space, non-ASCII digits: rejected.
    for bad in ["abc", "12a", "12 34", "12345678901", "１２３"] {
        assert!(
            matches!(
                set_steam_appid(&pool, "steam::814380", Some(bad)).await,
                Err(Error::InvalidOverride(_))
            ),
            "{bad:?} must be rejected"
        );
    }
    assert_eq!(steam_appid_of(&pool, "steam::814380").await.unwrap(), None);

    // Unknown id errors on both accessors.
    assert!(matches!(
        set_steam_appid(&pool, "steam::1", Some("570")).await,
        Err(Error::UnknownGame(_))
    ));
    assert!(matches!(
        steam_appid_of(&pool, "steam::1").await,
        Err(Error::UnknownGame(_))
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn list_games_keeps_rows_with_null_or_set_hidden() {
    let dir = std::env::temp_dir().join(format!("tuxgt-hidden-list-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    for (id, detected, over) in [
        ("steam::1", Some(1), None),
        ("steam::2", Some(0), Some(1)),
        ("steam::3", None, None),
    ] {
        seed_game(
            &pool,
            SeedGame {
                id,
                manager: "steam",
                game_id: id.rsplit(':').next().unwrap(),
                name: Some(id),
                detected_hidden: detected,
                override_hidden: over,
                ..Default::default()
            },
        )
        .await;
    }
    let listed = list_games(&pool, None, None, None).await.unwrap();
    assert_eq!(listed.len(), 3, "list_games returns hidden rows too");
    let hidden_of = |id: &str| listed.iter().find(|g| g.id == id).unwrap().hidden;
    assert!(hidden_of("steam::1"));
    assert!(hidden_of("steam::2"));
    assert!(!hidden_of("steam::3"), "NULL detected_hidden means visible");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

struct RecordingSteam {
    recs: Vec<GameRecord>,
    scans: std::sync::atomic::AtomicUsize,
}

impl RecordingSteam {
    fn new(recs: Vec<GameRecord>) -> Self {
        Self {
            recs,
            scans: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn scans(&self) -> usize {
        self.scans.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl GameProvider for RecordingSteam {
    fn plugin_id(&self) -> &'static str {
        "steam"
    }
    fn scan(&self) -> Result<Vec<GameRecord>> {
        self.scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.recs.clone())
    }
}

#[tokio::test]
async fn disabled_provider_scan_skips_and_keeps_rows() {
    let dir = std::env::temp_dir().join(format!("tuxgt-hidden-disabled-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let mut host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();

    let steam = RecordingSteam::new(vec![
        rec("steam", "", "1", "One"),
        rec("steam", "", "2", "Two"),
    ]);
    scan_with(&pool, &host, &[&steam]).await.unwrap();
    assert_eq!(steam.scans(), 1);

    host.set_enabled("steam", false).unwrap();
    let disabled = RecordingSteam::new(vec![rec("steam", "", "1", "One")]);
    let listed = scan_with(&pool, &host, &[&disabled]).await.unwrap();
    assert_eq!(disabled.scans(), 0, "disabled provider must not be scanned");
    let ids: Vec<&str> = listed.iter().map(|g| g.id.as_str()).collect();
    assert!(ids.contains(&"steam::1"), "rows stay in the index: {ids:?}");
    assert!(
        ids.contains(&"steam::2"),
        "disabled provider rows are not pruned: {ids:?}"
    );

    host.set_enabled("steam", true).unwrap();
    let reenabled = RecordingSteam::new(vec![rec("steam", "", "1", "One")]);
    let listed = scan_with(&pool, &host, &[&reenabled]).await.unwrap();
    assert_eq!(reenabled.scans(), 1);
    let ids: Vec<&str> = listed.iter().map(|g| g.id.as_str()).collect();
    assert!(ids.contains(&"steam::1"));
    assert!(
        !ids.contains(&"steam::2"),
        "re-enabled provider prunes: {ids:?}"
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn add_manual_refuses_when_manual_disabled() {
    let dir = std::env::temp_dir().join(format!("tuxgt-hidden-add-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let mut host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();
    let exe_dir = dir.join("exe");
    tokio::fs::create_dir_all(&exe_dir).await.unwrap();
    let exe = exe_dir.join("game.bin");
    tokio::fs::write(&exe, b"x").await.unwrap();

    host.set_enabled("manual", false).unwrap();
    match add_manual(&pool, &host, &exe).await.unwrap_err() {
        Error::PluginDisabled(id) => assert_eq!(id, "manual"),
        other => panic!("expected PluginDisabled, got {other}"),
    }
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM games")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n.0, 0, "no row inserted while manual is disabled");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// R62: parallel first-boot. N tasks sharing one db file race the
/// PRAGMA snapshot against each other's ADD COLUMN; pre-fix all but one
/// fail with `duplicate column name`.
#[tokio::test]
async fn migrate_games_concurrent_add_column_safe() {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::time::Duration;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-r62-concurrent-{}-{nanos}",
        std::process::id()
    ));
    let cfg = dir.join("config");
    tokio::fs::create_dir_all(&cfg).await.unwrap();
    let db_path = cfg.join("tuxgt.sqlite");
    // Minimal pre-migration schema: base table only, no added columns,
    // so every task's PRAGMA snapshot sees all columns as missing.
    let setup_opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .to_owned();
    let setup = SqlitePoolOptions::new()
        .connect_with(setup_opts)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE games (id TEXT PRIMARY KEY NOT NULL, name TEXT)")
        .execute(&setup)
        .await
        .unwrap();
    setup.close().await;

    // One pool per task, like N parallel `open_db` calls on one file.
    // `busy_timeout` lets lock losers wait their turn so the race
    // resolves to the duplicate-column path, not SQLITE_BUSY.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let db = db_path.clone();
        let d = dir.clone();
        handles.push(tokio::spawn(async move {
            let opts = SqliteConnectOptions::new()
                .filename(&db)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(10))
                .to_owned();
            let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();
            let r = migrate_games(&pool, &d).await;
            pool.close().await;
            r
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }

    // Every column landed exactly once; a sequential rerun is a no-op.
    let pool = crate::open_db(&dir).await.unwrap();
    let cols: Vec<(i32, String, String, i32, Option<String>, i32)> =
        sqlx::query_as("PRAGMA table_info(games)")
            .fetch_all(&pool)
            .await
            .unwrap();
    let names: Vec<&str> = cols.iter().map(|r| r.1.as_str()).collect();
    for want in ["manager", "store", "game_id", "steam_appid", "last_played"] {
        assert_eq!(names.iter().filter(|&&n| n == want).count(), 1, "{names:?}");
    }
    migrate_games(&pool, &dir).await.unwrap();
    pool.close().await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn game_index_aligns_with_list_games() {
    let dir = std::env::temp_dir().join(format!("tuxgt-idx-align-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    // Seeded out of order; both queries sort by name NOCASE, id.
    for (id, name, hidden) in [
        ("steam::3", Some("c game"), None),
        ("steam::1", Some("a game"), Some(1)),
        ("steam::2", None, None),
    ] {
        seed_game(
            &pool,
            SeedGame {
                id,
                manager: "steam",
                name,
                override_hidden: hidden,
                ..Default::default()
            },
        )
        .await;
    }
    let full = list_games(&pool, None, None, None).await.unwrap();
    let index = list_game_index(&pool).await.unwrap();
    assert_eq!(full.len(), 3);
    assert_eq!(index.len(), 3);
    // Same positions, same effective fields the GUI keeps always.
    for (g, e) in full.iter().zip(index.iter()) {
        assert_eq!(e.id, g.id);
        assert_eq!(e.name, g.name);
        assert_eq!(e.manager, g.manager);
        assert_eq!(e.hidden, g.hidden);
        assert_eq!(e, &GameIndexRow::from_row(g));
    }
    assert_eq!(index[0].id, "steam::2"); // NULL name sorts first
    assert_eq!(index[1].id, "steam::1");
    assert!(index[1].hidden);
    assert_eq!(index[2].id, "steam::3");
    pool.close().await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn game_row_by_id_roundtrip() {
    let dir = std::env::temp_dir().join(format!("tuxgt-idx-byid-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "steam::814380",
            manager: "steam",
            name: Some("Satisfactory"),
            ..Default::default()
        },
    )
    .await;
    let want = list_games(&pool, None, None, None).await.unwrap();
    assert_eq!(
        game_row_by_id(&pool, "steam::814380").await.unwrap(),
        Some(want.into_iter().next().unwrap())
    );
    assert_eq!(game_row_by_id(&pool, "steam::nope").await.unwrap(), None);
    pool.close().await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
