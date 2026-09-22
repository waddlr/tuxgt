//! Restore after a partial slot write, and conversion preflight before any stop.
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

async fn setup(tag: &str, adapter: &str) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-slot-restore-{tag}-{}-{}",
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
    let gid = format!("manual:standalone:restore{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("restore{tag}"),
            name: Some("Restore"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    set_game_adapter(&pool, &gid, adapter).await.unwrap();
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
async fn clobber_restore_puts_the_first_row_back() {
    let fx = setup("clobber", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    let mut opti = crate::read_manifest(&fx.data, &fx.gid, "opti")
        .unwrap()
        .unwrap();
    opti.files[0].dest = "dxgi.dll".into();
    crate::write_manifest(&fx.data, &opti).unwrap();
    let _ = fs::remove_file(fx.root.join("d3d11.dll"));
    let shade_pre = remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_INSTALL);
    let opti_pre = remembered_slot(&fx.data, &fx.gid, "opti", ADAPTER_INSTALL);
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
    assert_eq!(dest(&fx, "opti"), "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    assert!(!fx.root.join("d3d12.dll").exists());
    assert_eq!(fs::read(fx.root.join("winmm.dll")).unwrap(), b"foreign");
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "shade", ADAPTER_INSTALL),
        shade_pre
    );
    assert_eq!(
        remembered_slot(&fx.data, &fx.gid, "opti", ADAPTER_INSTALL),
        opti_pre
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn swap_leaves_the_scratch_stem_alone() {
    let fx = setup("swap", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    fs::write(fx.root.join("d3d12.dll"), b"foreign").unwrap();
    apply_slot_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        &[("shade", "d3d11"), ("opti", "dxgi")],
        false,
    )
    .await
    .unwrap();
    assert_eq!(dest(&fx, "shade"), "d3d11.dll");
    assert_eq!(dest(&fx, "opti"), "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("d3d11.dll")).unwrap(), b"shade");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"opti");
    assert_eq!(fs::read(fx.root.join("d3d12.dll")).unwrap(), b"foreign");
    assert!(!fx.root.join("ReShade64.dll").exists());
    assert!(!fx.root.join("OptiScaler.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn preflight_asks_before_a_prospective_install_dest() {
    let fx = setup("ask", ADAPTER_PRELOAD).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    install(&fx, "shade", "<self>").await;
    fs::write(fx.root.join("dxgi.dll"), b"foreign").unwrap();
    let err = preflight_convert_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        ADAPTER_INSTALL,
        false,
        &[("shade", "dxgi")],
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, Error::NeedConfirm(msg) if msg.contains("dxgi")),
        "{err}"
    );
    assert_eq!(dest(&fx, "shade"), "ReShade64.dll");
    assert_eq!(
        game_adapter(&fx.pool, &fx.gid).await.unwrap(),
        ADAPTER_PRELOAD
    );
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"foreign");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn preflight_rejects_a_shared_stem() {
    let fx = setup("share", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", "dxgi").await;
    install(&fx, "opti", "d3d11").await;
    let err = preflight_convert_picks(
        &fx.pool,
        &fx.data,
        &fx.gid,
        ADAPTER_INSTALL,
        true,
        &[("shade", "winmm"), ("opti", "winmm")],
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::SlotInUse { .. }), "{err}");
    assert_eq!(dest(&fx, "shade"), "dxgi.dll");
    assert_eq!(dest(&fx, "opti"), "d3d11.dll");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
