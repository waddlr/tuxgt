use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use std::path::PathBuf;

#[tokio::test]
async fn overlay_wrapper_already_present_is_not_duplicated() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let id = "manual:standalone:dddddddd";
    let (pool, dir, paths, host) = manual_row(id, &true_bin, Some("mangohud %command%")).await;
    for w in ["mangohud", "gamemode"] {
        crate::set_wrapper(&pool, id, w).await.unwrap();
    }
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(
        spec.argv(),
        vec![
            "mangohud".to_string(),
            "gamemoderun".to_string(),
            paths
                .launcher
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            true_bin.to_string_lossy().into_owned(),
        ]
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn steam_row_ignores_stored_wrappers() {
    let (pool, dir) = pool_dir().await;
    let host = wrapper_host(&dir);
    let mut paths = stub_paths(&dir);
    let steam = dir.join("steam");
    std::fs::write(&steam, b"#!/bin/sh\n").unwrap();
    paths.steam = Some(steam.clone());
    let id = "steam:standalone:4139290751";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            store: "standalone",
            game_id: "4139290751",
            name: Some("t"),
            ..Default::default()
        },
    )
    .await;
    crate::set_wrapper(&pool, id, "gamemode").await.unwrap();
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.program, steam);
    assert!(spec.wrappers.is_empty());
    assert!(!spec.owned);
    // dispatch does not consume or clear the stored selection
    assert_eq!(crate::game_wrappers(&pool, id).await.unwrap(), ["gamemode"]);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn disabled_wrapper_plugin_skips_stored_wrappers() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let id = "manual:standalone:eeeeeeee";
    let (pool, dir, paths, mut host) = manual_row(id, &true_bin, None).await;
    crate::set_wrapper(&pool, id, "gamemode").await.unwrap();
    host.set_enabled("wrapper", false).unwrap();
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(
        spec.argv(),
        vec![
            paths
                .launcher
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            true_bin.to_string_lossy().into_owned(),
        ]
    );
    // rows stay (E12 rule)
    assert_eq!(crate::game_wrappers(&pool, id).await.unwrap(), ["gamemode"]);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn existing_launcher_token_skips_lookup() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let mut paths = stub_paths(&dir);
    paths.launcher = None;
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeeeeeee",
            manager: "manual",
            store: "standalone",
            game_id: "eeeeeeee",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            launch_options: Some("/opt/tuxgt-launcher %command%"),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeeeeeee", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.argv()[0], "/opt/tuxgt-launcher");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn steam_applaunch_ignores_detected_exe() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let mut paths = stub_paths(&dir);
    let steam = dir.join("steam");
    std::fs::write(&steam, b"#!/bin/sh\n").unwrap();
    paths.steam = Some(steam.clone());
    seed_game(
        &pool,
        SeedGame {
            id: "steam:standalone:4139290751",
            manager: "steam",
            store: "standalone",
            game_id: "4139290751",
            name: Some("mo2"),
            exe_path: Some(dir.join("ModOrganizer.exe").to_str().unwrap()),
            detected_exe_path: Some(dir.join("Stock Game/SkyrimSE.exe").to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "steam:standalone:4139290751", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.program, steam);
    assert_eq!(
        &spec.args[..],
        ["steam://rungameid/17778118404213833728".to_string()]
    );
    assert!(spec.wrappers.is_empty());
    assert!(!spec.owned);
    assert!(spec
        .argv()
        .iter()
        .all(|a| !a.contains("SkyrimSE") && !a.contains("umu-run")));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn steam_missing_client() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "steam::1",
            manager: "steam",
            game_id: "1",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    let err = build_launch_spec(&pool, &host, "steam::1", &paths, &dir)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::MissingSteam));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn build_launch_spec_succeeds_for_32bit_dx11() {
    let dir = std::env::temp_dir().join(format!("tuxgt-launch32-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Plant both SO fixtures so selection can be asserted on the exact filename
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher.so"), b"64").unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher32.so"), b"32").unwrap();
    let pool = crate::open_db(&dir).await.unwrap();
    let exe = dir.join("game32.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:launch32",
            manager: "manual",
            store: "standalone",
            game_id: "launch32",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    let host = crate::PluginHost::load_with(&[], dir.join("config")).unwrap();
    let paths = crate::launch::LaunchPaths {
        launcher: Some(dir.join("tuxgt-launcher")),
        steam: None,
        heroic: None,
        umu: None,
        wine: None,
    };
    let spec = crate::launch::build_launch_spec(&pool, &host, "manual:standalone:launch32", &paths, &dir).await.unwrap();
    assert!(spec.owned);
    let so = spec.env.get("TUXGT_LAUNCHER_SO").expect("TUXGT_LAUNCHER_SO");
    assert!(so.ends_with("libtuxgt-launcher32.so"), "expected 32-bit SO, got {so:?}");
    // 64-bit control: same setup with bitness 64 must select the 64-bit SO
    let dir64 = std::env::temp_dir().join(format!("tuxgt-launch64-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let _ = std::fs::remove_dir_all(&dir64);
    std::fs::create_dir_all(dir64.join("lib")).unwrap();
    std::fs::write(dir64.join("lib/libtuxgt-launcher.so"), b"64").unwrap();
    std::fs::write(dir64.join("lib/libtuxgt-launcher32.so"), b"32").unwrap();
    let pool64 = crate::open_db(&dir64).await.unwrap();
    let exe64 = dir64.join("game64.exe");
    std::fs::write(&exe64, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool64,
        crate::testing::SeedGame {
            id: "manual:standalone:launch64",
            manager: "manual",
            store: "standalone",
            game_id: "launch64",
            name: Some("t"),
            exe_path: Some(exe64.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let host64 = crate::PluginHost::load_with(&[], dir64.join("config")).unwrap();
    let paths64 = crate::launch::LaunchPaths {
        launcher: Some(dir64.join("tuxgt-launcher")),
        steam: None,
        heroic: None,
        umu: None,
        wine: None,
    };
    let spec64 = crate::launch::build_launch_spec(&pool64, &host64, "manual:standalone:launch64", &paths64, &dir64).await.unwrap();
    let so64 = spec64.env.get("TUXGT_LAUNCHER_SO").expect("TUXGT_LAUNCHER_SO 64");
    assert!(so64.ends_with("libtuxgt-launcher.so") && !so64.ends_with("32.so"), "expected 64-bit SO, got {so64:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir64);
}

