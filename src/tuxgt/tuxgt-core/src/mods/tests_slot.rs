//! `<self>` keeps the stock DLL name. A proxy stem another enabled mod
//! already holds is `SlotInUse`. Preload installs do not collide.
use super::*;
use crate::game::{set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::instance::user_mods_dir;
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

async fn setup(tag: &str, adapter: &str) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-slot-{tag}-{}-{}",
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
    let gid = format!("manual:standalone:slot{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("slot{tag}"),
            name: Some("Slot"),
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

async fn install(fx: &Fx, id: &str, slot: Option<&str>) -> crate::Result<FileManifest> {
    install_instance(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        id,
        &InstallOpts {
            yes: true,
            slot: slot.map(str::to_string),
            ..InstallOpts::default()
        },
        None,
    )
    .await
}

#[tokio::test]
async fn fresh_install_asks_before_files_and_self_lands_stock() {
    let fx = setup("ask", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    let err = install(&fx, "shade", None).await.unwrap_err();
    assert!(
        matches!(&err, Error::NeedSlotChoice(msg) if msg.contains("shade")),
        "{err}"
    );
    assert!(crate::read_manifest(&fx.data, &fx.gid, "shade")
        .unwrap()
        .is_none());
    assert!(!fx.root.join("dxgi.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    let m = install(&fx, "shade", Some("<self>")).await.unwrap();
    assert_eq!(m.files[0].dest, "ReShade64.dll");
    assert_eq!(fs::read(fx.root.join("ReShade64.dll")).unwrap(), b"shade");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn preload_installs_keep_stock_names() {
    let fx = setup("pre", ADAPTER_PRELOAD).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    let shade = install(&fx, "shade", None).await.unwrap();
    let opti = install(&fx, "opti", None).await.unwrap();
    assert_eq!(shade.files[0].dest, "ReShade64.dll");
    assert_eq!(opti.files[0].dest, "OptiScaler.dll");
    assert!(resync_repick_instances(&fx.data, &fx.cfg, &fx.gid)
        .unwrap()
        .is_empty());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// The installed-card dropdown calls this with `yes = false`. A proxy pick
/// has to come back to the stock name: staging, the manifest, and the
/// loader line.
#[tokio::test]
async fn preload_proxy_returns_to_self() {
    let fx = setup("back", ADAPTER_PRELOAD).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    let installed = install(&fx, "shade", None).await.unwrap();
    assert_eq!(installed.files[0].dest, "ReShade64.dll");
    let dxgi = set_instance_slot(&fx.pool, &fx.data, &fx.gid, "shade", "dxgi", false)
        .await
        .unwrap();
    assert_eq!(dxgi.files[0].dest, "dxgi.dll");
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, "shade");
    assert!(stage.join("dxgi.dll").is_file());
    let back = set_instance_slot(&fx.pool, &fx.data, &fx.gid, "shade", "<self>", false)
        .await
        .unwrap();
    assert_eq!(back.files[0].dest, "ReShade64.dll");
    assert!(back.files[0].source.ends_with("ReShade64.dll"));
    assert!(stage.join("ReShade64.dll").is_file());
    assert!(!stage.join("dxgi.dll").exists());
    assert_eq!(fs::read(stage.join("ReShade64.dll")).unwrap(), b"shade");
    let lines = crate::prewire::ini_lines_for(&back);
    assert!(
        lines
            .iter()
            .any(|l| l == "LoadDLL=ReShade64.dll=shade/ReShade64.dll"),
        "{lines:?}"
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn second_proxy_names_the_holder_until_self() {
    let fx = setup("hold", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", Some("dxgi")).await.unwrap();
    let err = install(&fx, "opti", Some("dxgi")).await.unwrap_err();
    assert!(
        matches!(
            &err,
            Error::SlotInUse { instance, slot, holder }
                if instance == "opti" && slot == "dxgi" && holder == "shade"
        ),
        "{err}"
    );
    assert!(!fx.root.join("OptiScaler.dll").exists());
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    let opti = install(&fx, "opti", Some("<self>")).await.unwrap();
    assert_eq!(opti.files[0].dest, "OptiScaler.dll");
    assert_eq!(fs::read(fx.root.join("OptiScaler.dll")).unwrap(), b"opti");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"shade");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn disabled_holder_does_not_block_the_slot() {
    let fx = setup("off", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    install(&fx, "shade", Some("dxgi")).await.unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "shade", false, true)
        .await
        .unwrap();
    let opti = install(&fx, "opti", Some("dxgi")).await.unwrap();
    assert_eq!(opti.files[0].dest, "dxgi.dll");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

fn extra(fx: &Fx, id: &str, name: &str, bytes: &[u8]) {
    fs::write(fx.dir.join(format!("pkg-{id}")).join(name), bytes).unwrap();
}

#[tokio::test]
async fn optiscaler_companion_takes_the_picked_slot() {
    let fx = setup("comp", ADAPTER_INSTALL).await;
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    extra(&fx, "opti", "amd_fidelityfx_dx12.dll", b"ffx");
    let err = install(&fx, "opti", None).await.unwrap_err();
    assert!(matches!(err, Error::NeedSlotChoice(_)), "{err}");
    let m = install(&fx, "opti", Some("dxgi")).await.unwrap();
    let dest = |name: &str| {
        m.files
            .iter()
            .find(|f| f.source.ends_with(name))
            .unwrap()
            .dest
            .as_str()
    };
    assert_eq!(dest("OptiScaler.dll"), "dxgi.dll");
    assert_eq!(dest("amd_fidelityfx_dx12.dll"), "amd_fidelityfx_dx12.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"opti");
    assert_eq!(
        fs::read(fx.root.join("amd_fidelityfx_dx12.dll")).unwrap(),
        b"ffx"
    );
    let moved = set_instance_slot(&fx.pool, &fx.data, &fx.gid, "opti", "<self>", true)
        .await
        .unwrap();
    assert!(moved
        .files
        .iter()
        .any(|f| f.source.ends_with("OptiScaler.dll") && f.dest == "OptiScaler.dll"));
    assert!(!fx.root.join("dxgi.dll").exists());
    assert_eq!(fs::read(fx.root.join("OptiScaler.dll")).unwrap(), b"opti");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn two_reshade_injectors_refuse_the_slot() {
    let fx = setup("both", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"64");
    extra(&fx, "shade", "ReShade32.dll", b"32");
    let err = install(&fx, "shade", Some("dxgi")).await.unwrap_err();
    assert!(err.to_string().contains("no single injector"), "{err}");
    assert!(!fx.root.join("dxgi.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn disabled_slot_change_does_not_copy_the_dll_back() {
    let fx = setup("dis", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    install(&fx, "shade", Some("dxgi")).await.unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "shade", false, true)
        .await
        .unwrap();
    assert!(!fx.root.join("dxgi.dll").exists());
    let m = set_instance_slot(&fx.pool, &fx.data, &fx.gid, "shade", "<self>", true)
        .await
        .unwrap();
    assert_eq!(m.files[0].dest, "ReShade64.dll");
    assert!(!fx.root.join("ReShade64.dll").exists());
    assert!(!fx.root.join("dxgi.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

#[tokio::test]
async fn unplace_removes_the_proxy_without_dropping_the_stock_name() {
    let fx = setup("unpl", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    install(&fx, "shade", Some("dxgi")).await.unwrap();
    let m = unplace_instance_slot(&fx.pool, &fx.data, &fx.gid, "shade", "<self>", false)
        .await
        .unwrap();
    assert_eq!(m.files[0].dest, "ReShade64.dll");
    assert!(!fx.root.join("dxgi.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

fn parked(fx: &Fx, id: &str, mod_type: &str, dest: &str, enabled: bool) {
    let m = FileManifest {
        game: fx.gid.clone(),
        instance: id.into(),
        mod_type: mod_type.into(),
        adapter: ADAPTER_INSTALL.into(),
        enabled,
        load_order: 0,
        files: vec![PlannedFile {
            source: format!("mods/user/{id}/{dest}"),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
            load: None,
        }]
        .into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        include: Box::default(),
        harvested: Default::default(),
        provenance: Default::default(),
    };
    crate::write_manifest(&fx.data, &m).unwrap();
}

#[tokio::test]
async fn resync_repick_lists_only_a_shared_proxy() {
    let fx = setup("repick", ADAPTER_INSTALL).await;
    recipe(&fx, "shade", "reshade", "ReShade64.dll", b"shade");
    recipe(&fx, "opti", "optiscaler", "OptiScaler.dll", b"opti");
    parked(&fx, "shade", "reshade", "dxgi.dll", true);
    parked(&fx, "opti", "optiscaler", "dxgi.dll", true);
    let mut ids = resync_repick_instances(&fx.data, &fx.cfg, &fx.gid).unwrap();
    ids.sort();
    assert_eq!(ids, ["opti", "shade"]);
    parked(&fx, "shade", "reshade", "dxgi.dll", false);
    assert!(resync_repick_instances(&fx.data, &fx.cfg, &fx.gid)
        .unwrap()
        .is_empty());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
