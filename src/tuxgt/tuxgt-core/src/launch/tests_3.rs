use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::write_manifest;
use crate::FileManifest;

#[tokio::test]
async fn winedlloverrides_merges_and_pv_dedupes() {
    let (pool, dir) = pool_dir().await;
    let host = test_host(&dir);
    let paths = stub_paths(&dir);
    let proton_dir = dir.join("Proton 9.0");
    std::fs::create_dir_all(&proton_dir).unwrap();
    let script = proton_dir.join("proton");
    std::fs::write(&script, b"#!/bin/sh\n").unwrap();
    let exe = dir.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    // install_dir == prefix: the PV entry must appear exactly once.
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:eeee0004",
            manager: "manual",
            store: "standalone",
            game_id: "eeee0004",
            name: Some("t"),
            install_dir: Some(dir.to_str().unwrap()),
            exe_path: Some(exe.to_str().unwrap()),
            prefix_path: Some(dir.to_str().unwrap()),
            proton: Some(script.to_str().unwrap()),
            env: Some("{\"WINEDLLOVERRIDES\":\"winmm=n,b\"}"),
            detected_platform: Some("proton"),
            ..Default::default()
        },
    )
    .await;
    write_manifest(
        &dir,
        &FileManifest {
            game: "manual:standalone:eeee0004".into(),
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
    let spec = build_launch_spec(&pool, &host, "manual:standalone:eeee0004", &paths, &dir)
        .await
        .unwrap();
    let overrides = spec
        .env
        .get("WINEDLLOVERRIDES")
        .cloned()
        .unwrap_or_default();
    let mut parts: Vec<&str> = overrides.split(';').collect();
    parts.sort_unstable();
    assert_eq!(parts, vec!["dxgi=n,b", "winmm=n,b"]);
    let rw = spec
        .env
        .get("PRESSURE_VESSEL_FILESYSTEMS_RW")
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        rw.split(':')
            .filter(|p| *p == dir.to_str().unwrap())
            .count(),
        1
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

fn env_manifest(
    game: &str,
    instance: &str,
    enabled: bool,
    rows: &[(&str, &str, bool)],
) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "custom".into(),
        adapter: "preload".into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: rows
            .iter()
            .map(|(k, v, on)| crate::PlannedEnv {
                key: (*k).into(),
                value: (*v).into(),
                enabled: *on,
            })
            .collect(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

#[test]
fn dll_stem_normalizes() {
    assert_eq!(dll_stem("d3dcompiler_47.dll"), "d3dcompiler_47");
    assert_eq!(dll_stem("D3DCompiler_47.DLL"), "d3dcompiler_47");
    assert_eq!(dll_stem(" winmm "), "winmm");
    assert!(dll_stem("").is_empty());
}

#[test]
fn override_entries_split_per_dll() {
    let got: Vec<(String, String)> =
        split_override_entries("comdlg32,shell32=n,b;d3dcompiler_47=n;mscoree=;garbage");
    assert_eq!(
        got,
        vec![
            ("comdlg32".to_string(), "n,b".to_string()),
            ("shell32".to_string(), "n,b".to_string()),
            ("d3dcompiler_47".to_string(), "n".to_string()),
            ("mscoree".to_string(), "".to_string()),
        ]
    );
}

#[test]
fn mod_env_merges_generic_last_wins_and_stems() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e74-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let game = "manual:standalone:modenv1";
    write_manifest(
        &dir,
        &env_manifest(
            game,
            "a-mod",
            true,
            &[
                ("FOO", "1", true),
                ("WINEDLLOVERRIDES", "d3dcompiler_47=n", true),
                ("SKIPME", "x", false),
            ],
        ),
    )
    .unwrap();
    write_manifest(
        &dir,
        &env_manifest(
            game,
            "b-mod",
            true,
            &[("FOO", "2", true), ("WINEDLLOVERRIDES", "winmm=n,b", true)],
        ),
    )
    .unwrap();
    write_manifest(
        &dir,
        &env_manifest(game, "c-mod", false, &[("FOO", "3", true)]),
    )
    .unwrap();
    let mut env = std::collections::BTreeMap::from([
        ("WINEDLLOVERRIDES".to_string(), "winmm=n".to_string()),
        ("OTHER".to_string(), "x".to_string()),
    ]);
    apply_mod_env(&mut env, &dir, game).unwrap();
    assert_eq!(env.get("FOO").map(String::as_str), Some("2"));
    assert_eq!(env.get("SKIPME"), None);
    let o = env.get("WINEDLLOVERRIDES").cloned().unwrap_or_default();
    let mut parts: Vec<&str> = o.split(';').collect();
    parts.sort_unstable();
    assert_eq!(parts, vec!["d3dcompiler_47=n", "winmm=n"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mod_env_follows_load_order() {
    let dir = std::env::temp_dir().join(format!("tuxgt-loadorder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let game = "manual:standalone:loadorder1";
    let mut a = env_manifest(game, "a-mod", true, &[("FOO", "1", true)]);
    a.load_order = 1;
    let mut b = env_manifest(game, "b-mod", true, &[("FOO", "2", true)]);
    b.load_order = 0;
    write_manifest(&dir, &a).unwrap();
    write_manifest(&dir, &b).unwrap();
    let mut env = std::collections::BTreeMap::new();
    apply_mod_env(&mut env, &dir, game).unwrap();
    // `a-mod` loads later (higher load_order), so its value wins.
    assert_eq!(env.get("FOO").map(String::as_str), Some("1"));
    let _ = std::fs::remove_dir_all(&dir);
}
