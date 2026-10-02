use super::testing::*;
use super::*;
use crate::client::StoreClient;
use crate::game::GameId;
use crate::testing::{seed_game, SeedGame};
use crate::{set_knob, unset_knob};
use std::fs;

#[tokio::test]
async fn auto_restore_defers_while_owner_runs_then_resumes() {
    // Needs-gone game with both arms (apply record + handle on) while the
    // owning client runs: store bytes, record, and handle bit all unchanged;
    // the PREFIX-local session rewrite still lands. Once the client is gone
    // the next sync restores + disarms exactly as before.
    let (pool, dir, host, id) = setup().await;
    let gid = GameId::parse(&id).unwrap();
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    unset_knob(&pool, &id, "mangohud").await.unwrap();
    let record_path = crate::game::game_dir(&dir, &gid).join("apply.toml");
    let before = fs::read(&record_path).unwrap();
    let guard = StoreClient::set_running_for_test(StoreClient::Steam, true);
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert_eq!(fs::read(&record_path).unwrap(), before);
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_some());
    assert!(game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=1"), "{text}");
    assert!(!text.contains("MANGOHUD="), "{text}");
    drop(guard);
    // Pinned stopped, not merely unpinned: the resume half must hold on a
    // host that really has Steam running.
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_none());
    assert!(!game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn auto_restore_clears_hook_only_while_owner_runs() {
    // Handle-only arm (no apply record) is PREFIX-local: needs-gone
    // clears the handle even while Steam runs.
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    unset_knob(&pool, &id, "mangohud").await.unwrap();
    let _guard = StoreClient::set_running_for_test(StoreClient::Steam, true);
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(!game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn auto_restore_manual_row_proceeds_despite_steam_running() {
    // Manual rows have no owning client: a running Steam never defers them.
    let (pool, dir, host, _) = setup().await;
    let mid = "manual:standalone:abcdef12";
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:abcdef12",
            manager: "manual",
            store: "standalone",
            game_id: "abcdef12",
            name: Some("m"),
            exe_path: Some(""),
            prefix_path: Some(""),
            ..Default::default()
        },
    )
    .await;
    set_knob(&pool, mid, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, mid, true).await.unwrap();
    unset_knob(&pool, mid, "mangohud").await.unwrap();
    let _guard = StoreClient::set_running_for_test(StoreClient::Steam, true);
    sync_session(&pool, &dir, &host, mid).await.unwrap();
    assert!(!game_handle(&pool, mid).await.unwrap());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn session_writes_preload_flag_for_preload_instance() {
    let (pool, dir, host, id) = setup().await;
    crate::write_manifest(&dir, &opti_manifest(&id, "reshade", "reshade")).unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("preload=1"), "{text}");
    assert!(text.contains("inject=0"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn trampoline_loads_so_when_preload_flag_set() {
    let (pool, dir, host, id) = setup().await;
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let so = dir.join("lib/libtuxgt-launcher.so");
    std::fs::write(&so, b"so").unwrap();
    let probe = dir.join("probe.exe");
    std::fs::write(&probe, "#!/bin/sh\necho \"PROBE_LD_PRELOAD=$LD_PRELOAD\"\n").unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&probe)
        .status()
        .unwrap();
    crate::write_manifest(&dir, &opti_manifest(&id, "reshade", "reshade")).unwrap();
    crate::set_override(&pool, &id, "exe", Some(probe.to_str().unwrap()))
        .await
        .unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    let launcher =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../launcher/tuxgt-launcher");
    let hit = std::process::Command::new("sh")
        .arg(&launcher)
        .arg(&probe)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &dir)
        .env("TUXGT_DATA", &dir)
        .env("TUXGT_LAUNCHER_SO", &so)
        .env("STEAM_COMPAT_DATA_PATH", "/pfx/sekiro")
        .env("LD_PRELOAD", "/sentinel/keep.so")
        .output()
        .unwrap();
    assert!(hit.status.success());
    let out = String::from_utf8(hit.stdout).unwrap();
    assert!(out.contains("/sentinel/keep.so"), "{out}");
    assert!(out.contains("libtuxgt-launcher.so"), "{out}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
