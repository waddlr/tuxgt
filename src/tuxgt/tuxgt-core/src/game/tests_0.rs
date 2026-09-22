use super::testing::*;
use super::*;
use crate::provider::{GameProvider, GameRecord};
use crate::testing::{seed_game, SeedGame};
use crate::{PluginHost, Result};
use std::collections::HashSet;

#[test]
fn art_url_prefers_cover_then_header() {
    assert_eq!(
        art_row(Some("https://x/c.jpg"), Some("https://x/h.jpg")).art_url(),
        Some("https://x/c.jpg")
    );
    assert_eq!(
        art_row(None, Some("https://x/h.jpg")).art_url(),
        Some("https://x/h.jpg")
    );
    assert!(art_row(Some("/local/c.jpg"), None).art_url().is_none());
    assert!(art_row(None, None).art_url().is_none());
}

/// E65: metadata appid — stored overlay wins over the Steam game segment;
/// a non-Steam row without an overlay has none.
#[test]
fn resolved_appid_prefers_stored_overlay() {
    let row = |id: &str, manager: &str, overlay: Option<&str>| {
        let mut g = art_row(None, None);
        g.id = id.into();
        g.manager = manager.into();
        g.steam_appid = overlay.map(str::to_string);
        g
    };
    assert_eq!(
        row("heroic:gog:1", "heroic", Some("814380")).resolved_appid(),
        Some("814380")
    );
    assert_eq!(
        row("steam::976730", "steam", Some("814380")).resolved_appid(),
        Some("814380")
    );
    assert_eq!(
        row("steam::976730", "steam", None).resolved_appid(),
        Some("976730")
    );
    assert_eq!(
        row("manual:standalone:abc", "manual", None).resolved_appid(),
        None
    );
    // An empty stored value counts as unset, like `steam_appid_of`.
    assert_eq!(
        row("steam::976730", "steam", Some("")).resolved_appid(),
        Some("976730")
    );
    assert_eq!(
        row("heroic:gog:1", "heroic", Some("")).resolved_appid(),
        None
    );
}

#[test]
fn game_id_roundtrip() {
    let id = GameId::parse("steam::814380").unwrap();
    assert_eq!(id.manager, "steam");
    assert_eq!(id.store, "");
    assert_eq!(id.game, "814380");
    assert_eq!(id.to_string(), "steam::814380");

    let sc = GameId::parse("steam:standalone:2786274309").unwrap();
    assert_eq!(sc.store, "standalone");
    assert_eq!(sc.to_string(), "steam:standalone:2786274309");

    let h = GameId::parse("heroic:epic:Fortnite").unwrap();
    assert_eq!(h.to_string(), "heroic:epic:Fortnite");

    assert!(GameId::parse("steam").is_err());
    assert!(GameId::parse("steam:814380").is_err());
    assert!(GameId::parse("::x").is_err());
}

