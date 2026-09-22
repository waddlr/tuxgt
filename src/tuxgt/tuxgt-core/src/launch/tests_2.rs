use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::FileManifest;
use crate::{disable_knob, set_custom, set_global_knob, set_knob, write_manifest};
use std::path::PathBuf;

#[tokio::test]
async fn global_then_game_then_custom_and_skip_disabled() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let _g = crate::env::TEST_ENV_LOCK.lock().unwrap();
    let (pool, dir) = pool_dir().await;
    let prev_hud = std::env::var_os("DXVK_HUD");
    let prev_mango = std::env::var_os("MANGOHUD");
    std::env::remove_var("DXVK_HUD");
    std::env::remove_var("MANGOHUD");
    let host = env_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeee0003",
            manager: "manual",
            store: "standalone",
            game_id: "eeee0003",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            env: Some("{\"B\":\"store\"}"),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    set_global_knob(&pool, "dxvk-hud", "full").await.unwrap();
    set_global_knob(&pool, "mangohud", "1").await.unwrap();
    set_knob(&pool, "manual:standalone:eeee0003", "dxvk-hud", "1")
        .await
        .unwrap();
    set_knob(&pool, "manual:standalone:eeee0003", "mangohud", "1")
        .await
        .unwrap();
    disable_knob(&pool, "manual:standalone:eeee0003", "mangohud")
        .await
        .unwrap();
    set_knob(&pool, "manual:standalone:eeee0003", "proton-wined3d", "1")
        .await
        .unwrap();
    disable_knob(&pool, "manual:standalone:eeee0003", "proton-wined3d")
        .await
        .unwrap();
    set_custom(&pool, "manual:standalone:eeee0003", "B", "custom")
        .await
        .unwrap();
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeee0003", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.env.get("DXVK_HUD").map(String::as_str), Some("1"));
    assert_eq!(spec.env.get("MANGOHUD").map(String::as_str), Some("1"));
    assert!(
        !spec.env.contains_key("PROTON_USE_WINED3D"),
        "{:?}",
        spec.env
    );
    assert_eq!(spec.env.get("B").map(String::as_str), Some("custom"));
    match prev_hud {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
    match prev_mango {
        Some(v) => std::env::set_var("MANGOHUD", v),
        None => std::env::remove_var("MANGOHUD"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn disabled_provider_knobs_skipped() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeee0002",
            manager: "manual",
            store: "standalone",
            game_id: "eeee0002",
            name: Some("t"),
            exe_path: Some(true_bin.to_str().unwrap()),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    set_knob(&pool, "manual:standalone:eeee0002", "mangohud", "1")
        .await
        .unwrap();
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeee0002", &paths, &dir)
        .await
        .unwrap();
    assert!(!spec.env.contains_key("MANGOHUD"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn proton_env_plan_and_pv_rw() {
    let (pool, dir) = pool_dir().await;
    crate::instance::seed_official_share_from_repo(&dir);
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton-CachyOS");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:penv9",
            manager: "manual",
            store: "standalone",
            game_id: "penv9",
            name: Some("s"),
            exe_path: Some(exe.to_str().unwrap()),
            prefix_path: Some(dir.join("pfx").to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(
        &dir,
        &FileManifest {
            game: "manual:standalone:penv9".into(),
            instance: "optiscaler".into(),
            mod_type: "optiscaler".into(),
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
    let spec = build_launch_spec(&pool, &host, "manual:standalone:penv9", &paths, &dir)
        .await
        .unwrap();
    assert_eq!(
        spec.env.get("PROTON_USE_OPTISCALER").map(String::as_str),
        Some("1")
    );
    let rw = spec
        .env
        .get("PRESSURE_VESSEL_FILESYSTEMS_RW")
        .cloned()
        .unwrap_or_default();
    assert!(rw
        .split(':')
        .any(|p| p == dir.join("pfx").to_str().unwrap()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn no_proton_env_without_flavor() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton 9.0");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:penv8",
            manager: "manual",
            store: "standalone",
            game_id: "penv8",
            name: Some("s"),
            exe_path: Some(exe.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(
        &dir,
        &FileManifest {
            game: "manual:standalone:penv8".into(),
            instance: "optiscaler".into(),
            mod_type: "optiscaler".into(),
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
    let spec = build_launch_spec(&pool, &host, "manual:standalone:penv8", &paths, &dir)
        .await
        .unwrap();
    assert!(!spec.env.contains_key("PROTON_USE_OPTISCALER"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
#[tokio::test]
async fn no_proton_env_without_plan() {
    // reshade's official recipe lacks proton_env: the flavor matches
    // but the recipe gate denies the grant.
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton-CachyOS");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:penv7",
            manager: "manual",
            store: "standalone",
            game_id: "penv7",
            name: Some("s"),
            exe_path: Some(exe.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(
        &dir,
        &FileManifest {
            game: "manual:standalone:penv7".into(),
            instance: "reshade".into(),
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
    let spec = build_launch_spec(&pool, &host, "manual:standalone:penv7", &paths, &dir)
        .await
        .unwrap();
    assert!(!spec.env.contains_key("PROTON_USE_OPTISCALER"));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn proton_env_gate_follows_recipe() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-penv-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    crate::instance::seed_official_share_from_repo(&dir);
    assert!(instance_allows_proton_env(&dir, "optiscaler").unwrap());
    assert!(!instance_allows_proton_env(&dir, "reshade").unwrap());
    assert!(!instance_allows_proton_env(&dir, "no-such-instance").unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn managed_exports_and_winedlloverrides() {
    let true_bin = PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("GE-Proton9-1");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeee0003",
            manager: "manual",
            store: "standalone",
            game_id: "eeee0003",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(
        &dir,
        &FileManifest {
            game: "manual:standalone:eeee0003".into(),
            instance: "optiscaler".into(),
            mod_type: "optiscaler".into(),
            adapter: "install".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![crate::download::PlannedFile {
                source: "cache/k/f#dxgi.dll".into(),
                dest: "dxgi.dll".into(),
                sha256: "aa".into(),
                enabled: true,
            }]
            .into_boxed_slice(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
            env: Box::default(),
        },
    )
    .unwrap();
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeee0003", &paths, &dir)
        .await
        .unwrap();
    assert!(spec
        .env
        .get("TUXGT_LAUNCHER_INI")
        .is_some_and(|p| p.ends_with("tuxgt-launcher.ini")));
    assert!(spec.env.contains_key("TUXGT_GAME_DIR"));
    assert!(spec.env.contains_key("TUXGT_DEPOT"));
    assert!(spec
        .env
        .get("WINEDLLOVERRIDES")
        .is_some_and(|v| v.split(';').any(|e| e == "dxgi=n,b")));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
