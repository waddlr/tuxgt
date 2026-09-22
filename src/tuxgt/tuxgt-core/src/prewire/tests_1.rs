use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use std::fs;

fn lines_for(files: Vec<(&str, bool)>, include: &[&str]) -> Vec<String> {
    let m = crate::FileManifest {
        game: "g".into(),
        instance: "inst".into(),
        mod_type: "effect".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: include.iter().map(|s| s.to_string()).collect(),
        files: files
            .into_iter()
            .map(|(dest, enabled)| crate::download::PlannedFile {
                source: "s".into(),
                dest: dest.into(),
                sha256: "h".into(),
                enabled,
            })
            .collect(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    ini_lines_for(&m)
}

#[test]
fn fully_omitted_dir_emits_nothing() {
    let lines = lines_for(vec![("Luma/a.hlsl", false), ("Luma/b.hlsl", false)], &[]);
    assert!(lines.is_empty(), "{lines:?}");
}

#[test]
fn kept_root_file_stays_per_file_line() {
    let lines = lines_for(vec![("notes.txt", true)], &[]);
    assert_eq!(lines, ["IncludeFile=notes.txt=inst/notes.txt"]);
}

#[test]
fn include_covered_dll_joins_tree() {
    let lines = lines_for(
        vec![("shaders/foo.dll", true), ("shaders/a.fx", true)],
        &["shaders/"],
    );
    assert_eq!(lines, ["IncludeFile=shaders/=inst/shaders/"], "{lines:?}");
}

#[tokio::test]
async fn omitted_dest_keeps_tree_collapse() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0006",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0006",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let file = |dest: &str, enabled: bool| crate::download::PlannedFile {
        source: "s".into(),
        dest: dest.into(),
        sha256: "h".into(),
        enabled,
    };
    let m = crate::FileManifest {
        game: "manual:standalone:ffff0006".into(),
        instance: "luma-crimson-desert".into(),
        mod_type: "reshade_addon".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![
            file("dxgi.dll", true),
            file("OptiScaler.ini", false),
            file("Luma/a.hlsl", true),
            file("Luma/b.hlsl", false),
            file("Luma/c.hlsl", true),
        ]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&data, &m).unwrap();
    prewire_game(&data, &pool, "manual:standalone:ffff0006")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0006").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(
        text.contains("LoadDLL=dxgi.dll=luma-crimson-desert/dxgi.dll"),
        "{text}"
    );
    assert!(!text.contains("OptiScaler.ini"), "{text}");
    assert!(!text.contains("b.hlsl"), "{text}");
    // Always-tree: one omission no longer breaks the collapse; the kept
    // files ride the tree line and no per-file lines are emitted.
    let inc: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("IncludeFile="))
        .collect();
    assert_eq!(
        inc,
        ["IncludeFile=Luma/=luma-crimson-desert/Luma/"],
        "{text}"
    );
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn disabled_manifest_drops_section() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0003",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0003",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx11"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("manual:standalone:ffff0003", "reshade", "preload", false),
    )
    .unwrap();
    prewire_game(&data, &pool, "manual:standalone:ffff0003")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0003").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    // Type-only body is still written; no stale LoadDLL survives.
    assert!(text.contains("[game]"));
    assert!(text.contains("Type=dx11_64"));
    assert!(!text.contains("LoadDLL="));
    let _ = fs::remove_dir_all(&data);
}
#[test]
fn budget_rejects_overflowing_loaddll_list() {
    let body: Vec<String> = (0..300)
        .map(|i| {
            format!("LoadDLL=very-long-dll-name-{i:03}.dll=inst/very-long-dll-name-{i:03}.dll")
        })
        .collect();
    let err = check_ini_budget("manual:standalone:budget", "game", &body).unwrap_err();
    assert!(err.to_string().contains("8192-byte budget"), "{err}");
}

#[test]
fn budget_accepts_collapsed_tree() {
    let body = vec!["IncludeFile=reshade-shaders/=inst/reshade-shaders/".to_string()];
    check_ini_budget("g", "s", &body).unwrap();
}

#[tokio::test]
async fn prewire_refuses_pack_over_budget() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0007",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0007",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // 400 top-level dirs collapse to 400 tree lines: over the 8192 cap.
    let files: Vec<crate::download::PlannedFile> = (0..400)
        .map(|i| crate::download::PlannedFile {
            source: "s".into(),
            dest: format!("dir{i:03}/f.fx"),
            sha256: "h".into(),
            enabled: true,
        })
        .collect();
    let m = crate::FileManifest {
        game: "manual:standalone:ffff0007".into(),
        instance: "manydirs".into(),
        mod_type: "effect".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: files.into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&data, &m).unwrap();
    let err = prewire_game(&data, &pool, "manual:standalone:ffff0007")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("8192-byte budget"), "{err}");
    let _ = fs::remove_dir_all(&data);
}
#[test]
fn include_dll_renders_includefile_not_loaddll() {
    let file = |dest: &str| crate::download::PlannedFile {
        source: "s".into(),
        dest: dest.into(),
        sha256: "h".into(),
        enabled: true,
    };
    let m = crate::FileManifest {
        game: "manual:standalone:inc1".into(),
        instance: "incmod".into(),
        mod_type: "custom".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: vec!["proxy.dll".into()].into_boxed_slice(),
        files: vec![file("proxy.dll"), file("other.dll")].into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    let lines = ini_lines_for(&m);
    assert!(
        lines.contains(&"IncludeFile=proxy.dll=incmod/proxy.dll".to_string()),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("LoadDLL=proxy.dll=")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"LoadDLL=other.dll=incmod/other.dll".to_string()),
        "{lines:?}"
    );
}

#[tokio::test]
async fn shared_dest_emits_both_lines_in_load_order() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    let game = "manual:standalone:loadorder2";
    seed_game(
        &pool,
        SeedGame {
            id: game,
            manager: "manual",
            store: "standalone",
            game_id: "loadorder2",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let file = |dest: &str| crate::download::PlannedFile {
        source: "s".into(),
        dest: dest.into(),
        sha256: "h".into(),
        enabled: true,
    };
    let mk = |instance: &str, order: i64| crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "effect".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: order,
        include: Box::default(),
        files: vec![file("reshade-shaders/Shaders/shared.fx")].into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&data, &mk("aaa", 0)).unwrap();
    crate::write_manifest(&data, &mk("zzz", 1)).unwrap();
    prewire_game(&data, &pool, game).await.unwrap();
    let gdir = crate::game::game_dir(&data, &crate::game::GameId::parse(game).unwrap());
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    let pos_a = text.find("aaa/reshade-shaders/").expect("aaa line");
    let pos_z = text.find("zzz/reshade-shaders/").expect("zzz line");
    assert!(pos_a < pos_z, "{text}");
    // Flip the order: the later-in-order manifest emits last.
    crate::write_manifest(&data, &mk("aaa", 2)).unwrap();
    prewire_game(&data, &pool, game).await.unwrap();
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    let pos_a = text.find("aaa/reshade-shaders/").expect("aaa line");
    let pos_z = text.find("zzz/reshade-shaders/").expect("zzz line");
    assert!(pos_z < pos_a, "{text}");
    let _ = fs::remove_dir_all(&data);
}
