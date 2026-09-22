//! Slot choice: a fresh Install install and a conversion ask before any
//! byte moves. `<self>` keeps the stock DLL name. A proxy stem is applied
//! by `set_instance_slot` before the conversion replays with `slots_chosen`.
use super::adapter::{arm_fail, disarm_fail, FailPoint};
use super::*;
use crate::game::{game_adapter, set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::instance::{payload_dir, user_mods_dir};
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest, PlannedFile};
use sqlx::SqlitePool;
use std::fs;
use std::path::PathBuf;

const IID: &str = "r9shade";
const STOCK: &[u8] = b"reshade-bytes";

struct Fx {
    dir: PathBuf,
    data: PathBuf,
    cfg: PathBuf,
    root: PathBuf,
    pool: SqlitePool,
    gid: String,
}

impl Fx {
    fn manifest(&self) -> FileManifest {
        crate::need_manifest(&self.data, &self.gid, IID).unwrap()
    }
}

/// A game with a hand-written user ReShade recipe (`slot` when `Some`) and
/// its payload staged under `mods/user/`. `manifest` writes a stock-named
/// preload manifest plus staging, mirroring a preload-side install.
async fn setup(tag: &str, slot: Option<&str>, manifest: bool) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-r9-{tag}-{}-{}",
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
    let gid = format!("manual:standalone:r9{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("r9{tag}"),
            name: Some("R9"),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: Some(root.to_str().unwrap()),
            detected_api: Some("dx12"),
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    let pkg = dir.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("ReShade64.dll"), STOCK).unwrap();
    let slot_line = slot
        .map(|s| format!("slot = \"{s}\"\n"))
        .unwrap_or_default();
    fs::create_dir_all(user_mods_dir(&data)).unwrap();
    fs::write(
        user_mods_dir(&data).join(format!("{IID}.toml")),
        format!(
            "id = \"{IID}\"\ntype = \"reshade\"\nlabel = \"R9 Shade\"\n{slot_line}[source]\ntype = \"local\"\npath = \"{}\"\n",
            pkg.to_str().unwrap(),
        ),
    )
    .unwrap();
    let payload = payload_dir(&data, false, None, IID);
    fs::create_dir_all(&payload).unwrap();
    fs::write(payload.join("ReShade64.dll"), STOCK).unwrap();
    if manifest {
        let sha = crate::sha256_file(&payload.join("ReShade64.dll")).unwrap();
        let m = FileManifest {
            game: gid.clone(),
            instance: IID.into(),
            mod_type: "reshade".into(),
            adapter: ADAPTER_PRELOAD.into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![PlannedFile {
                source: format!("mods/user/{IID}/ReShade64.dll"),
                dest: "ReShade64.dll".into(),
                sha256: sha.clone(),
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
        let staged = [crate::StageInput {
            rel: "ReShade64.dll",
            src: &payload.join("ReShade64.dll"),
            sha: &sha,
        }];
        crate::sync_staging(&data, &gid, IID, &m.mod_type, &staged, false).unwrap();
        crate::write_manifest(&data, &m).unwrap();
        set_game_adapter(&pool, &gid, ADAPTER_PRELOAD)
            .await
            .unwrap();
    }
    Fx {
        dir,
        data,
        cfg,
        root,
        pool,
        gid,
    }
}

async fn convert(
    fx: &Fx,
    target: &str,
    yes: bool,
    slots_chosen: bool,
) -> crate::Result<ConversionReport> {
    convert_game_adapter(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        target,
        yes,
        slots_chosen,
    )
    .await
}

async fn pick(fx: &Fx, slot: &str) {
    set_instance_slot(&fx.pool, &fx.data, &fx.gid, IID, slot, true)
        .await
        .unwrap();
}

/// Preload→install asks before moving a byte. The replay after the pick
/// copies the chosen proxy name.
#[tokio::test]
async fn convert_preload_to_install_asks_then_lands_the_pick() {
    let fx = setup("fwd", Some("dxgi"), true).await;
    let before = fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap();
    let err = convert(&fx, ADAPTER_INSTALL, true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::NeedSlotChoice(msg) if msg.contains(IID)),
        "{err}"
    );
    assert_eq!(
        fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap(),
        before
    );
    assert!(!fx.root.join("dxgi.dll").exists());
    pick(&fx, "dxgi").await;
    let r = convert(&fx, ADAPTER_INSTALL, true, true).await.unwrap();
    assert_eq!(r.instances, vec![IID.to_string()]);
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().files[0].dest, "dxgi.dll");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), STOCK);
    assert!(!fx.root.join("ReShade64.dll").exists());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// The way back asks, because the dest is a proxy name. Keeping that name
/// (`slots_chosen` without a rename) leaves `LoadDLL` on `dxgi.dll`.
#[tokio::test]
async fn convert_install_to_preload_can_keep_the_proxy_name() {
    let fx = setup("back", Some("dxgi"), true).await;
    assert!(convert(&fx, ADAPTER_INSTALL, true, false).await.is_err());
    pick(&fx, "dxgi").await;
    convert(&fx, ADAPTER_INSTALL, true, true).await.unwrap();
    let err = convert(&fx, ADAPTER_PRELOAD, true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::NeedSlotChoice(msg) if msg.contains(IID)),
        "{err}"
    );
    assert_eq!(fx.manifest().files[0].dest, "dxgi.dll");
    assert!(fx.root.join("dxgi.dll").is_file());
    convert(&fx, ADAPTER_PRELOAD, true, true).await.unwrap();
    assert_eq!(fx.manifest().adapter, "preload");
    assert_eq!(fx.manifest().files[0].dest, "dxgi.dll");
    assert!(!fx.root.join("dxgi.dll").exists());
    let gid = crate::game::GameId::parse(&fx.gid).unwrap();
    let ini = fs::read_to_string(crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data, &gid,
    )))
    .unwrap();
    assert!(ini.contains("LoadDLL=dxgi.dll="), "{ini}");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Install→preload defaults the offer to `<self>`. Applying that pick
