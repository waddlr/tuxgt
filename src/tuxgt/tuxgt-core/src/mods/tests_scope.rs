use super::testing::nanos;
use super::*;
use crate::testing::{seed_game, SeedGame};
use std::collections::BTreeSet;
use std::fs;

fn scope_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-scope-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest(
    game: &str,
    instance: &str,
    adapter: &str,
    enabled: bool,
    files: Vec<crate::PlannedFile>,
) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade_addon".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: files.into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

fn planned(dest: &str, sha: &str) -> crate::PlannedFile {
    crate::PlannedFile {
        source: format!("mods/user/pack/{dest}"),
        dest: dest.into(),
        sha256: sha.into(),
        enabled: true,
        load: None,
    }
}

fn recipe(id: &str, requires: &str) -> String {
    format!("id = \"{id}\"\ntype = \"custom\"\nlabel = \"{id}\"\n{requires}[source]\ntype = \"local\"\npath = \"/tmp/x\"\n")
}

#[tokio::test]
async fn scope_lists_installed_games_with_flags() {
    let dir = scope_dir("list");
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pool = crate::open_db(&data).await.unwrap();
    let ga = "manual:standalone:scope-a";
    let gb = "manual:standalone:scope-b";
    crate::write_manifest(&data, &manifest(ga, "pack", "preload", true, vec![])).unwrap();
    crate::write_manifest(&data, &manifest(gb, "pack", "preload", false, vec![])).unwrap();
    // Unsorted input: hits come back sorted by display name.
    let games = vec![
        ("manual:standalone:other".to_string(), "Other".to_string()),
        (gb.to_string(), "Game B".to_string()),
        (ga.to_string(), "Game A".to_string()),
    ];
    let hits = install_scope(&pool, &cfg, &data, &games, "pack")
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].game, ga);
    assert_eq!(hits[0].name, "Game A");
    assert!(hits[0].enabled);
    assert!(hits[0].foreign.is_empty());
    assert!(hits[0].root_error.is_none());
    assert_eq!(hits[1].game, gb);
    assert!(!hits[1].enabled);
}

#[tokio::test]
async fn scope_reports_foreign_and_required_by() {
    let dir = scope_dir("foreign");
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:scope-c";
    let root = dir.join("root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("dxgi.dll"), b"foreign-bytes").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let staged = "0".repeat(64);
    crate::write_manifest(
        &data,
        &manifest(
            gid,
            "pack",
            "install",
            true,
            vec![planned("dxgi.dll", &staged)],
        ),
    )
    .unwrap();
    crate::write_manifest(&data, &manifest(gid, "addon-x", "preload", true, vec![])).unwrap();
    let udir = crate::instance::user_mods_dir(&data);
    fs::create_dir_all(&udir).unwrap();
    fs::write(udir.join("pack.toml"), recipe("pack", "")).unwrap();
    fs::write(
        udir.join("addon-x.toml"),
        recipe("addon-x", "requires = [\"pack\"]\n"),
    )
    .unwrap();
    let games = vec![(gid.to_string(), "Game C".to_string())];
    let hits = install_scope(&pool, &cfg, &data, &games, "pack")
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].foreign, vec!["dxgi.dll".to_string()]);
    assert!(hits[0].root_error.is_none());
    assert_eq!(hits[0].required_by, vec!["addon-x".to_string()]);
}

#[tokio::test]
async fn scope_marks_missing_game_root() {
    let dir = scope_dir("noroot");
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pool = crate::open_db(&data).await.unwrap();
    // Install manifest for a game with no DB row: root unresolvable.
    let gid = "manual:standalone:gone";
    crate::write_manifest(
        &data,
        &manifest(
            gid,
            "pack",
            "install",
            true,
            vec![planned("dxgi.dll", "abc")],
        ),
    )
    .unwrap();
    let games = vec![(gid.to_string(), "Gone".to_string())];
    let hits = install_scope(&pool, &cfg, &data, &games, "pack")
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].foreign.is_empty());
    assert!(hits[0].root_error.is_some());
}

