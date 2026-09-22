use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use crate::{set_custom, set_knob};
use std::path::PathBuf;

#[tokio::test]
async fn heroic_dispatches_to_client() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let mut paths = stub_paths(&dir);
    let heroic = dir.join("heroic");
    std::fs::write(&heroic, b"#!/bin/sh\n").unwrap();
    paths.heroic = Some(heroic.clone());
    for (id, store, game_id, url) in [
        (
            "heroic:gog:42",
            "gog",
            "42",
            "heroic://launch?appName=42&runner=gog",
        ),
        (
            "heroic:standalone:abc",
            "standalone",
            "abc",
            "heroic://launch?appName=abc&runner=sideload",
        ),
        ("heroic:other:7", "other", "7", "heroic://launch?appName=7"),
    ] {
        seed_game(
            &pool,
            SeedGame {
                id,
                manager: "heroic",
                store,
                game_id,
                name: Some("h"),
                exe_path: Some(dir.join("game.exe").to_str().unwrap()),
                ..Default::default()
            },
        )
        .await;
        let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
            .await
            .unwrap();
        assert_eq!(spec.program, heroic);
        assert_eq!(&spec.args[..], [url.to_string()]);
        assert!(spec.wrappers.is_empty());
        assert!(spec.env.is_empty());
        assert!(!spec.owned);
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn heroic_missing_client() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "heroic:gog:1",
            manager: "heroic",
            store: "gog",
            game_id: "1",
            name: Some("h"),
            ..Default::default()
        },
    )
    .await;
    let err = build_launch_spec(&pool, &host, "heroic:gog:1", &paths, &dir)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::MissingHeroic));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn proton_waitforexit_when_no_umu() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton-GE");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:proton1",
            manager: "manual",
            store: "standalone",
            game_id: "proton1",
            name: Some("s"),
            exe_path: Some(exe.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:proton1", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.program, script);
    assert_eq!(
        &spec.args[..],
        [
            "waitforexitandrun".to_string(),
            exe.to_string_lossy().into_owned()
        ]
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn unknown_and_missing_exe() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let err = build_launch_spec(&pool, &host, "manual:standalone:missing1", &paths, &dir)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::UnknownGame(_)));
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:cccccccc",
            manager: "manual",
            store: "standalone",
            game_id: "cccccccc",
            name: Some("t"),
            ..Default::default()
        },
    )
    .await;
    let err = build_launch_spec(&pool, &host, "manual:standalone:cccccccc", &paths, &dir)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::MissingExe(_)));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn override_exe_wins() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:dddddddd",
            manager: "manual",
            store: "standalone",
            game_id: "dddddddd",
            name: Some("t"),
            exe_path: Some(a.to_str().unwrap()),
            override_exe_path: Some(b.to_str().unwrap()),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:dddddddd", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.program, b);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn knob_and_custom_merge_order() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = env_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeee0001",
            manager: "manual",
            store: "standalone",
            game_id: "eeee0001",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            env: Some("{\"A\":\"1\",\"B\":\"store\"}"),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    set_knob(&pool, "manual:standalone:eeee0001", "mangohud", "1")
        .await
        .unwrap();
    set_custom(&pool, "manual:standalone:eeee0001", "B", "custom")
        .await
        .unwrap();
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeee0001", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.env.get("A").map(String::as_str), Some("1"));
    assert_eq!(spec.env.get("B").map(String::as_str), Some("custom"));
    assert_eq!(spec.env.get("MANGOHUD").map(String::as_str), Some("1"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
