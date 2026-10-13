use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};

#[tokio::test]
async fn pv_rw_includes_game_root_outside_install_dir() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton-CachyOS");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let mo2 = dir.join("MO2");
    let game = dir.join("StockGame");
    std::fs::create_dir_all(&mo2).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    let exe = game.join("SkyrimSE.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:pvrw1",
            manager: "manual",
            store: "standalone",
            game_id: "pvrw1",
            name: Some("mo2"),
            install_dir: Some(mo2.to_str().unwrap()),
            exe_path: Some(mo2.join("ModOrganizer.exe").to_str().unwrap()),
            detected_exe_path: Some(exe.to_str().unwrap()),
            prefix_path: Some(dir.join("pfx").to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    let spec = build_launch_spec(&pool, &host, "manual:standalone:pvrw1", &paths, &dir)
        .await
        .unwrap();
    let rw = spec
        .env
        .get("PRESSURE_VESSEL_FILESYSTEMS_RW")
        .cloned()
        .unwrap_or_default();
    let parts: Vec<_> = rw.split(':').collect();
    assert!(parts.contains(&mo2.to_str().unwrap()), "{rw}");
    assert!(parts.contains(&game.to_str().unwrap()), "{rw}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
