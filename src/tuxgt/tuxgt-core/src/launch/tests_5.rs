//! R37 regression contract: plain Play stays vanilla and never becomes an
//! install operation because a game picked the Install adapter.
//!
//! The GUI's `launch_play` is a direct spawn, so what core owes it is a
//! LaunchSpec that is still the plain store launch. Named-injector dests
//! are install-only (Hook illegal, Apply required); a proxy dest
//! (`dxgi.dll`) also needs Apply.
use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{game_adapter, set_game_adapter, write_manifest, FileManifest, PlannedFile};
use std::fs;

fn install_manifest(game: &str, instance: &str, dest: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![PlannedFile {
            source: format!("cache/{instance}/{dest}"),
            dest: dest.into(),
            sha256: "aa".into(),
            enabled: true,
            load: None,
        }]
        .into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

/// A store game on the Install adapter with enabled install manifests and
/// the hook channel off: Play must still be the plain store launch, and the
/// launch path must not touch the stored choice.
#[tokio::test]
async fn plain_play_of_an_install_game_stays_vanilla() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
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
            name: Some("Play"),
            install_dir: Some(dir.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    // The user picked Install for this game and installed a Mod on it.
    set_game_adapter(&pool, id, "install").await.unwrap();
    write_manifest(&dir, &install_manifest(id, "reshade", "ReShade64.dll")).unwrap();
    assert!(!crate::game_handle(&pool, id).await.unwrap());
    assert_eq!(game_adapter(&pool, id).await.unwrap(), "install");

    // Named injector dest: Hook is illegal; Apply is the channel.
    let needs = game_launch_needs(&pool, &dir, id).await.unwrap();
    assert!(needs.install_only, "{needs:?}");
    assert!(needs.show_radio(), "install-only still needs Apply");
    assert!(needs.channel_needed());
    assert!(!needs.hook_legal(true), "install-only forbids Hook");
    assert!(needs.apply_legal());

    // The spec is still the plain store launch.
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(
        spec.program, steam,
        "plain Play must still launch through the store"
    );
    assert!(
        spec.args.iter().any(|a| a.starts_with("steam://")),
        "store launch carries the steam uri: {:?}",
        spec.args
    );
    // Store Play is not the loader: no managed ini is exported, and the
    // game dir carries no TuxGT env.
    assert!(spec.env.get("TUXGT_LAUNCHER_INI").is_none());
    assert!(spec.env.get("TUXGT_DEPOT").is_none());

    // Building the spec never writes the adapter choice (Play is not an
    // install operation), and the manifests are untouched.
    assert_eq!(game_adapter(&pool, id).await.unwrap(), "install");
    let m = crate::need_manifest(&dir, id, "reshade").unwrap();
    assert_eq!(m.adapter, "install");
    assert!(m.enabled);
    // No loader ini was written for a plain store Play.
    let gid = crate::game::GameId::parse(id).unwrap();
    let ini = crate::prewire::managed_ini(&crate::game::game_dir(&dir, &gid));
    assert!(
        !ini.exists() || !fs::read_to_string(&ini).unwrap().contains("ReShade64.dll"),
        "plain Play must not prewire"
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// Proxy-named install dest writes WINEDLLOVERRIDES: Hook is illegal,
/// Apply is required. Play itself stays the store launch until Apply arms.
#[tokio::test]
async fn install_proxy_dest_needs_apply() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
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
            name: Some("Play"),
            install_dir: Some(dir.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    set_game_adapter(&pool, id, "install").await.unwrap();
    write_manifest(&dir, &install_manifest(id, "optiscaler", "dxgi.dll")).unwrap();
    let needs = game_launch_needs(&pool, &dir, id).await.unwrap();
    assert!(needs.env, "{needs:?}");
    assert!(!needs.install_only);
    assert!(needs.show_radio());
    assert!(!needs.hook_legal(true));
    assert!(needs.apply_legal());
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert_eq!(spec.program, steam);
    assert!(spec.env.get("TUXGT_LAUNCHER_INI").is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// Preload keeps the channel: the same game on the other adapter still
/// needs one, so the selector did not collapse the launch modes globally.
#[tokio::test]
async fn a_preload_game_keeps_its_channel() {
    let (pool, dir) = pool_dir().await;
    let id = "steam::r37preload";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            store: "",
            game_id: "r37preload",
            name: Some("Preload"),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(game_adapter(&pool, id).await.unwrap(), "preload");
    let mut m = install_manifest(id, "reshade", "dxgi.dll");
    m.adapter = "preload".into();
    write_manifest(&dir, &m).unwrap();
    let needs = game_launch_needs(&pool, &dir, id).await.unwrap();
    assert!(needs.channel_needed());
    assert!(needs.show_radio());
    assert!(needs.hook_legal(true));
    assert!(!needs.install_only);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_when_hook_illegal_is_noop_when_hook_legal() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let id = "steam::814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    crate::set_knob(&pool, id, "mangohud", "1").await.unwrap();
    let out = crate::apply_when_hook_illegal(&pool, &dir, &host, id)
        .await
        .unwrap();
    assert!(out.is_none());
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_when_hook_illegal_install_only_skips_while_owner_runs() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let id = "steam::814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(&dir, &install_manifest(id, "reshade", "ReShade64.dll")).unwrap();
    let needs = crate::game_launch_needs(&pool, &dir, id).await.unwrap();
    assert!(needs.install);
    assert!(needs.apply_legal());
    assert!(!needs.hook_legal(true));
    let _guard = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, true);
    let out = crate::apply_when_hook_illegal(&pool, &dir, &host, id)
        .await
        .unwrap();
    assert!(out.is_none());
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_when_hook_illegal_mixed_install_skips_while_owner_runs() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let id = "steam::814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    let mut pre = install_manifest(id, "reshade", "ReShade64.dll");
    pre.adapter = "preload".into();
    write_manifest(&dir, &pre).unwrap();
    write_manifest(&dir, &install_manifest(id, "opti", "OptiScaler.dll")).unwrap();
    let needs = crate::game_launch_needs(&pool, &dir, id).await.unwrap();
    assert!(needs.install);
    assert!(needs.preload);
    assert!(needs.hook_legal(true));
    assert!(needs.apply_legal());
    let _guard = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, true);
    let out = crate::apply_when_hook_illegal(&pool, &dir, &host, id)
        .await
        .unwrap();
    assert!(out.is_none());
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_when_hook_illegal_is_noop_when_already_applied() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let id = "steam::814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(&dir, &install_manifest(id, "optiscaler", "dxgi.dll")).unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.into(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    let out = crate::apply_when_hook_illegal(&pool, &dir, &host, id)
        .await
        .unwrap();
    assert!(out.is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_when_hook_illegal_skips_while_owner_runs() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let id = "steam::814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(&dir, &install_manifest(id, "optiscaler", "dxgi.dll")).unwrap();
    let _guard = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, true);
    let out = crate::apply_when_hook_illegal(&pool, &dir, &host, id)
        .await
        .unwrap();
    assert!(out.is_none());
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
