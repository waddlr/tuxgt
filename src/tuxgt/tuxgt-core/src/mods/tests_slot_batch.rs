//! A card of slot picks is one write: a chain applies in an order that frees
//! each stem, a shared stem writes nothing, and a later error puts the first
//! dest back.
use super::*;
use crate::game::{set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::instance::{payload_dir, user_mods_dir};
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest, PlannedFile};
use sqlx::SqlitePool;
use std::fs;
use std::path::PathBuf;

struct Fx {
    dir: PathBuf,
    data: PathBuf,
    cfg: PathBuf,
    root: PathBuf,
    pool: SqlitePool,
    gid: String,
}

async fn setup(tag: &str) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-slot-batch-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let root = dir.join("gamedir");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&root).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:batch{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("batch{tag}"),
            name: Some("Batch"),
            exe_path: Some(exe.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            install_dir: Some(root.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    set_game_adapter(&pool, &gid, ADAPTER_INSTALL)
        .await
        .unwrap();
    Fx {
        dir,
        data,
        cfg,
        root,
        pool,
        gid,
    }
}

fn recipe(fx: &Fx, id: &str, mod_type: &str, dll: &str, bytes: &[u8]) {
    let pkg = fx.dir.join(format!("pkg-{id}"));
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join(dll), bytes).unwrap();
    fs::create_dir_all(user_mods_dir(&fx.data)).unwrap();
    fs::write(
        user_mods_dir(&fx.data).join(format!("{id}.toml")),
        format!(
            "id = \"{id}\"\ntype = \"{mod_type}\"\nlabel = \"{id}\"\nslot = \"dxgi\"\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            pkg.to_str().unwrap()
        ),
    )
    .unwrap();
}

async fn install(fx: &Fx, id: &str, slot: &str) {
    install_instance(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        id,
        &InstallOpts {
            yes: true,
            slot: Some(slot.to_string()),
            ..InstallOpts::default()
        },
        None,
    )
    .await
    .unwrap();
}

fn dest(fx: &Fx, id: &str) -> String {
    crate::need_manifest(&fx.data, &fx.gid, id).unwrap().files[0]
        .dest
        .clone()
}

#[tokio::test]
async fn swap_applies_in_either_row_order() {
    let fx = setup("swap").await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    // shade is listed first and wants the stem opti still holds.
    apply_slot_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        &[("shade", "d3d11"), ("opti", "winmm")],
        true,
    )
    .await
    .unwrap();
    assert_eq!(dest(&fx, "shade"), "d3d11.dll");
    assert_eq!(dest(&fx, "opti"), "winmm.dll");
    assert_eq!(fs::read(fx.root.join("d3d11.dll")).unwrap(), b"shade");
    assert_eq!(fs::read(fx.root.join("winmm.dll")).unwrap(), b"opti");
    assert!(!fx.root.join("dxgi.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn shared_stem_writes_nothing() {
    let fx = setup("clash").await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    let err = apply_slot_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        &[("shade", "d3d12"), ("opti", "d3d12")],
        true,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::SlotInUse { .. }), "{err}");
    assert_eq!(dest(&fx, "shade"), "dxgi.dll");
    assert_eq!(dest(&fx, "opti"), "d3d11.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    assert!(!fx.root.join("d3d12.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn mid_apply_failure_restores_the_first_dest() {
    let fx = setup("mid").await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    // winmm.dll is a foreign file, so the second pick asks before it writes.
    fs::write(fx.root.join("winmm.dll"), b"foreign").unwrap();
    let err = apply_slot_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        &[("shade", "d3d12"), ("opti", "winmm")],
        false,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::NeedConfirm(_)), "{err}");
    assert_eq!(dest(&fx, "shade"), "dxgi.dll");
    assert_eq!(dest(&fx, "opti"), "d3d11.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    assert!(!fx.root.join("d3d12.dll").exists());
    assert_eq!(fs::read(fx.root.join("winmm.dll")).unwrap(), b"foreign");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

fn proxied(fx: &Fx) {
    let payload = payload_dir(&fx.data, false, None, "shade");
    fs::create_dir_all(&payload).unwrap();
    fs::write(payload.join("ReShade64.dll"), b"shade").unwrap();
    fs::create_dir_all(user_mods_dir(&fx.data)).unwrap();
    let pkg = fx.dir.join("pkg-shade");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("ReShade64.dll"), b"shade").unwrap();
    fs::write(
        user_mods_dir(&fx.data).join("shade.toml"),
        format!(
            "id = \"shade\"\ntype = \"reshade\"\nlabel = \"shade\"\nslot = \"dxgi\"\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            pkg.to_str().unwrap()
        ),
    )
    .unwrap();
    let sha = crate::sha256_file(&payload.join("ReShade64.dll")).unwrap();
    let m = FileManifest {
        game: fx.gid.clone(),
        instance: "shade".into(),
        mod_type: "reshade".into(),
        adapter: ADAPTER_INSTALL.into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![PlannedFile {
            source: "mods/user/shade/ReShade64.dll".into(),
            dest: "dxgi.dll".into(),
            sha256: sha.clone(),
            enabled: true,
            load: None,
        }]
        .into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: vec!["ReShade.ini".into()].into_boxed_slice(),
        harvested: Default::default(),
        provenance: Default::default(),
    };
    let staged = [crate::StageInput {
        rel: "dxgi.dll",
        src: &payload.join("ReShade64.dll"),
        sha: &sha,
    }];
    crate::sync_staging(&fx.data, &fx.gid, "shade", "reshade", &staged, false).unwrap();
    crate::write_manifest(&fx.data, &m).unwrap();
    fs::copy(
        crate::stage_dir(&fx.data, &fx.gid, "shade").join("dxgi.dll"),
        fx.root.join("dxgi.dll"),
    )
    .unwrap();
}

#[tokio::test]
async fn unplace_waits_for_overwrite_consent() {
    let fx = setup("ask").await;
    proxied(&fx);
    fs::write(fx.root.join("ReShade.ini"), b"live").unwrap();
    fs::create_dir_all(crate::runtime_dir(&fx.data, &fx.gid)).unwrap();
    fs::write(
        crate::runtime_dir(&fx.data, &fx.gid).join("ReShade.ini"),
        b"stale",
    )
    .unwrap();
    let err = convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_PRELOAD,
        false,
        true,
        &[("shade", "<self>")],
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, Error::NeedConfirm(msg) if msg.contains("ReShade.ini")),
        "{err}"
    );
    assert_eq!(dest(&fx, "shade"), "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_INSTALL
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_INSTALL).as_deref(),
        Some("dxgi")
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_PRELOAD).as_deref(),
        Some("<self>")
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn failed_convert_restores_the_proxy() {
    let fx = setup("roll").await;
    proxied(&fx);
    super::adapter::arm_fail(super::adapter::FailPoint::AfterFirstCopy);
    let err = convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_PRELOAD,
        true,
        true,
        &[("shade", "<self>")],
    )
    .await
    .unwrap_err();
    super::adapter::disarm_fail();
    assert!(matches!(err, Error::Install(_)), "{err}");
    assert_eq!(dest(&fx, "shade"), "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    let stage = crate::stage_dir(&fx.data, &fx.gid, "shade");
    assert!(stage.join("dxgi.dll").is_file(), "staging proxy restored");
    assert!(!stage.join("ReShade64.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_INSTALL
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_INSTALL).as_deref(),
        Some("dxgi")
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_PRELOAD).as_deref(),
        Some("<self>")
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn successful_unplace_lands_the_stock_name() {
    let fx = setup("stock").await;
    proxied(&fx);
    convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_PRELOAD,
        true,
        true,
        &[("shade", "<self>")],
    )
    .await
    .unwrap();
    assert_eq!(dest(&fx, "shade"), "ReShade64.dll");
    assert!(!fx.root.join("dxgi.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    assert_eq!(
        crate::game::game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_PRELOAD
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_PRELOAD).as_deref(),
        Some("<self>")
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_INSTALL).as_deref(),
        Some("dxgi")
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