#[tokio::test]
async fn standalone_migration_renames_rows_and_files() {
    let dir = std::env::temp_dir().join(format!("tuxgt-b02-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    for (id, manager, store, game) in [
        ("steam:shortcut:1", "steam", "shortcut", "1"),
        ("heroic:sideload:2", "heroic", "sideload", "2"),
        ("manual::abcdef12", "manual", "", "abcdef12"),
        ("steam::3", "steam", "", "3"),
        ("heroic:gog:4", "heroic", "gog", "4"),
    ] {
        seed_game(
            &pool,
            SeedGame {
                id,
                manager,
                store,
                game_id: game,
                name: Some("t"),
                ..Default::default()
            },
        )
        .await;
    }
    sqlx::query(
            "INSERT INTO metadata_cache (game_id, source, data, fetched_at) VALUES ('heroic:sideload:2', 'steamgriddb', '{}', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
    crate::download::write_manifest(
        &dir,
        &crate::download::FileManifest {
            game: "steam:shortcut:1".into(),
            instance: "m".into(),
            mod_type: "reshade".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
            env: Box::default(),
        },
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("games/heroic_sideload/2/stage")).unwrap();
    std::fs::write(
        dir.join("games/heroic_sideload/2/stage/m.staging.toml"),
        "game = 'heroic:sideload:2'\ninstance = 'm'\n",
    )
    .unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: "manual::abcdef12".into(),
            manager: "manual".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("games/steam_shortcut/1/backups")).unwrap();
    crate::env::set_knob(&pool, "steam:shortcut:1", "mangohud", "1")
        .await
        .unwrap();

    migrate_games(&pool, &dir).await.unwrap();

    let ids: Vec<(String, String)> = sqlx::query_as("SELECT id, store FROM games ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    for (want_id, want_store) in [
        ("steam:standalone:1", "standalone"),
        ("heroic:standalone:2", "standalone"),
        ("manual:standalone:abcdef12", "standalone"),
        ("steam::3", ""),
        ("heroic:gog:4", "gog"),
    ] {
        assert!(
            ids.contains(&(want_id.to_string(), want_store.to_string())),
            "{ids:?}"
        );
    }
    let meta: Vec<(String,)> = sqlx::query_as("SELECT game_id FROM metadata_cache")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(meta, vec![("heroic:standalone:2".to_string(),)]);
    assert!(dir
        .join("games/steam_standalone/1/manifests/m.toml")
        .is_file());
    assert!(dir.join("games/heroic_standalone/2/stage").is_dir());
    assert!(dir
        .join("games/manual_standalone/abcdef12/apply.toml")
        .is_file());
    assert!(dir.join("games/steam_standalone/1/backups").is_dir());
    let ms = crate::download::game_manifests(&dir, "steam:standalone:1").unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].game, "steam:standalone:1");
    let rec = crate::apply::read_record(&dir, "manual:standalone:abcdef12")
        .unwrap()
        .unwrap();
    assert_eq!(rec.game, "manual:standalone:abcdef12");
    let staged =
        std::fs::read_to_string(dir.join("games/heroic_standalone/2/stage/m.staging.toml"))
            .unwrap();
    assert!(
        staged.contains("game = \"heroic:standalone:2\""),
        "{staged}"
    );
    assert_eq!(
        crate::env::knob_values(&pool, "steam:standalone:1")
            .await
            .unwrap(),
        vec![("mangohud".to_string(), "1".to_string())]
    );
    migrate_games(&pool, &dir).await.unwrap();
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM games")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n.0, 5);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn id8_stable_and_collision() {
    let empty = HashSet::new();
    let a = id8("same-key", &empty);
    let b = id8("same-key", &empty);
    assert_eq!(a, b);
    assert_eq!(a.len(), 8);
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));

    let mut taken = HashSet::new();
    taken.insert(format!("manual:standalone:{a}"));
    let c = id8_prefixed("manual:standalone:", "same-key", &taken);
    assert_ne!(c, a);
    assert_eq!(c.len(), 8);
}

struct FakeSteam(Vec<GameRecord>);

impl GameProvider for FakeSteam {
    fn plugin_id(&self) -> &'static str {
        "steam"
    }
    fn scan(&self) -> Result<Vec<GameRecord>> {
        Ok(self.0.clone())
    }
}

fn rec(manager: &str, store: &str, game: &str, name: &str) -> GameRecord {
    GameRecord::new(GameId::new(manager, store, game).unwrap(), name)
}

