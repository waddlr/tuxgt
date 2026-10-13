use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{disable_mod, include_covers};

#[test]
fn replacing_one_file_keeps_a_disabled_complete_mod_disabled() {
    let (dir, data, cfg) = scratch("dis");
    let all = dir.join("all");
    for name in STREAM {
        write_tree(&all, name, b"v1");
    }
    let report = provide_files(&cfg, &data, "nvidia-streamline", &all).unwrap();
    assert!(report.ready());
    assert!(report.enabled);
    disable_mod(&cfg, "nvidia-streamline", &data).unwrap();
    let again = dir.join("again");
    write_tree(&again, "sl.common.dll", b"v2");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &again).unwrap();
    assert!(report.ready());
    assert!(!report.enabled);
    let inst = find_mod(&cfg, &data, "nvidia-streamline").unwrap();
    assert!(!inst.enabled);
    assert!(provided_files(&data, &inst).unwrap().unwrap().ready);
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.common.dll")).unwrap(),
        b"v2"
    );
    assert_eq!(
        fs::read(payload(&data, "nvidia-streamline").join("sl.pcl.dll")).unwrap(),
        b"v1"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_waits_for_files_then_lands_without_download() {
    let (dir, data, cfg) = scratch("inst");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:provide";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let one = dir.join("one");
    write_tree(&one, "sl.interposer.dll", b"only");
    provide_files(&cfg, &data, "nvidia-streamline", &one).unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    // The typed park error is what the GUI matches to ask for files.
    let msg = err.to_string();
    assert!(matches!(err, crate::Error::NeedsProvidedFiles(_)), "{msg}");
    assert!(msg.contains("files not provided"), "{msg}");
    assert!(msg.contains("sl.common.dll"), "{msg}");
    assert!(crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .is_none());

    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr-bytes").unwrap();
    let report = provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    assert!(report.ready());
    let before = fs::read(payload(&data, "nvngx-dlssnr").join("nvngx_dlssnr.dll")).unwrap();
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvngx-dlssnr",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m
        .files
        .iter()
        .any(|f| { f.dest == "nvngx_dlssnr.dll" && include_covers(&m.include, &f.dest) }));
    assert_eq!(m.provenance.source, "provided");

    let mut opts = InstallOpts::default();
    opts.redownload = true;
    install_instance(&pool, &data, &cfg, gid, "nvngx-dlssnr", &opts, None)
        .await
        .unwrap();
    assert_eq!(
        fs::read(payload(&data, "nvngx-dlssnr").join("nvngx_dlssnr.dll")).unwrap(),
        before
    );

    crate::write_manifest(&data, &reshade_manifest(gid)).unwrap();
    for (id, ty) in [
        ("d3dcompiler-47", "custom"),
        ("nvidia-streamline", "custom"),
        ("nvngx-dlssnr-proxy", "custom"),
    ] {
        crate::write_manifest(&data, &dep_manifest(gid, id, ty)).unwrap();
    }
    let addon = dir.join("addon");
    write_tree(&addon, "renodx-dlss.addon64", b"addon");
    provide_files(&cfg, &data, "renodx-dlss", &addon).unwrap();
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "renodx-dlss",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(m.files.iter().any(|f| f.dest.ends_with(".addon64")));
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn catalog_check_is_offline() {
    let (dir, data, cfg) = scratch("cat");
    let pool = crate::open_db(&data).await.unwrap();
    match check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
        .await
        .unwrap()
    {
        CatalogStatus::NeedsFiles { detail } => {
            assert!(detail.contains("files not provided"), "{detail}");
            assert!(detail.contains("nvngx_dlssnr.dll"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    let status = check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
        .await
        .unwrap();
    assert_eq!(status, CatalogStatus::UpToDate);
    let row: (String,) =
        sqlx::query_as("SELECT update_status FROM mods_cache WHERE id = 'nvngx-dlssnr'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "uptodate");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn replaced_payload_stays_catalog_current() {
    let (dir, data, cfg) = scratch("cat-replaced");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:provide-replaced";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let dll = dir.join("nvngx_dlssnr.dll");
    fs::write(&dll, b"nr-v1").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvngx-dlssnr",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    fs::write(&dll, b"nr-v2").unwrap();
    provide_files(&cfg, &data, "nvngx-dlssnr", &dll).unwrap();
    assert_eq!(
        check_catalog_update(&pool, &data, &cfg, "nvngx-dlssnr")
            .await
            .unwrap(),
        CatalogStatus::UpToDate
    );
    assert_eq!(
        check_update(&data, &cfg, gid, "nvngx-dlssnr")
            .await
            .unwrap(),
        UpdateStatus::Available {
            detail: "files replaced".into(),
        }
    );
    clear_provided_files(&cfg, &data, "nvngx-dlssnr").unwrap();
    let baseline = ensure_update_baseline(&pool, &data, &cfg, gid, "nvngx-dlssnr")
        .await
        .unwrap();
    assert!(!baseline.installed);
    match baseline.status {
        UpdateStatus::Unknown { reason } => {
            assert!(reason.contains("files not provided"), "{reason}");
            assert_eq!(short_update_reason(&reason), "gui-mod-update-unknown-files");
        }
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn partial_reprovide_does_not_publish_a_digest() {
    let (dir, data, cfg) = scratch("part-digest");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:part-digest";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide"),
            ..Default::default()
        },
    )
    .await;
    let all = dir.join("all");
    for name in STREAM {
        write_tree(&all, name, b"v1");
    }
    provide_files(&cfg, &data, "nvidia-streamline", &all).unwrap();
    install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let installed = crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .unwrap();
    assert!(!installed.provenance.asset_sha256.is_empty());
    clear_provided_files(&cfg, &data, "nvidia-streamline").unwrap();
    let one = dir.join("one");
    write_tree(&one, "sl.pcl.dll", b"only");
    let report = provide_files(&cfg, &data, "nvidia-streamline", &one).unwrap();
    assert!(!report.ready());
    assert!(!payload(&data, "nvidia-streamline")
        .join(".provenance.toml")
        .exists());
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let rows = mod_cache_rows(&pool).await.unwrap();
    let row = rows
        .iter()
        .find(|r| r.id == "nvidia-streamline")
        .expect("cache row");
    assert!(row.asset_sha256.is_empty(), "{}", row.asset_sha256);
    let still = crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .unwrap();
    assert_eq!(
        still.provenance.asset_sha256,
        installed.provenance.asset_sha256
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A `--with-requires` dep that is itself not ready stops the nested
/// install naming the dep, not the target, and writes no manifest for
/// either — the GUI park parses that id to ask for the dep's files.
/// The dep is also disabled (the cleared-payload state): readiness is
/// reported before the disabled gate, direct and nested alike, so the
/// park can happen; completing the provide re-enables it.
#[tokio::test]
async fn nested_require_names_the_not_ready_dep() {
    let (dir, data, cfg) = scratch("nested");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:provide-nested";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("Provide Nested"),
            ..Default::default()
        },
    )
    .await;
    let pkg = dir.join("targetpkg");
    write_tree(&pkg, "target.dll", b"dll");
    let recipe = format!(
        "id = \"needs-streamline\"\ntype = \"custom\"\nlabel = \"needs streamline\"\nrequires = [\"nvidia-streamline\"]\n[source]\ntype = \"local\"\npath = \"{}\"\n",
        pkg.to_str().unwrap()
    );
    let src = dir.join("needs-streamline.toml");
    fs::write(&src, recipe).unwrap();
    crate::add_mod(&cfg, &src, &data).unwrap();
    crate::disable_mod(&cfg, "nvidia-streamline", &data).unwrap();
    // Direct install of the disabled not-ready dep parks too.
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, crate::Error::NeedsProvidedFiles(_)), "{err}");
    let opts = InstallOpts {
        with_requires: Some("nvidia-streamline".into()),
        ..InstallOpts::default()
    };
    let err = install_instance(&pool, &data, &cfg, gid, "needs-streamline", &opts, None)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::NeedsProvidedFiles(_)), "{err}");
    assert!(err.to_string().starts_with("nvidia-streamline:"), "{err}");
    assert!(crate::read_manifest(&data, gid, "nvidia-streamline")
        .unwrap()
        .is_none());
    assert!(crate::read_manifest(&data, gid, "needs-streamline")
        .unwrap()
        .is_none());

    // Ready and explicitly disabled again, the disabled gate wins.
    let all = dir.join("all");
    for name in STREAM {
        write_tree(&all, name, name.as_bytes());
    }
    provide_files(&cfg, &data, "nvidia-streamline", &all).unwrap();
    crate::disable_mod(&cfg, "nvidia-streamline", &data).unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "nvidia-streamline",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, crate::Error::InstanceDisabled(_)), "{err}");
    let _ = fs::remove_dir_all(&dir);
}
