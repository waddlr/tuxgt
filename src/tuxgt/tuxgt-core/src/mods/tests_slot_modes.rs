//! A failed preload-to-install convert puts the preload files back and keeps
//! both modes' slot tokens.
use super::*;
use crate::game::{game_adapter, set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::instance::user_mods_dir;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
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
        "tuxgt-slot-modes-{tag}-{}-{}",
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
    let gid = format!("manual:standalone:modes{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("modes{tag}"),
            name: Some("Modes"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    set_game_adapter(&pool, &gid, ADAPTER_PRELOAD)
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

fn recipe(fx: &Fx) {
    let pkg = fx.dir.join("pkg-shade");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("ReShade64.dll"), b"shade").unwrap();
    fs::create_dir_all(user_mods_dir(&fx.data)).unwrap();
    fs::write(
        user_mods_dir(&fx.data).join("shade.toml"),
        format!(
            "id = \"shade\"\ntype = \"reshade\"\nlabel = \"shade\"\nslot = \"dxgi\"\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            pkg.to_str().unwrap()
        ),
    )
    .unwrap();
}

async fn install_stock(fx: &Fx) {
    recipe(fx);
    let m = install_instance(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        "shade",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(m.files[0].dest, "ReShade64.dll");
}

fn dest(fx: &Fx) -> String {
    crate::need_manifest(&fx.data, &fx.gid, "shade")
        .unwrap()
        .files[0]
        .dest
        .clone()
}

fn modes(fx: &Fx, adapter: &str) -> Option<String> {
    remembered_slot(&fx.data, &fx.gid, "shade", adapter)
}

#[tokio::test]
async fn failed_install_convert_keeps_the_preload_name() {
    let fx = setup("fail").await;
    install_stock(&fx).await;
    super::adapter::arm_fail(super::adapter::FailPoint::AfterFirstCopy);
    let err = convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        true,
        &[("shade", "dxgi")],
    )
    .await
    .unwrap_err();
    super::adapter::disarm_fail();
    assert!(matches!(err, Error::Install(_)), "{err}");
    assert_eq!(dest(&fx), "ReShade64.dll");
    assert_eq!(
        game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_PRELOAD
    );
    let stage = crate::stage_dir(&fx.data, &fx.gid, "shade");
    assert!(
        stage.join("ReShade64.dll").is_file(),
        "staging stock restored"
    );
    assert!(!stage.join("dxgi.dll").exists());
    assert!(!fx.root.join("dxgi.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    assert_eq!(modes(&fx, ADAPTER_PRELOAD).as_deref(), Some("<self>"));
    assert_eq!(modes(&fx, ADAPTER_INSTALL).as_deref(), Some("dxgi"));
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn install_consent_rolls_the_preload_name_back() {
    let fx = setup("ask").await;
    install_stock(&fx).await;
    fs::write(fx.root.join("dxgi.dll"), b"foreign").unwrap();
    let err = convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        false,
        true,
        &[("shade", "dxgi")],
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, Error::NeedConfirm(msg) if msg.contains("dxgi")),
        "{err}"
    );
    assert_eq!(dest(&fx), "ReShade64.dll");
    assert_eq!(
        game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_PRELOAD
    );
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"foreign");
    let stage = crate::stage_dir(&fx.data, &fx.gid, "shade");
    assert!(stage.join("ReShade64.dll").is_file());
    assert!(!stage.join("dxgi.dll").exists());
    assert_eq!(modes(&fx, ADAPTER_PRELOAD).as_deref(), Some("<self>"));
    assert_eq!(modes(&fx, ADAPTER_INSTALL).as_deref(), Some("dxgi"));
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn failed_convert_keeps_a_preload_proxy() {
    let fx = setup("proxy").await;
    install_stock(&fx).await;
    set_instance_slot(&fx.pool, &fx.data, &fx.gid, "shade", "d3d11", false)
        .await
        .unwrap();
    assert_eq!(dest(&fx), "d3d11.dll");
    super::adapter::arm_fail(super::adapter::FailPoint::AfterFirstCopy);
    let err = convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        true,
        &[("shade", "dxgi")],
    )
    .await
    .unwrap_err();
    super::adapter::disarm_fail();
    assert!(matches!(err, Error::Install(_)), "{err}");
    assert_eq!(dest(&fx), "d3d11.dll");
    assert_eq!(
        game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_PRELOAD
    );
    let stage = crate::stage_dir(&fx.data, &fx.gid, "shade");
    assert!(stage.join("d3d11.dll").is_file());
    assert!(!stage.join("dxgi.dll").exists());
    assert!(!fx.root.join("dxgi.dll").exists());
    assert_eq!(modes(&fx, ADAPTER_PRELOAD).as_deref(), Some("d3d11"));
    assert_eq!(modes(&fx, ADAPTER_INSTALL).as_deref(), Some("dxgi"));
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn successful_install_keeps_the_preload_token() {
    let fx = setup("ok").await;
    install_stock(&fx).await;
    convert_after_unplace(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        true,
        &[("shade", "dxgi")],
    )
    .await
    .unwrap();
    assert_eq!(dest(&fx), "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    assert_eq!(
        game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_INSTALL
    );
    assert_eq!(modes(&fx, ADAPTER_PRELOAD).as_deref(), Some("<self>"));
    assert_eq!(modes(&fx, ADAPTER_INSTALL).as_deref(), Some("dxgi"));
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
