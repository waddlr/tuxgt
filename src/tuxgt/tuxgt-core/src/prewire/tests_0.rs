use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use std::fs;
use std::path::Path;

#[test]
fn section_replace_preserves_rest() {
    let data = data();
    let ini = managed_ini(&data);
    set_ini_section(
        &ini,
        "game",
        &[
            "Type=dx12_64".into(),
            "LoadDLL=dxgi.dll=s/i/dxgi.dll".into(),
        ],
    )
    .unwrap();
    set_ini_section(&ini, "other", &["Type=dx11_64".into()]).unwrap();
    // Rewrite with different case; loader matches case-insensitively.
    set_ini_section(&ini, "GAME", &["Type=dx12_64".into()]).unwrap();
    let text = fs::read_to_string(managed_ini(&data)).unwrap();
    assert_eq!(
        text.matches("[game]").count() + text.matches("[GAME]").count(),
        1
    );
    assert!(text.contains("[other]\nType=dx11_64\n"));
    remove_ini_section(&ini, "gAmE").unwrap();
    let text = fs::read_to_string(managed_ini(&data)).unwrap();
    assert!(!text.contains("LoadDLL="));
    assert!(text.contains("[other]"));
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn init_keys_owned_rest_preserved() {
    let data = data();
    fs::create_dir_all(&data).unwrap();
    let ini = managed_ini(&data);
    fs::write(&ini, "[Init]\nCustom=1\n").unwrap();
    ensure_game_init(&ini).unwrap();
    let text = fs::read_to_string(managed_ini(&data)).unwrap();
    assert!(text.contains("Custom=1"));
    assert!(text.contains("GamesDir=runtime"));
    assert!(text.contains("DepotDir=stage"));
    let _ = fs::remove_dir_all(&data);
}

async fn game_pool(dir: &Path) -> SqlitePool {
    crate::open_db(dir).await.unwrap()
}

fn preload_manifest(
    game: &str,
    instance: &str,
    adapter: &str,
    enabled: bool,
) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: vec![
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "ReShade64.dll".into(),
                sha256: "a".into(),
                enabled: true,
            },
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "notes.txt".into(),
                sha256: "b".into(),
                enabled: true,
            },
        ]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}

#[test]
fn prefix_dests_never_reach_loader_lists() {
    let mut m = preload_manifest("manual:standalone:ffff0009", "pfxmod", "install", true);
    m.files = m
        .files
        .into_vec()
        .into_iter()
        .chain([crate::download::PlannedFile {
            source: "s".into(),
            dest: "pfx:windows/system32/foo.dll".into(),
            sha256: "c".into(),
            enabled: true,
        }])
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let lines = ini_lines_for(&m);
    assert!(lines.iter().any(|l| l.contains("ReShade64.dll")));
    assert!(lines.iter().all(|l| !l.contains("pfx:")));
}