#[tokio::test]
async fn prune_steam_keeps_manual() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e03-prune-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();

    let fake = FakeSteam(vec![
        rec("steam", "", "1", "One"),
        rec("steam", "", "2", "Two"),
    ]);
    scan_with(
        &pool,
        &host,
        &[&fake, &crate::provider::manual::ManualProvider],
    )
    .await
    .unwrap();

    let exe_dir = dir.join("exe");
    tokio::fs::create_dir_all(&exe_dir).await.unwrap();
    let exe = exe_dir.join("m.bin");
    tokio::fs::write(&exe, b"x").await.unwrap();
    add_manual(&pool, &host, &exe).await.unwrap();

    let fake = FakeSteam(vec![rec("steam", "", "1", "One")]);
    let listed = scan_with(
        &pool,
        &host,
        &[&fake, &crate::provider::manual::ManualProvider],
    )
    .await
    .unwrap();
    let ids: Vec<&str> = listed.iter().map(|g| g.id.as_str()).collect();
    assert!(ids.contains(&"steam::1"));
    assert!(!ids.iter().any(|i| *i == "steam::2"));
    assert!(ids.iter().any(|i| i.starts_with("manual:standalone:")));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn fts_query_by_name() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e03-fts-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();
    let fake = FakeSteam(vec![
        rec("steam", "", "1", "Sekiro Shadows"),
        rec("steam", "", "2", "Celeste"),
    ]);
    scan_with(&pool, &host, &[&fake]).await.unwrap();
    let hit = list_games(&pool, None, None, Some("Sekiro")).await.unwrap();
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].id, "steam::1");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn manual_readd_same_path_keeps_id() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e03-manual-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();
    let exe_dir = dir.join("exe");
    tokio::fs::create_dir_all(&exe_dir).await.unwrap();
    let a = exe_dir.join("game.bin");
    let b = exe_dir.join("game.exe");
    tokio::fs::write(&a, b"x").await.unwrap();
    tokio::fs::write(&b, b"y").await.unwrap();
    let r1 = add_manual(&pool, &host, &a).await.unwrap();
    let r2 = add_manual(&pool, &host, &a).await.unwrap();
    let r3 = add_manual(&pool, &host, &b).await.unwrap();
    assert_eq!(r1.id, r2.id);
    assert_ne!(r1.id, r3.id);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn hidden_override_beats_detected_and_survives_rescan() {
    let dir = std::env::temp_dir().join(format!("tuxgt-hidden-override-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let cfg = dir.join("cfg");
    let host = PluginHost::load_with(crate::FIRST_PARTY, &cfg).unwrap();

    let mut one = rec("steam", "", "1", "Hidden One");
    one.hidden = true;
    let three = rec("steam", "", "3", "Visible Three");
    let fake = FakeSteam(vec![one, rec("steam", "", "2", "Visible Two"), three]);
    scan_with(&pool, &host, &[&fake]).await.unwrap();

    let listed = list_games(&pool, None, None, None).await.unwrap();
    assert_eq!(listed.len(), 3, "list_games keeps hidden rows");
    let hidden_of = |id: &str| listed.iter().find(|g| g.id == id).unwrap().hidden;
    assert!(hidden_of("steam::1"), "detected hidden shows through");
    assert!(!hidden_of("steam::2"));

    set_hidden_override(&pool, "steam::1", Some(false))
        .await
        .unwrap();
    set_hidden_override(&pool, "steam::2", Some(true))
        .await
        .unwrap();
    let listed = list_games(&pool, None, None, None).await.unwrap();
    let hidden_of = |id: &str| listed.iter().find(|g| g.id == id).unwrap().hidden;
    assert!(
        !hidden_of("steam::1"),
        "override Some(false) beats detected true"
    );
    assert!(
        hidden_of("steam::2"),
        "override Some(true) beats detected false"
    );

    // Rescan: 1 and 2 keep their detected values, so a clobbered override
    // would flip them back; 3 flips to prove detection refreshed.
    let mut one = rec("steam", "", "1", "Hidden One");
    one.hidden = true;
    let mut three = rec("steam", "", "3", "Visible Three");
    three.hidden = true;
    let rescan = FakeSteam(vec![one, rec("steam", "", "2", "Visible Two"), three]);
    scan_with(&pool, &host, &[&rescan]).await.unwrap();
    let listed = list_games(&pool, None, None, None).await.unwrap();
    let hidden_of = |id: &str| listed.iter().find(|g| g.id == id).unwrap().hidden;
    assert!(!hidden_of("steam::1"), "override survives rescan");
    assert!(hidden_of("steam::2"), "override survives rescan");
    assert!(hidden_of("steam::3"), "detected_hidden refreshes on rescan");

    set_hidden_override(&pool, "steam::1", None).await.unwrap();
    set_hidden_override(&pool, "steam::2", None).await.unwrap();
    let listed = list_games(&pool, None, None, None).await.unwrap();
    let hidden_of = |id: &str| listed.iter().find(|g| g.id == id).unwrap().hidden;
    assert!(hidden_of("steam::1"), "None clears back to detected true");
    assert!(!hidden_of("steam::2"), "None clears back to detected false");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn launch_env_lines_sorted_key_value() {
    let cfg = GameLaunchConfig {
        launch_options: None,
        env: Some(r#"{"B":"2","A":"1"}"#.into()),
        wrapper: None,
    };
    assert_eq!(cfg.env_lines(), vec!["A=1".to_string(), "B=2".to_string()]);
    let empty = GameLaunchConfig::default();
    assert!(empty.env_lines().is_empty());
    let raw = GameLaunchConfig {
        env: Some("FOO=1 BAR=2".into()),
        ..Default::default()
    };
    assert_eq!(raw.env_lines(), vec!["FOO=1 BAR=2".to_string()]);
}
