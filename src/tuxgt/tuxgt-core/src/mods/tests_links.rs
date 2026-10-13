//! Install: Links end-to-end (install/enable/disable/keep/slot/uninstall)
//! plus Copy↔Links conversion, over a two-claimant contested dest.
use super::adapter::{arm_fail, convert_game_adapter, disarm_fail, FailPoint};
use super::*;
use crate::game::ADAPTER_INSTALL_LINKS;
use crate::testing::{seed_game, SeedGame};
use std::fs;

struct Fx {
    dir: std::path::PathBuf,
    data: std::path::PathBuf,
    cfg: std::path::PathBuf,
    root: std::path::PathBuf,
    pool: sqlx::SqlitePool,
    gid: String,
    dest: String,
}

fn stamp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "tuxgt-links-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn staged(fx: &Fx, instance: &str) -> std::path::PathBuf {
    crate::stage::stage_dir(&fx.data, &fx.gid, instance).join(&fx.dest)
}

/// Two enabled install claimants on one dest: `aaa` (load 0, AAA bytes)
/// and `bbb` (load 1, BBB bytes), applied in load order over FOREIGN bytes,
/// with catalog recipes so conversion's recipe gate passes.
async fn setup(tag: &str, adapter_a: &str, adapter_b: &str) -> Fx {
    setup_with_dest(tag, adapter_a, adapter_b, "dxgi.dll").await
}