/// lands the stock name.
#[tokio::test]
async fn convert_install_to_preload_rename_back_to_self() {
    let fx = setup("backself", Some("dxgi"), true).await;
    assert!(convert(&fx, ADAPTER_INSTALL, true, false).await.is_err());
    pick(&fx, "dxgi").await;
    convert(&fx, ADAPTER_INSTALL, true, true).await.unwrap();
    pick(&fx, "<self>").await;
    convert(&fx, ADAPTER_PRELOAD, true, true).await.unwrap();
    assert_eq!(fx.manifest().files[0].dest, "ReShade64.dll");
    assert!(!fx.root.join("dxgi.dll").exists());
    assert!(!fx.root.join("ReShade64.dll").exists());
    let gid = crate::game::GameId::parse(&fx.gid).unwrap();
    let ini = fs::read_to_string(crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data, &gid,
    )))
    .unwrap();
    assert!(ini.contains("LoadDLL=ReShade64.dll="), "{ini}");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A recipe slot does not skip the ask. The conversion refuses before
/// moving a byte.
#[tokio::test]
async fn convert_to_install_asks_even_when_the_recipe_names_a_slot() {
    let fx = setup("unsure", Some("dxgi"), true).await;
    let before = fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap();
    let err = convert(&fx, ADAPTER_INSTALL, true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::NeedSlotChoice(msg) if msg.contains(IID)),
        "{err}"
    );
    assert_eq!(
        fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap(),
        before
    );
    assert!(!fx.root.join("dxgi.dll").exists());
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A slotless recipe on a fresh Install-adapter install parks until the
/// caller supplies the proxy name.
#[tokio::test]
async fn install_unsure_parks_until_slot_passed() {
    let fx = setup("fresh-unsure", None, false).await;
    set_game_adapter(&fx.pool, &fx.gid, ADAPTER_INSTALL)
        .await
        .unwrap();
    let err = install_instance(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        IID,
        &InstallOpts {
            yes: true,
            ..InstallOpts::default()
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&err, Error::NeedSlotChoice(msg) if msg.contains(IID)),
        "{err}"
    );
    let m = install_instance(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        IID,
        &InstallOpts {
            yes: true,
            slot: Some("winmm".into()),
            ..InstallOpts::default()
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(m.files[0].dest, "winmm.dll");
    assert_eq!(fs::read(fx.root.join("winmm.dll")).unwrap(), STOCK);
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Consent names the dest the pick left behind. The first call only asks
/// which slot, and leaves the foreign file untouched.
#[tokio::test]
async fn convert_consent_names_prospective_proxy_dest() {
    let fx = setup("consent", Some("dxgi"), true).await;
    fs::write(fx.root.join("dxgi.dll"), b"user-dxgi").unwrap();
    let err = convert(&fx, ADAPTER_INSTALL, false, false)
        .await
        .unwrap_err();
    assert!(matches!(&err, Error::NeedSlotChoice(_)), "{err}");
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"user-dxgi");
    assert_eq!(fx.manifest().files[0].dest, "ReShade64.dll");
    pick(&fx, "dxgi").await;
    let err = convert(&fx, ADAPTER_INSTALL, false, true)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::NeedConfirm(msg) if msg.contains("dxgi.dll")),
        "{err}"
    );
    assert_eq!(fs::read(fx.root.join("dxgi.dll")).unwrap(), b"user-dxgi");
    assert_eq!(fx.manifest().files[0].dest, "dxgi.dll");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A user touch on the staged stock dest refuses before the first byte
/// moves — and the touch itself survives, since no rollback runs.
#[tokio::test]
async fn convert_touched_staging_refuses_pre_mutation() {
    let fx = setup("touched", Some("dxgi"), true).await;
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, IID);
    fs::write(stage.join("ReShade64.dll"), b"user-bytes").unwrap();
    let before = fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap();
    let err = convert(&fx, ADAPTER_INSTALL, true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::StagedModified(msg) if msg.contains("ReShade64.dll")),
        "{err}"
    );
    assert_eq!(
        fs::read(stage.join("ReShade64.dll")).unwrap(),
        b"user-bytes"
    );
    assert!(
        !stage.join("dxgi.dll").exists(),
        "no restage ran before the refusal"
    );
    assert_eq!(
        fs::read(crate::manifest_path(&fx.data, &fx.gid, IID)).unwrap(),
        before
    );
    assert!(!fx.root.join("dxgi.dll").exists());
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A failed convert leaves the slot pick in place: the rename already
/// committed, and the rollback restores the adapter and the game dir.
#[tokio::test]
async fn convert_rollback_keeps_the_committed_pick() {
    let fx = setup("rollback", Some("dxgi"), true).await;
    pick(&fx, "dxgi").await;
    arm_fail(FailPoint::AfterFirstCopy);
    let err = convert(&fx, ADAPTER_INSTALL, true, true).await.unwrap_err();
    disarm_fail();
    assert!(matches!(&err, Error::Install(_)), "{err}");
    let m = fx.manifest();
    assert_eq!(m.adapter, "preload");
    assert_eq!(m.files[0].dest, "dxgi.dll");
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, IID);
    assert_eq!(fs::read(stage.join("dxgi.dll")).unwrap(), STOCK);
    assert!(!stage.join("ReShade64.dll").exists());
    assert!(!fx.root.join("dxgi.dll").exists());
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
