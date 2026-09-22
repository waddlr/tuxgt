//! Proxy-slot `WINEDLLOVERRIDES`. `<self>` stock names add nothing.
use super::testing::*;
use super::*;
use crate::download::PlannedFile;
use crate::testing::{seed_game, SeedGame};
use crate::{write_manifest, FileManifest, PlannedEnv};

fn files(dests: &[&str]) -> Box<[PlannedFile]> {
    dests
        .iter()
        .map(|dest| PlannedFile {
            source: format!("cache/k/{dest}"),
            dest: (*dest).into(),
            sha256: "aa".into(),
            enabled: true,
            load: None,
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn manifest(
    game: &str,
    adapter: &str,
    dests: &[&str],
    enabled: bool,
    env: &[(&str, &str)],
) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: "optiscaler".into(),
        mod_type: "optiscaler".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: files(dests),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: env
            .iter()
            .map(|(k, v)| PlannedEnv {
                key: (*k).into(),
                value: (*v).into(),
                enabled: true,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    }
}

async fn overrides(adapter: &str, dests: &[&str], enabled: bool, env: &[(&str, &str)]) -> String {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("GE-Proton9-1");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    let id = "manual:standalone:slotenv";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: "slotenv",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(&dir, &manifest(id, adapter, dests, enabled, env)).unwrap();
    let spec = build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    let out = spec
        .env
        .get("WINEDLLOVERRIDES")
        .cloned()
        .unwrap_or_default();
    let _ = tokio::fs::remove_dir_all(&dir).await;
    out
}

fn has_stem(value: &str, entry: &str) -> bool {
    value.split(';').any(|e| e == entry)
}

#[tokio::test]
async fn proxy_slot_sets_override_for_preload_and_install() {
    let preload = overrides("preload", &["dxgi.dll"], true, &[]).await;
    assert!(has_stem(&preload, "dxgi=n,b"), "{preload}");
    let install = overrides("install", &["d3d11.dll"], true, &[]).await;
    assert!(has_stem(&install, "d3d11=n,b"), "{install}");
}

#[tokio::test]
async fn self_slot_skips_stock_name_and_keeps_companion() {
    let install = overrides("install", &["OptiScaler.dll", "nvngx.dll"], true, &[]).await;
    assert!(!has_stem(&install, "OptiScaler=n,b"), "{install}");
    assert!(has_stem(&install, "nvngx=n,b"), "{install}");
    let preload = overrides("preload", &["ReShade64.dll"], true, &[]).await;
    assert!(preload.is_empty(), "{preload}");
}

#[tokio::test]
async fn existing_stem_wins_and_disabled_slot_adds_nothing() {
    let kept = overrides(
        "preload",
        &["dxgi.dll"],
        true,
        &[("WINEDLLOVERRIDES", "dxgi=b")],
    )
    .await;
    assert_eq!(kept, "dxgi=b");
    let off = overrides("install", &["dxgi.dll"], false, &[]).await;
    assert!(off.is_empty(), "{off}");
}

#[tokio::test]
async fn grouped_override_keeps_its_stem() {
    let kept = overrides(
        "install",
        &["dxgi.dll", "nvngx.dll"],
        true,
        &[("WINEDLLOVERRIDES", "d3d11,dxgi.dll=b")],
    )
    .await;
    assert!(has_stem(&kept, "dxgi=b"), "{kept}");
    assert!(!has_stem(&kept, "dxgi=n,b"), "{kept}");
    assert!(has_stem(&kept, "d3d11=b"), "{kept}");
    assert!(has_stem(&kept, "nvngx=n,b"), "{kept}");
}
