use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use sqlx::SqlitePool;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn file_keep_install_adapter_confirms_before_mutating() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e64-confirm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let root = dir.join("gamedir");
    fs::create_dir_all(&root).unwrap();
    let gid = "manual:standalone:e64confirm";
    let pool = crate::open_db(&data).await.unwrap();
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            manager: "manual",
            store: "standalone",
            game_id: "e64confirm",
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // `dxgi.ini` sits on a protected stem, so its game-dir file is foreign.
    let foreign = b"foreign-dxgi-ini".to_vec();
    fs::write(root.join("dxgi.ini"), &foreign).unwrap();
    let depot = data.join("mods").join("official").join("reshade");
    fs::create_dir_all(&depot).unwrap();
    let mut files = Vec::new();
    for dest in ["ReShade64.dll", "dxgi.ini"] {
        let p = depot.join(dest);
        fs::write(&p, dest.as_bytes()).unwrap();
        files.push(crate::PlannedFile {
            source: format!("mods/official/reshade/{dest}"),
            dest: dest.into(),
            sha256: crate::sha256_file(&p).unwrap(),
            enabled: true,
        });
    }
    let m = crate::FileManifest {
        game: gid.into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
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
    let stage = crate::stage::stage_dir(&data, gid, "reshade");
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
    crate::sync_staging(&data, gid, "reshade", &staged, false).unwrap();
    crate::write_manifest(&data, &m).unwrap();

    // Omit first: allowed, and the staged copy goes away.
    set_file_keep(&pool, &data, gid, "reshade", "dxgi.ini", false, true)
        .await
        .unwrap();
    assert!(!stage.join("dxgi.ini").is_file());

    let man = crate::manifest_path(&data, gid, "reshade");
    let before = fs::read(&man).unwrap();
    let err = set_file_keep(&pool, &data, gid, "reshade", "dxgi.ini", true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::NeedConfirm(m) if m.contains("dxgi.ini")),
        "{err}"
    );
    assert_eq!(
        fs::read(&man).unwrap(),
        before,
        "refused enable rewrote the manifest"
    );
    assert!(!stage.join("dxgi.ini").is_file(), "refused enable restaged");
    assert_eq!(fs::read(root.join("dxgi.ini")).unwrap(), foreign);
    assert!(
        !crate::read_manifest(&data, gid, "reshade")
            .unwrap()
            .unwrap()
            .files
            .iter()
            .find(|f| f.dest == "dxgi.ini")
            .unwrap()
            .enabled
    );

    // Confirmed: restaged, copied over the foreign file, backed up.
    let m = set_file_keep(&pool, &data, gid, "reshade", "dxgi.ini", true, true)
        .await
        .unwrap();
    assert!(
        m.files
            .iter()
            .find(|f| f.dest == "dxgi.ini")
            .unwrap()
            .enabled
    );
    assert_eq!(fs::read(stage.join("dxgi.ini")).unwrap(), b"dxgi.ini");
    assert_eq!(fs::read(root.join("dxgi.ini")).unwrap(), b"dxgi.ini");
    let backed = crate::read_manifest(&data, gid, "reshade")
        .unwrap()
        .unwrap();
    let rel = backed.backups.get("dxgi.ini").expect("backup recorded");
    assert_eq!(fs::read(data.join(rel)).unwrap(), foreign);
    let _ = fs::remove_dir_all(&dir);
}
fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

