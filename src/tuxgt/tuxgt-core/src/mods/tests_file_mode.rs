use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use std::fs;

fn file(dest: &str) -> crate::PlannedFile {
    crate::PlannedFile {
        source: format!("mods/user/opti/{dest}"),
        dest: dest.into(),
        sha256: "aa".into(),
        enabled: true,
        load: None,
    }
}

async fn seeded() -> (
    std::path::PathBuf,
    std::path::PathBuf,
    sqlx::SqlitePool,
    String,
) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-file-load-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:fileload".to_string();
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: "fileload",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    (dir, data, pool, gid)
}

fn manifest(gid: &str, files: Vec<crate::PlannedFile>, include: &[&str]) -> crate::FileManifest {
    crate::FileManifest {
        game: gid.into(),
        instance: "opti".into(),
        mod_type: "custom".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: include.iter().map(|s| (*s).to_string()).collect(),
        files: files.into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

fn planned(dest: &str, load: Option<bool>) -> crate::PlannedFile {
    crate::PlannedFile {
        source: "s".into(),
        dest: dest.into(),
        sha256: "h".into(),
        enabled: true,
        load,
    }
}

#[test]
fn per_game_load_overrides_recipe_include() {
    let m = crate::FileManifest {
        game: "manual:standalone:inc2".into(),
        instance: "incmod".into(),
        mod_type: "custom".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: vec!["proxy.dll".into()].into_boxed_slice(),
        files: vec![
            planned("proxy.dll", Some(true)),
            planned("other.dll", Some(false)),
            planned("bin/extra.dll", Some(false)),
        ]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    let lines = crate::prewire::ini_lines_for(&m);
    assert!(
        lines.contains(&"LoadDLL=proxy.dll=incmod/proxy.dll".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"IncludeFile=other.dll=incmod/other.dll".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"IncludeFile=bin/=incmod/bin/".to_string()),
        "{lines:?}"
    );
    assert!(!lines.iter().any(|l| l.contains("extra.dll")), "{lines:?}");
    assert!(!crate::file_is_loaddll("proxy.dll", &m.include, None));
    assert!(crate::file_is_loaddll("other.dll", &m.include, None));
    assert!(!crate::file_mode_applicable(
        "pfx:windows/system32/d3dcompiler_47.dll"
    ));
    assert!(crate::file_mode_applicable("other.dll"));
    assert!(!crate::file_mode_applicable("note.txt"));
}

#[tokio::test]
async fn switch_moves_the_ini_line_and_roundtrips() {
    let (dir, data, pool, gid) = seeded().await;
    let m = manifest(&gid, vec![file("plug.dll"), file("note.txt")], &[]);
    crate::write_manifest(&data, &m).unwrap();
    crate::prewire_game(&data, &pool, &gid).await.unwrap();
    let ini = crate::managed_ini(&crate::game::game_dir(
        &data,
        &crate::game::GameId::parse(&gid).unwrap(),
    ));
    let before = fs::read_to_string(&ini).unwrap();
    assert!(
        before.contains("LoadDLL=plug.dll=opti/plug.dll"),
        "{before}"
    );

    let m = set_file_load(&pool, &data, &gid, "opti", "plug.dll", false)
        .await
        .unwrap();
    assert_eq!(
        m.files.iter().find(|f| f.dest == "plug.dll").unwrap().load,
        Some(false)
    );
    let after = fs::read_to_string(&ini).unwrap();
    assert!(
        after.contains("IncludeFile=plug.dll=opti/plug.dll"),
        "{after}"
    );
    assert!(!after.contains("LoadDLL=plug.dll="), "{after}");

    // Back to the recipe default clears the stored override.
    let m = set_file_load(&pool, &data, &gid, "opti", "plug.dll", true)
        .await
        .unwrap();
    assert!(m
        .files
        .iter()
        .find(|f| f.dest == "plug.dll")
        .unwrap()
        .load
        .is_none());
    let back = fs::read_to_string(&ini).unwrap();
    assert!(back.contains("LoadDLL=plug.dll=opti/plug.dll"), "{back}");

    let err = set_file_load(&pool, &data, &gid, "opti", "note.txt", true)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let err = set_file_load(&pool, &data, &gid, "opti", "nope.dll", true)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn include_default_forces_load_and_prefix_is_rejected() {
    let (dir, data, pool, gid) = seeded().await;
    let mut files = vec![file("proxy.dll")];
    files.push(crate::PlannedFile {
        source: "mods/user/opti/sys.dll".into(),
        dest: "pfx:windows/system32/sys.dll".into(),
        sha256: "aa".into(),
        enabled: true,
        load: None,
    });
    crate::write_manifest(&data, &manifest(&gid, files, &["proxy.dll"])).unwrap();
    crate::prewire_game(&data, &pool, &gid).await.unwrap();
    let m = set_file_load(&pool, &data, &gid, "opti", "proxy.dll", true)
        .await
        .unwrap();
    assert_eq!(
        m.files.iter().find(|f| f.dest == "proxy.dll").unwrap().load,
        Some(true)
    );
    let ini = crate::managed_ini(&crate::game::game_dir(
        &data,
        &crate::game::GameId::parse(&gid).unwrap(),
    ));
    let body = fs::read_to_string(&ini).unwrap();
    assert!(body.contains("LoadDLL=proxy.dll=opti/proxy.dll"), "{body}");
    let err = set_file_load(
        &pool,
        &data,
        &gid,
        "opti",
        "pfx:windows/system32/sys.dll",
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let _ = fs::remove_dir_all(&dir);
}