async fn setup_with_dest(tag: &str, adapter_a: &str, adapter_b: &str, dest: &str) -> Fx {
    let dir = stamp(tag);
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let root = dir.join("gamedir");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&root).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:links{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("links{tag}"),
            name: Some("Links"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    // Persist the game-level adapter choice the manifests were installed
    // under, so conversion validates from a consistent state.
    crate::game::set_game_adapter(&pool, &gid, adapter_a)
        .await
        .unwrap();
    fs::write(root.join(dest), b"FOREIGN").unwrap();
    for (iid, bytes, adapter, load) in [
        ("aaa", b"AAA".as_slice(), adapter_a, 0),
        ("bbb", b"BBB".as_slice(), adapter_b, 1),
    ] {
        let pkg = dir.join(format!("pkg-{iid}"));
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join(dest), bytes).unwrap();
        crate::add_mod_from(
            &cfg,
            "custom",
            iid,
            &pkg,
            None,
            None,
            &data,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let depot = data.join("mods").join("user").join(iid);
        fs::create_dir_all(&depot).unwrap();
        fs::write(depot.join(dest), bytes).unwrap();
        let m = crate::FileManifest {
            game: gid.clone(),
            instance: iid.into(),
            mod_type: "reshade".into(),
            adapter: adapter.into(),
            enabled: true,
            load_order: load,
            include: Box::default(),
            files: vec![crate::PlannedFile {
                source: format!("mods/user/{iid}/{dest}"),
                dest: dest.into(),
                sha256: crate::sha256_file(&depot.join(dest)).unwrap(),
                enabled: true,
                load: None,
            }]
            .into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        };
        let dep_dll = depot.join(dest);
        let sha = m.files[0].sha256.clone();
        let staged_input = crate::StageInput {
            rel: dest,
            src: &dep_dll,
            sha: &sha,
        };
        crate::sync_staging(&data, &gid, iid, &m.mod_type, &[staged_input], false).unwrap();
        crate::write_manifest(&data, &m).unwrap();
    }
    // Apply in load order through the real install path, so backups and
    // conflict state match production instead of a hand-rolled layout.
    crate::set_load_order(&pool, &data, &gid, &["aaa".into(), "bbb".into()])
        .await
        .unwrap();
    Fx {
        dir,
        data,
        cfg,
        root,
        pool,
        gid,
        dest: dest.into(),
    }
}

fn dest(fx: &Fx) -> std::path::PathBuf {
    fx.root.join(&fx.dest)
}

#[tokio::test]
async fn uninstall_winner_on_links_promotes_loser_link() {
    let fx = setup("uwo", "install-links", "install-links").await;
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "bbb"));
    uninstall_instance(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "bbb", true)
        .await
        .unwrap();
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "aaa"));
    uninstall_instance(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "aaa", true)
        .await
        .unwrap();
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"FOREIGN");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_loser_on_links_preserves_winner_and_backup_chain() {
    let fx = setup("ulo", "install-links", "install-links").await;
    uninstall_instance(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "aaa", true)
        .await
        .unwrap();
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "bbb"));
    // The foreign backup record moved to the winner: the last removal
    // still restores the foreign original.
    uninstall_instance(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "bbb", true)
        .await
        .unwrap();
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"FOREIGN");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn disable_winner_on_links_promotes_loser() {
    let fx = setup("dwo", "install-links", "install-links").await;
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "bbb", false, true)
        .await
        .unwrap();
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "aaa"));
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn keep_off_winner_on_links_promotes_loser() {
    // A non-proxy dest is omittable (proxy DLLs are required dests and
    // refuse keep-off before any mutation).
    let fx = setup_with_dest("kwo", "install-links", "install-links", "shared.fx").await;
    set_file_keep(&fx.pool, &fx.data, &fx.gid, "bbb", "shared.fx", false, true)
        .await
        .unwrap();
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "aaa"));
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn slot_rename_promotes_loser_on_the_old_dest() {
    let fx = setup("sro", "install-links", "install-links").await;
    set_instance_slot(&fx.pool, &fx.data, &fx.gid, "bbb", "d3d11", true)
        .await
        .unwrap();
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "aaa"));
    assert_eq!(
        fs::read(fx.root.join("d3d11.dll")).unwrap(),
        b"BBB",
        "the renamed winner lands on its new dest"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

fn manifest_adapter(fx: &Fx, instance: &str) -> String {
    crate::need_manifest(&fx.data, &fx.gid, instance)
        .unwrap()
        .adapter
        .clone()
}

#[tokio::test]
async fn convert_copy_to_links_relinks_without_touching_backups() {
    let fx = setup("c2l", "install", "install").await;
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"BBB");
    convert_game_adapter(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        "install-links",
        true,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_INSTALL_LINKS
    );
    assert_eq!(manifest_adapter(&fx, "aaa"), ADAPTER_INSTALL_LINKS);
    assert_eq!(manifest_adapter(&fx, "bbb"), ADAPTER_INSTALL_LINKS);
    assert_eq!(fs::read_link(dest(&fx)).unwrap(), staged(&fx, "bbb"));
    // Backups survive the swap untouched: the foreign record stays with
    // the first claimant and no new backup is taken.
    let aaa = crate::need_manifest(&fx.data, &fx.gid, "aaa").unwrap();
    let bbb = crate::need_manifest(&fx.data, &fx.gid, "bbb").unwrap();
    assert!(aaa.backups.contains_key("dxgi.dll"));
    assert!(bbb.backups.is_empty());
    assert_eq!(
        fs::read(fx.data.join(&aaa.backups["dxgi.dll"])).unwrap(),
        b"FOREIGN"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn convert_links_to_copy_restores_files_without_touching_backups() {
    let fx = setup("l2c", "install-links", "install-links").await;
    convert_game_adapter(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "install", true, false)
        .await
        .unwrap();
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        "install"
    );
    assert!(fs::symlink_metadata(dest(&fx)).is_ok_and(|meta| !meta.file_type().is_symlink()));
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"BBB");
    let aaa = crate::need_manifest(&fx.data, &fx.gid, "aaa").unwrap();
    let bbb = crate::need_manifest(&fx.data, &fx.gid, "bbb").unwrap();
    assert!(aaa.backups.contains_key("dxgi.dll"));
    assert!(bbb.backups.is_empty());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn convert_links_to_preload_restores_the_foreign_original() {
    let fx = setup("l2p", "install-links", "install-links").await;
    // Leaving the game-dir root still needs the return-to-self pick
    // settled: slots_chosen replays without renaming.
    convert_game_adapter(&fx.pool, &fx.data, &fx.cfg, &fx.gid, "preload", true, true)
        .await
        .unwrap();
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        "preload"
    );
    // The first claimant's backup consolidates onto the winner before
    // removal, so the foreign original restores instead of vanishing.
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"FOREIGN");
    assert_eq!(manifest_adapter(&fx, "aaa"), "preload");
    assert_eq!(manifest_adapter(&fx, "bbb"), "preload");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn convert_copy_links_rollback_restores_kinds() {
    let fx = setup("rbk", "install", "install").await;
    arm_fail(FailPoint::AfterFirstCopy);
    let err = convert_game_adapter(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        "install-links",
        true,
        false,
    )
    .await
    .unwrap_err();
    disarm_fail();
    assert!(err.to_string().contains("after the first copy"), "{err}");
    // Rollback restores bytes and the persisted choice, not just bytes.
    assert_eq!(fs::read(dest(&fx)).unwrap(), b"BBB");
    assert!(fs::symlink_metadata(dest(&fx)).is_ok_and(|meta| !meta.file_type().is_symlink()));
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        "install"
    );
    assert_eq!(manifest_adapter(&fx, "aaa"), "install");
    let _ = fs::remove_dir_all(&fx.dir);
}