#[tokio::test]
async fn section_from_gameinfo_and_preload() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0001",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0001",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("manual:standalone:ffff0001", "reshade", "preload", true),
    )
    .unwrap();
    prewire_game(&data, &pool, "manual:standalone:ffff0001")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0001").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("[game]"));
    assert!(text.contains("Type=dx12_64"));
    assert!(text.contains("GamesDir=runtime"));
    assert!(text.contains("LoadDLL=ReShade64.dll=reshade/ReShade64.dll"));
    assert!(text.contains("IncludeFile=notes.txt=reshade/notes.txt"));
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn detected_exe_only_gets_section() {
    // Heroic standalone rows often carry no store exe: detectors found it.
    let data = data();
    let pool = game_pool(&data).await;
    seed_game(
        &pool,
        SeedGame {
            id: "heroic:standalone:aa11",
            manager: "heroic",
            store: "standalone",
            game_id: "aa11",
            name: Some("t"),
            detected_exe_path: Some("/games/CrimsonDesert.exe"),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("heroic:standalone:aa11", "reshade", "preload", true),
    )
    .unwrap();
    prewire_game(&data, &pool, "heroic:standalone:aa11")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("heroic:standalone:aa11").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("[CrimsonDesert]"));
    assert!(text.contains("Type=dx12_64"));
    assert!(text.contains("LoadDLL=ReShade64.dll=reshade/ReShade64.dll"));
}
#[tokio::test]
async fn steam_row_gets_section() {
    // Steam rows prewire `[<stem>]` like every other manager.
    let data = data();
    let pool = game_pool(&data).await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam:store:3321460",
            manager: "steam",
            store: "store",
            game_id: "3321460",
            name: Some("t"),
            detected_exe_path: Some("/games/CrimsonDesert.exe"),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("steam:store:3321460", "reshade", "preload", true),
    )
    .unwrap();
    prewire_game(&data, &pool, "steam:store:3321460")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("steam:store:3321460").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(text.contains("[CrimsonDesert]"));
    assert!(text.contains("Type=dx12_64"));
    assert!(text.contains("LoadDLL=ReShade64.dll=reshade/ReShade64.dll"));
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn steam_and_install_only_leave_no_section() {
    let data = data();
    let pool = game_pool(&data).await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::42",
            manager: "steam",
            game_id: "42",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("steam::42", "reshade", "preload", true),
    )
    .unwrap();
    prewire_game(&data, &pool, "steam::42").await.unwrap();
    let sdir = crate::game::game_dir(&data, &crate::game::GameId::parse("steam::42").unwrap());
    let text = fs::read_to_string(managed_ini(&sdir)).unwrap();
    assert!(text.contains("GamesDir=runtime"));
    assert!(!text.contains("LoadDLL="));
    let exe = data.join("tool.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0002",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0002",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    crate::write_manifest(
        &data,
        &preload_manifest("manual:standalone:ffff0002", "opti", "install", true),
    )
    .unwrap();
    prewire_game(&data, &pool, "manual:standalone:ffff0002")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0002").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    assert!(!text.contains("[tool]"));
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn effect_pack_emits_one_tree_include() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0004",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0004",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let mut files = Vec::new();
    for i in 0..44 {
        files.push(crate::download::PlannedFile {
            source: "s".into(),
            dest: format!("reshade-shaders/Shaders/lilium__{i}.fx"),
            sha256: "h".into(),
            enabled: true,
        });
    }
    for i in 0..3 {
        files.push(crate::download::PlannedFile {
            source: "s".into(),
            dest: format!("reshade-shaders/Textures/lilium__{i}.png"),
            sha256: "h".into(),
            enabled: true,
        });
    }
    let m = crate::FileManifest {
        game: "manual:standalone:ffff0004".into(),
        instance: "my-effect".into(),
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
    prewire_game(&data, &pool, "manual:standalone:ffff0004")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0004").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    let inc: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("IncludeFile="))
        .collect();
    // One tree entry covers both roots; the launcher caps the list at 8192.
    assert_eq!(inc.len(), 1, "{inc:?}");
    assert_eq!(
        inc[0],
        "IncludeFile=reshade-shaders/=my-effect/reshade-shaders/"
    );
    let total: usize = inc.iter().map(|l| l.len() + 2).sum();
    assert!(total < 8192, "IncludeFile list {total} bytes");
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn nested_pack_emits_one_tree_include() {
    let data = data();
    let pool = game_pool(&data).await;
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:ffff0005",
            manager: "manual",
            store: "standalone",
            game_id: "ffff0005",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let mut files = Vec::new();
    for i in 0..139 {
        files.push(crate::download::PlannedFile {
            source: "s".into(),
            dest: format!("Luma/CrimsonDesert/Luma_{i}.hlsl"),
            sha256: "h".into(),
            enabled: true,
        });
    }
    // Root-level non-DLL dest keeps the per-file form.
    files.push(crate::download::PlannedFile {
        source: "s".into(),
        dest: "Luma-Crimson-Desert.addon".into(),
        sha256: "h".into(),
        enabled: true,
    });
    let m = crate::FileManifest {
        game: "manual:standalone:ffff0005".into(),
        instance: "luma-crimson-desert".into(),
        mod_type: "reshade_addon".into(),
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
    prewire_game(&data, &pool, "manual:standalone:ffff0005")
        .await
        .unwrap();
    let gdir = crate::game::game_dir(
        &data,
        &crate::game::GameId::parse("manual:standalone:ffff0005").unwrap(),
    );
    let text = fs::read_to_string(managed_ini(&gdir)).unwrap();
    let inc: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("IncludeFile="))
        .collect();
    assert_eq!(inc.len(), 2, "{inc:?}");
    assert!(
        inc.contains(&"IncludeFile=Luma/=luma-crimson-desert/Luma/"),
        "{inc:?}"
    );
    assert!(
        inc.contains(
            &"IncludeFile=Luma-Crimson-Desert.addon=luma-crimson-desert/Luma-Crimson-Desert.addon"
        ),
        "{inc:?}"
    );
    let total: usize = inc.iter().map(|l| l.len() + 2).sum();
    assert!(total < 8192, "IncludeFile list {total} bytes");
    let _ = fs::remove_dir_all(&data);
}
