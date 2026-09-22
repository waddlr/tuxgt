use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest};
use std::fs;

#[tokio::test]
async fn uninstall_drops_omitted_dest_ignores_other_omitted_keep() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-uninst-omit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "steam::1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("UninstOmit"),
            ..Default::default()
        },
    )
    .await;
    let rt = crate::stage::runtime_dir(&data, gid);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("kept.dll"), b"k").unwrap();
    fs::write(rt.join("omitted.fx"), b"o").unwrap();
    fs::write(rt.join("other.dll"), b"x").unwrap();
    let file = |dest: &str, enabled: bool| crate::PlannedFile {
        source: dest.into(),
        dest: dest.into(),
        sha256: "aa".into(),
        enabled,
    };
    crate::write_manifest(
        &data,
        &FileManifest {
            game: gid.into(),
            instance: "pack".into(),
            mod_type: "effect".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![file("kept.dll", true), file("omitted.fx", false)].into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
    crate::write_manifest(
        &data,
        &FileManifest {
            game: gid.into(),
            instance: "other".into(),
            mod_type: "custom".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![file("other.dll", true), file("omitted.fx", false)].into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
    uninstall_instance(&pool, &data, &data.join("config"), gid, "pack", true)
        .await
        .unwrap();
    assert!(!rt.join("kept.dll").exists());
    assert!(!rt.join("omitted.fx").exists());
    assert!(rt.join("other.dll").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_from_package_applies_dests_and_keeps_extras() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e49-mods-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pkg = dir.join("pkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(pkg.join("extra.ini"), b"extra").unwrap();
    crate::add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:e49test";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("E49"),
            ..Default::default()
        },
    )
    .await;
    let man = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "optiscaler-custom",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let dests: Vec<&str> = man.files.iter().map(|f| f.dest.as_str()).collect();
    assert!(dests.contains(&"dxgi.dll"), "{dests:?}");
    assert!(dests.contains(&"extra.ini"), "{dests:?}");
    assert!(!dests.contains(&"OptiScaler.dll"), "{dests:?}");
    fs::write(pkg.join("later.bin"), b"l").unwrap();
    crate::rescan_mod(&cfg, "optiscaler-custom", None, None, &data).unwrap();
    let back = crate::read_manifest(&data, gid, "optiscaler-custom")
        .unwrap()
        .unwrap();
    let dests: Vec<&str> = back.files.iter().map(|f| f.dest.as_str()).collect();
    assert!(!dests.contains(&"later.bin"), "{dests:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// E64: omit restages/repwires, keep restores, required dests refuse.
#[tokio::test]
async fn file_keep_toggles_staging_and_prewire() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e64-mods-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let gid = "manual:standalone:e64test";
    let pool = crate::open_db(&data).await.unwrap();
    let exe = data.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            manager: "manual",
            store: "standalone",
            game_id: "e64test",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // Payload tree the manifest refs point at; staging syncs from it.
    let depot = data.join("mods").join("user").join("opti");
    fs::create_dir_all(&depot).unwrap();
    let mut files = Vec::new();
    for dest in ["dxgi.dll", "OptiScaler.ini"] {
        let p = depot.join(dest);
        fs::write(&p, dest.as_bytes()).unwrap();
        files.push(crate::PlannedFile {
            source: format!("mods/user/opti/{dest}"),
            dest: dest.into(),
            sha256: crate::sha256_file(&p).unwrap(),
            enabled: true,
        });
    }
    let m = crate::FileManifest {
        game: gid.into(),
        instance: "opti".into(),
        mod_type: "optiscaler".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: files.into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    };
    let stage = crate::stage::stage_dir(&data, gid, "opti");
    let srcs: Vec<std::path::PathBuf> = m.files.iter().map(|f| depot.join(&f.dest)).collect();
    let staged: Vec<crate::StageInput> = m
        .files
        .iter()
        .zip(srcs.iter())
        .map(|(f, src)| crate::StageInput {
            rel: &f.dest,
            src,
            sha: &f.sha256,
        })
        .collect();
    crate::sync_staging(&data, gid, "opti", &staged, false).unwrap();
    crate::write_manifest(&data, &m).unwrap();
    let ini = crate::managed_ini(&crate::game::game_dir(
        &data,
        &crate::game::GameId::parse(gid).unwrap(),
    ));
    crate::prewire_game(&data, &pool, gid).await.unwrap();
    let before = fs::read_to_string(&ini).unwrap();
    assert!(
        before.contains("LoadDLL=dxgi.dll=opti/dxgi.dll"),
        "{before}"
    );
    assert!(
        before.contains("IncludeFile=OptiScaler.ini=opti/OptiScaler.ini"),
        "{before}"
    );

    // Omit the optional companion: manifest, staging, and ini all drop it.
    let m = set_file_keep(&pool, &data, gid, "opti", "OptiScaler.ini", false, true)
        .await
        .unwrap();
    assert!(
        !m.files
            .iter()
            .find(|f| f.dest == "OptiScaler.ini")
            .unwrap()
            .enabled
    );
    assert!(!stage.join("OptiScaler.ini").is_file());
    let after = fs::read_to_string(&ini).unwrap();
    assert!(!after.contains("OptiScaler.ini"), "{after}");
    assert!(after.contains("LoadDLL=dxgi.dll=opti/dxgi.dll"), "{after}");
    assert!(crate::stage_status(&data, gid)
        .unwrap()
        .iter()
        .all(|l| l.state == crate::StageState::InSync));

    // Keep it again: restaged from the depot and listed again.
    set_file_keep(&pool, &data, gid, "opti", "OptiScaler.ini", true, true)
        .await
        .unwrap();
    assert_eq!(
        fs::read(stage.join("OptiScaler.ini")).unwrap(),
        b"OptiScaler.ini"
    );
    let back = fs::read_to_string(&ini).unwrap();
    assert!(
        back.contains("IncludeFile=OptiScaler.ini=opti/OptiScaler.ini"),
        "{back}"
    );

    // The optiscaler slot dest is required and refuses to be omitted.
    let err = set_file_keep(&pool, &data, gid, "opti", "dxgi.dll", false, true)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::InvalidInstance(m) if m.contains("required")),
        "{err}"
    );
    // Unknown dests error too.
    assert!(
        set_file_keep(&pool, &data, gid, "opti", "nope.dll", false, true)
            .await
            .is_err()
    );
    let _ = fs::remove_dir_all(&dir);
}