#[test]
fn restore_plan_skips_changed_pairs() {
    let dir = scope_dir("restore");
    let data = dir.join("data");
    let ga = "manual:standalone:r-a";
    let gb = "manual:standalone:r-b";
    crate::write_manifest(&data, &manifest(ga, "pack", "preload", false, vec![])).unwrap();
    crate::write_manifest(&data, &manifest(gb, "pack", "preload", true, vec![])).unwrap();
    let recorded = BTreeSet::from([
        ga.to_string(),
        gb.to_string(),
        "manual:standalone:r-c".to_string(),
    ]);
    let plan = plan_bulk_restore(&data, "pack", &recorded);
    assert_eq!(plan.restore, vec![ga.to_string()]);
    assert_eq!(
        plan.skipped,
        vec![
            (gb.to_string(), SkipReason::Enabled),
            ("manual:standalone:r-c".to_string(), SkipReason::Uninstalled),
        ]
    );
}

#[tokio::test]
async fn uninstall_in_games_removes_preload_manifests() {
    let dir = scope_dir("uninst");
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let ga = "manual:standalone:u-a";
    let gb = "manual:standalone:u-b";
    for gid in [ga, gb] {
        seed_game(
            &pool,
            SeedGame {
                id: gid,
                detected_api: Some("dx12"),
                detected_bitness: Some("64"),
                ..Default::default()
            },
        )
        .await;
        crate::write_manifest(&data, &manifest(gid, "pack", "preload", true, vec![])).unwrap();
    }
    let games = vec![
        (ga.to_string(), "A".to_string()),
        (gb.to_string(), "B".to_string()),
    ];
    let out = uninstall_in_games(&pool, &data, &games, "pack").await;
    assert_eq!(out.len(), 2);
    for r in &out {
        assert!(r.result.is_ok(), "{}: {:?}", r.game, r.result);
    }
    assert!(crate::game_manifests(&data, ga).unwrap().is_empty());
    assert!(crate::game_manifests(&data, gb).unwrap().is_empty());
}

#[tokio::test]
async fn fanout_runners_continue_after_failure() {
    let dir = scope_dir("partial");
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let good = "manual:standalone:p-good";
    let missing = "manual:standalone:p-missing";
    seed_game(
        &pool,
        SeedGame {
            id: good,
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(&data, &manifest(good, "pack", "preload", true, vec![])).unwrap();
    // The manifest-less game errors; the run still reaches the good game.
    let games = vec![
        (missing.to_string(), "Missing".to_string()),
        (good.to_string(), "Good".to_string()),
    ];
    let out = set_enabled_in_games(&pool, &data, &games, "pack", false, false).await;
    assert_eq!(out.len(), 2);
    assert!(out[0].result.is_err());
    assert!(out[1].result.is_ok(), "{:?}", out[1].result);
    let m = &crate::game_manifests(&data, good).unwrap()[0];
    assert!(!m.enabled);
    // Same contract for uninstall: failure recorded, sibling removed.
    let out = uninstall_in_games(&pool, &data, &games, "pack").await;
    assert_eq!(out.len(), 2);
    assert!(out[0].result.is_err());
    assert!(out[1].result.is_ok(), "{:?}", out[1].result);
    assert!(crate::game_manifests(&data, good).unwrap().is_empty());
}

#[tokio::test]
async fn set_enabled_in_games_flips_preload_flags() {
    let dir = scope_dir("toggle");
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:t-a";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(&data, &manifest(gid, "pack", "preload", true, vec![])).unwrap();
    let games = vec![(gid.to_string(), "A".to_string())];
    let out = set_enabled_in_games(&pool, &data, &games, "pack", false, false).await;
    assert!(out[0].result.is_ok(), "{:?}", out[0].result);
    let m = &crate::game_manifests(&data, gid).unwrap()[0];
    assert!(!m.enabled);
    let out = set_enabled_in_games(&pool, &data, &games, "pack", true, true).await;
    assert!(out[0].result.is_ok(), "{:?}", out[0].result);
    let m = &crate::game_manifests(&data, gid).unwrap()[0];
    assert!(m.enabled);
}
