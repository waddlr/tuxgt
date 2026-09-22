//! Adapter conversion carries runtime-generated (harvested) files between
//! the game dir and `<game>/runtime/`: Install→Preload must not orphan
//! `ReShade.ini` in the game dir, and Preload→Install must not orphan it in
//! runtime. Same-content collisions dedupe silently; differing ones need
//! consent (the live side wins); rollback restores both roots.
use super::adapter::{arm_fail, disarm_fail, FailPoint};
use super::*;
use crate::game::{game_adapter, set_game_adapter, ADAPTER_INSTALL, ADAPTER_PRELOAD};
use crate::instance::{payload_dir, user_mods_dir};
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest, PlannedFile};
use sqlx::SqlitePool;
use std::fs;
use std::path::PathBuf;

const IID: &str = "rhshade";
const STOCK: &[u8] = b"reshade-bytes";
const TUNED_INI: &[u8] = b"[General]\nPresetPath=tuned.ini\n";
const STALE_INI: &[u8] = b"[General]\nPresetPath=stale.ini\n";
const LOG: &[u8] = b"reshade log\n";

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
    fn runtime(&self) -> PathBuf {
        crate::runtime_dir(&self.data, &self.gid)
    }
}

/// A game with a ReShade recipe (slot `dxgi`, both adapters allowed) and a
/// manifest mirroring real state: stock-named on preload, proxied on
/// install with the tracked copy in the game dir.
async fn setup(tag: &str, adapter: &str) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-rh-{tag}-{}-{}",
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
    let gid = format!("manual:standalone:rh{tag}");
    let exe = root.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: &format!("rh{tag}"),
            name: Some("RH"),
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
    fs::create_dir_all(user_mods_dir(&data)).unwrap();
    fs::write(
        user_mods_dir(&data).join(format!("{IID}.toml")),
        format!(
            "id = \"{IID}\"\ntype = \"reshade\"\nlabel = \"RH Shade\"\nslot = \"dxgi\"\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            pkg.to_str().unwrap(),
        ),
    )
    .unwrap();
    let payload = payload_dir(&data, false, None, IID);
    fs::create_dir_all(&payload).unwrap();
    fs::write(payload.join("ReShade64.dll"), STOCK).unwrap();
    let dest = if adapter == ADAPTER_INSTALL {
        "dxgi.dll"
    } else {
        "ReShade64.dll"
    };
    let sha = crate::sha256_file(&payload.join("ReShade64.dll")).unwrap();
    let m = FileManifest {
        game: gid.clone(),
        instance: IID.into(),
        mod_type: "reshade".into(),
        adapter: adapter.into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![PlannedFile {
            source: format!("mods/user/{IID}/ReShade64.dll"),
            dest: dest.into(),
            sha256: sha.clone(),
            enabled: true,
            load: None,
        }]
        .into_boxed_slice(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: vec!["ReShade.ini".to_string(), "ReShade.log".to_string()]
            .into_boxed_slice(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    };
    let staged = [crate::StageInput {
        rel: dest,
        src: &payload.join("ReShade64.dll"),
        sha: &sha,
    }];
    crate::sync_staging(&data, &gid, IID, &m.mod_type, &staged, false).unwrap();
    crate::write_manifest(&data, &m).unwrap();
    if adapter == ADAPTER_INSTALL {
        fs::copy(
            crate::stage_dir(&data, &gid, IID).join(dest),
            root.join(dest),
        )
        .unwrap();
    }
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

async fn convert(fx: &Fx, target: &str, yes: bool) -> crate::Result<ConversionReport> {
    // The picks are already in the fixture dests. These tests move harvested
    // files, so the slot prompt is answered up front.
    convert_game_adapter(&fx.pool, &fx.data, &fx.cfg, &fx.gid, target, yes, true).await
}

/// The reported bug: Install→Preload leaves `ReShade.ini`/`.log` in the
/// game dir while the mod now runs from `<game>/runtime/`.
#[tokio::test]
async fn install_to_preload_moves_harvested_to_runtime() {
    let fx = setup("topreload", ADAPTER_INSTALL).await;
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::write(fx.root.join("ReShade.log"), LOG).unwrap();
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "preload");
    assert_eq!(fx.manifest().adapter, "preload");
    assert!(!fx.root.join("ReShade.ini").is_file());
    assert!(!fx.root.join("ReShade.log").is_file());
    assert!(!fx.root.join("dxgi.dll").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        TUNED_INI
    );
    assert_eq!(fs::read(fx.runtime().join("ReShade.log")).unwrap(), LOG);
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Mirror: Preload→Install carries the runtime-side settings into the
/// game dir instead of orphaning them under `<game>/runtime/`.
#[tokio::test]
async fn preload_to_install_moves_harvested_to_game_dir() {
    let fx = setup("toinstall", ADAPTER_PRELOAD).await;
    fs::create_dir_all(fx.runtime()).unwrap();
    fs::write(fx.runtime().join("ReShade.ini"), TUNED_INI).unwrap();
    convert(&fx, ADAPTER_INSTALL, true).await.unwrap();
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert!(!fx.runtime().join("ReShade.ini").exists());
    assert_eq!(fs::read(fx.root.join("ReShade.ini")).unwrap(), TUNED_INI);
    assert!(fx.root.join("ReShade64.dll").is_file());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Same bytes at both ends: no consent needed, the source copy drops.
#[tokio::test]
async fn identical_collision_dedupes_without_consent() {
    let fx = setup("same", ADAPTER_INSTALL).await;
    fs::create_dir_all(fx.runtime()).unwrap();
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::write(fx.runtime().join("ReShade.ini"), TUNED_INI).unwrap();
    convert(&fx, ADAPTER_PRELOAD, false).await.unwrap();
    assert!(!fx.root.join("ReShade.ini").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        TUNED_INI
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Differing bytes at both ends: refuse before any mutation, then the
/// live (source-side) settings win once consented.
#[tokio::test]
async fn differing_collision_needs_confirm_then_source_wins() {
    let fx = setup("clash", ADAPTER_INSTALL).await;
    fs::create_dir_all(fx.runtime()).unwrap();
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::write(fx.runtime().join("ReShade.ini"), STALE_INI).unwrap();
    let err = convert(&fx, ADAPTER_PRELOAD, false).await.unwrap_err();
    match &err {
        Error::NeedConfirm(msg) => assert!(msg.contains("ReShade.ini"), "{msg}"),
        other => panic!("expected NeedConfirm, got {other}"),
    }
    assert_eq!(fs::read(fx.root.join("ReShade.ini")).unwrap(), TUNED_INI);
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        STALE_INI
    );
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert!(!fx.root.join("ReShade.ini").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        TUNED_INI
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// Pre-E18 manifests with empty globs still move: planning backfills
/// from the mod type like harvest does, without persisting.
#[tokio::test]
async fn empty_globs_backfill_from_mod_type() {
    let fx = setup("backfill", ADAPTER_INSTALL).await;
    let mut m = fx.manifest();
    m.generated_globs = Box::default();
    crate::write_manifest(&fx.data, &m).unwrap();
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert!(!fx.root.join("ReShade.ini").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        TUNED_INI
    );
    assert!(fx.manifest().generated_globs.is_empty());
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A failure after the harvested-file move restores both roots, the
/// manifest, and the stored choice; retrying then succeeds cleanly.
#[tokio::test]
async fn rollback_restores_harvested_both_roots() {
    let fx = setup("rollback", ADAPTER_INSTALL).await;
    fs::create_dir_all(fx.runtime()).unwrap();
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::write(fx.root.join("ReShade.log"), LOG).unwrap();
    fs::write(fx.runtime().join("ReShade.ini"), STALE_INI).unwrap();
    fs::write(fx.runtime().join("OptiScaler.ini"), STALE_INI).unwrap();
    arm_fail(FailPoint::AfterHarvestedMove);
    let err = convert(&fx, ADAPTER_PRELOAD, true).await;
    disarm_fail();
    assert!(err.is_err());
    assert_eq!(fs::read(fx.root.join("ReShade.ini")).unwrap(), TUNED_INI);
    assert_eq!(fs::read(fx.root.join("ReShade.log")).unwrap(), LOG);
    assert!(fx.root.join("dxgi.dll").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        STALE_INI
    );
    assert!(!fx.runtime().join("ReShade.log").exists());
    assert_eq!(
        fs::read(fx.runtime().join("OptiScaler.ini")).unwrap(),
        STALE_INI
    );
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().adapter, "install");
    convert(&fx, ADAPTER_PRELOAD, true).await.unwrap();
    assert!(!fx.root.join("ReShade.ini").is_file());
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        TUNED_INI
    );
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// A directory blocking a harvested dest refuses even when consented —
/// it is obstruction, not an overwrite — and mutates nothing.
#[tokio::test]
async fn non_file_dest_refuses_without_mutating() {
    let fx = setup("notfile", ADAPTER_INSTALL).await;
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::create_dir_all(fx.runtime().join("ReShade.ini")).unwrap();
    let err = convert(&fx, ADAPTER_PRELOAD, true).await.unwrap_err();
    assert!(matches!(err, Error::NotAFile(_)), "{err}");
    assert_eq!(fs::read(fx.root.join("ReShade.ini")).unwrap(), TUNED_INI);
    assert!(fx.runtime().join("ReShade.ini").is_dir());
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    assert_eq!(fx.manifest().adapter, "install");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}

/// The dry-run refuses a harvested collision exactly like the conversion
/// and persists nothing, so the GUI can run it before stopping the client.
#[tokio::test]
async fn validate_refuses_harvested_collision_without_mutating() {
    let fx = setup("validate", ADAPTER_INSTALL).await;
    fs::create_dir_all(fx.runtime()).unwrap();
    fs::write(fx.root.join("ReShade.ini"), TUNED_INI).unwrap();
    fs::write(fx.runtime().join("ReShade.ini"), STALE_INI).unwrap();
    let verr = validate_adapter_convert(
        &fx.pool,
        &fx.data,
        &fx.cfg,
        &fx.gid,
        ADAPTER_PRELOAD,
        false,
        true,
    )
    .await
    .unwrap_err();
    let cerr = convert(&fx, ADAPTER_PRELOAD, false).await.unwrap_err();
    assert!(verr.to_string().contains("ReShade.ini"), "{verr}");
    assert_eq!(verr.to_string(), cerr.to_string());
    assert_eq!(fs::read(fx.root.join("ReShade.ini")).unwrap(), TUNED_INI);
    assert_eq!(
        fs::read(fx.runtime().join("ReShade.ini")).unwrap(),
        STALE_INI
    );
    assert_eq!(game_adapter(&fx.pool, &fx.gid).await.unwrap(), "install");
    let _ = tokio::fs::remove_dir_all(&fx.dir).await;
}
