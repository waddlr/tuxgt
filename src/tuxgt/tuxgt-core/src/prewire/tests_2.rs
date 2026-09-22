use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use std::fs;

#[tokio::test]
async fn prewire_32bit_dx11_uses_reshade32_and_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game32_dx11.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:pre32dx11",
            manager: "manual",
            store: "standalone",
            game_id: "pre32dx11",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    let mut m = preload_manifest("manual:standalone:pre32dx11", "reshade32", "preload", true);
    // Override dest to ReShade32
    m.files = vec![crate::download::PlannedFile {
        source: "s".into(),
        dest: "ReShade32.dll".into(),
        sha256: "a".into(),
        enabled: true,
    }]
    .into_boxed_slice();
    crate::write_manifest(&data, &m).unwrap();
    prewire_game(&data, &pool, "manual:standalone:pre32dx11")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:pre32dx11").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx11_32"), "{text}");
    assert!(text.contains("ReShade32.dll"), "{text}");
    assert!(!text.contains("ReShade64.dll"), "{text}");
    let _ = std::fs::remove_dir_all(&data);
}
#[tokio::test]
async fn prewire_32bit_dx12_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game32_dx12.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:pre32dx12",
            manager: "manual",
            store: "standalone",
            game_id: "pre32dx12",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    // No manifest: just check Type is still written via gameinfo
    prewire_game(&data, &pool, "manual:standalone:pre32dx12")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:pre32dx12").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx12_32"), "{text}");
    let _ = std::fs::remove_dir_all(&data);
}
#[tokio::test]
async fn prewire_dx9_64_emits_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game_dx9_64.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:predx964",
            manager: "manual",
            store: "standalone",
            game_id: "predx964",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx9"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    prewire_game(&data, &pool, "manual:standalone:predx964")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:predx964").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx9_64"), "{text}");
    assert!(!text.contains("Type=dx11"), "{text}");
    assert!(!text.contains("Type=dx12"), "{text}");
    let _ = fs::remove_dir_all(&data);
}
#[tokio::test]
async fn prewire_dx10_64_emits_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game_dx10_64.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:predx1064",
            manager: "manual",
            store: "standalone",
            game_id: "predx1064",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx10"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    prewire_game(&data, &pool, "manual:standalone:predx1064")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:predx1064").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx10_64"), "{text}");
    let _ = fs::remove_dir_all(&data);
}
#[tokio::test]
async fn prewire_dx9_32_uses_reshade32_and_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game_dx9_32.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:predx932",
            manager: "manual",
            store: "standalone",
            game_id: "predx932",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx9"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    let mut m = preload_manifest("manual:standalone:predx932", "reshade32", "preload", true);
    m.files = vec![crate::download::PlannedFile {
        source: "s".into(),
        dest: "ReShade32.dll".into(),
        sha256: "a".into(),
        enabled: true,
    }]
    .into_boxed_slice();
    crate::write_manifest(&data, &m).unwrap();
    prewire_game(&data, &pool, "manual:standalone:predx932")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:predx932").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx9_32"), "{text}");
    assert!(text.contains("ReShade32.dll"), "{text}");
    assert!(!text.contains("ReShade64.dll"), "{text}");
    let _ = fs::remove_dir_all(&data);
}
#[tokio::test]
async fn prewire_dx10_32_emits_type() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game_dx10_32.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id: "manual:standalone:predx1032",
            manager: "manual",
            store: "standalone",
            game_id: "predx1032",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx10"),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    prewire_game(&data, &pool, "manual:standalone:predx1032")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:predx1032").unwrap(),
    );
    let text = std::fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("Type=dx10_32"), "{text}");
    let _ = fs::remove_dir_all(&data);
}