async fn local_game(
    tag: &str,
    payload: &[u8],
) -> (PathBuf, PathBuf, PathBuf, SqlitePool, String, String) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-r32-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pkg = dir.join("pkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), payload).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:r32{tag}");
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            name: Some("R32"),
            ..Default::default()
        },
    )
    .await;
    let iid = format!("r32plug{tag}");
    crate::add_mod_from(
        &cfg,
        "custom",
        &iid,
        &pkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    (dir, data, cfg, pool, gid, iid)
}
async fn index_row(
    pool: &SqlitePool,
    id: &str,
) -> Option<(String, String, String, i64, Option<i64>, Option<String>)> {
    sqlx::query_as(
            "SELECT id, kind, asset_sha256, installed, last_check, update_status FROM mods_cache WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn second_game(pool: &SqlitePool, tag: &str) -> String {
    let gid = format!("manual:standalone:{tag}");
    seed_game(
        pool,
        SeedGame {
            id: &gid,
            name: Some("Second"),
            ..Default::default()
        },
    )
    .await;
    gid
}

#[tokio::test]
async fn index_install_uninstall_counts() {
    let (dir, data, cfg, pool, gid, iid) = local_game("idx", b"plug-v1").await;
    // Pre-install reconcile: recipe-only row, empty sha, zero installs.
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let row = index_row(&pool, &iid).await.expect("recipe row");
    assert_eq!(row.1, "user");
    assert!(row.2.is_empty(), "no payload sha before acquire");
    assert_eq!(row.3, 0);
    // Install → 1 with the payload sha recorded. T13: mutators no
    // longer touch the cache; the row refreshes at the next reconcile.
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let row = index_row(&pool, &iid).await.expect("installed row");
    assert_eq!(row.3, 1);
    assert!(!row.2.is_empty(), "payload sha after install");
    // Uninstall → 0, row kept (recipe still listed).
    uninstall_instance(&pool, &data, &cfg, &gid, &iid, true)
        .await
        .unwrap();
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let row = index_row(&pool, &iid).await.expect("kept row");
    assert_eq!(row.3, 0);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn index_recipe_only_official_empty_sha_gone_recipe_drops() {
    let (dir, data, cfg, _pool, _gid, _iid) = local_game("idxoff", b"plug-v1").await;
    crate::instance::seed_official_share_from_repo(&data);
    reconcile_mod_cache(&_pool, &data, &cfg).await.unwrap();
    let row = index_row(&_pool, "reshade").await.expect("official row");
    assert_eq!(row.1, "official");
    assert!(row.2.is_empty(), "recipe-only official has no sha");
    assert_eq!(row.3, 0);
    // A user recipe removed from disk drops its row on reconcile.
    let gone = _iid.clone();
    crate::instance::remove_mod(&cfg, &data, &gone).unwrap();
    reconcile_mod_cache(&_pool, &data, &cfg).await.unwrap();
    assert!(
        index_row(&_pool, &gone).await.is_none(),
        "gone recipe drops"
    );
    assert!(
        index_row(&_pool, "reshade").await.is_some(),
        "official stays"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn index_two_games_share_one_row_and_local_check_is_offline() {
    let (dir, data, cfg, pool, gid, iid) = local_game("idx2", b"plug-v1").await;
    let gid2 = second_game(&pool, "r32idx2b").await;
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    install_instance(
        &pool,
        &data,
        &cfg,
        &gid2,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    // One catalog row, two installs: no per-game GitHub fan-out at the
    // catalog layer (local sources resolve with zero network).
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    let row = index_row(&pool, &iid).await.expect("shared row");
    assert_eq!(row.3, 2, "two manifests share one catalog row");
    assert_eq!(
        check_catalog_update(&pool, &data, &cfg, &iid)
            .await
            .unwrap(),
        CatalogStatus::UpToDate
    );
    let row = index_row(&pool, &iid).await.expect("checked row");
    assert!(row.4.is_some(), "last_check recorded");
    assert_eq!(row.5.as_deref(), Some("uptodate"));
    // Upstream bytes move: the catalog check reports Available locally.
    fs::write(
        data.join("mods").join("user").join(&iid).join("plug.dll"),
        b"plug-v2-longer",
    )
    .unwrap();
    assert_eq!(
        check_catalog_update(&pool, &data, &cfg, &iid)
            .await
            .unwrap(),
        CatalogStatus::Available {
            detail: format!("{iid}: local source changed"),
        }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn index_github_url_moved_is_available_without_cache() {
    // R32 URL-moved signal at the catalog layer: provenance names the old
    // asset URL and the payload holds files, but the cache is empty
    // (steady state keeps downloads/ empty). A manual_url recipe whose
    // URL differs from the payload provenance source is Available with
    // zero network (the same arm serves github literal URLs).
    let (dir, data, cfg, pool, _gid, iid) = local_game("idxurl", b"plug-v1").await;
    let payload = data.join("mods").join("user").join(&iid);
    let prov = crate::ModProvenance {
        source: "https://example.com/old-asset.zip".into(),
        asset_sha256: crate::sha256_hex(b"plug-v1"),
        asset_bytes: 7,
        fetched_at: crate::download::now_unix(),
    };
    crate::instance::write_payload_provenance(&payload, &prov);
    // Replace the local recipe with a manual_url naming a new URL.
    let toml_path = data.join("mods").join("user").join(format!("{iid}.toml"));
    let text = format!(
            "id = \"{iid}\"\ntype = \"custom\"\nlabel = \"url-moved\"\n\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/new-asset.zip\"\n"
        );
    fs::write(&toml_path, text).unwrap();
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    match check_catalog_update(&pool, &data, &cfg, &iid)
        .await
        .unwrap()
    {
        CatalogStatus::Available { detail } => {
            assert!(detail.contains("URL moved"), "{detail}")
        }
        other => panic!("expected Available, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn index_empty_payload_is_unknown_never_available() {
    let (dir, data, cfg, pool, _gid, iid) = local_game("idxempty", b"plug-v1").await;
    // Never installed: no payload, no provenance → Unknown, not Available.
    reconcile_mod_cache(&pool, &data, &cfg).await.unwrap();
    assert_eq!(
        check_catalog_update(&pool, &data, &cfg, &iid)
            .await
            .unwrap(),
        CatalogStatus::Unknown {
            reason: format!("{iid}: never downloaded"),
        }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_records_provenance_and_update_tracks_local_source() {
    let (dir, data, cfg, pool, gid, iid) = local_game("a", b"plug-v1").await;
    let m = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(
        m.provenance.source.starts_with("local:"),
        "{}",
        m.provenance.source
    );
    let (v1_digest, _) =
        crate::download::local_source_digest(&data.join("mods").join("user").join(&iid)).unwrap();
    assert_eq!(m.provenance.asset_sha256, v1_digest);
    assert!(m.provenance.asset_bytes > 0);
    assert_eq!(
        check_update(&data, &cfg, &gid, &iid).await.unwrap(),
        UpdateStatus::UpToDate
    );
    // New bytes upstream: the check reports it without fetching.
    fs::write(
        data.join("mods").join("user").join(&iid).join("plug.dll"),
        b"plug-v2-longer",
    )
    .unwrap();
    match check_update(&data, &cfg, &gid, &iid).await.unwrap() {
        UpdateStatus::Available { detail } => {
            assert!(detail.contains("local source changed"), "{detail}")
        }
        other => panic!("expected Available, got {other:?}"),
    }
    // Reinstall refreshes provenance and restores UpToDate.
    let m2 = install_instance(
        &pool,
        &data,
        &cfg,
        &gid,
        &iid,
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    let (v2_digest, _) =
        crate::download::local_source_digest(&data.join("mods").join("user").join(&iid)).unwrap();
    assert_eq!(m2.provenance.asset_sha256, v2_digest);
    assert_eq!(
        check_update(&data, &cfg, &gid, &iid).await.unwrap(),
        UpdateStatus::UpToDate
    );
    let _ = fs::remove_dir_all(&dir);
}
