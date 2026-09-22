//! Disable parks runtime dests and generated files; uninstall deletes them.
use super::*;
use crate::testing::{seed_game, SeedGame};
use std::fs;
use std::path::PathBuf;

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

pub(super) struct Fx {
    pub(super) dir: PathBuf,
    pub(super) data: PathBuf,
    pub(super) game: PathBuf,
    pub(super) pool: sqlx::SqlitePool,
    pub(super) gid: String,
}

pub(super) async fn setup(tag: &str, with_game: bool) -> Fx {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-park-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let game = dir.join("gamedir");
    if with_game {
        fs::create_dir_all(&game).unwrap();
    }
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:{tag}");
    let exe = dir.join("game.exe");
    fs::write(&exe, b"MZ").unwrap();
    let install = with_game.then(|| game.to_string_lossy().into_owned());
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            manager: "manual",
            store: "standalone",
            game_id: tag,
            name: Some(tag),
            exe_path: Some(exe.to_str().unwrap()),
            install_dir: install.as_deref(),
            ..Default::default()
        },
    )
    .await;
    Fx {
        dir,
        data,
        game,
        pool,
        gid,
    }
}

pub(super) fn file(dest: &str) -> crate::PlannedFile {
    crate::PlannedFile {
        source: dest.into(),
        dest: dest.into(),
        sha256: "aa".into(),
        enabled: true,
        load: None,
    }
}

pub(super) fn write_mod(
    fx: &Fx,
    instance: &str,
    mod_type: &str,
    adapter: &str,
    files: Vec<crate::PlannedFile>,
) {
    crate::write_manifest(
        &fx.data,
        &crate::FileManifest {
            game: fx.gid.clone(),
            instance: instance.into(),
            mod_type: mod_type.into(),
            adapter: adapter.into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: files.into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
}

pub(super) fn runtime(fx: &Fx) -> PathBuf {
    crate::stage::runtime_dir(&fx.data, &fx.gid)
}

pub(super) fn parked(fx: &Fx, instance: &str, side: &str) -> PathBuf {
    crate::game::game_dir(&fx.data, &crate::game::GameId::parse(&fx.gid).unwrap())
        .join("disabled")
        .join(instance)
        .join(side)
}

fn ini_text(fx: &Fx) -> String {
    let gdir = crate::game::game_dir(&fx.data, &crate::game::GameId::parse(&fx.gid).unwrap());
    fs::read_to_string(crate::prewire::managed_ini(&gdir)).unwrap_or_default()
}

#[tokio::test]
async fn disable_parks_runtime_files_and_enable_restores_them() {
    let fx = setup("park", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    write_mod(&fx, "other", "custom", "preload", vec![file("other.dll")]);
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"tuned").unwrap();
    fs::write(rt.join("ReShade.log"), b"log").unwrap();
    fs::write(rt.join("other.dll"), b"keep").unwrap();
    crate::prewire_game(&fx.data, &fx.pool, &fx.gid)
        .await
        .unwrap();
    assert!(ini_text(&fx).contains("LoadDLL=ReShade64.dll"));

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", false, true)
        .await
        .unwrap();

    assert!(!rt.join("ReShade64.dll").exists());
    assert!(!rt.join("ReShade.ini").exists());
    assert!(!rt.join("ReShade.log").exists());
    assert_eq!(fs::read(rt.join("other.dll")).unwrap(), b"keep");
    let park = parked(&fx, "reshade", "runtime");
    assert_eq!(fs::read(park.join("ReShade64.dll")).unwrap(), b"rs");
    assert_eq!(fs::read(park.join("ReShade.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(park.join("ReShade.log")).unwrap(), b"log");
    assert!(!ini_text(&fx).contains("LoadDLL=ReShade64.dll"));

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", true, true)
        .await
        .unwrap();
    assert_eq!(fs::read(rt.join("ReShade64.dll")).unwrap(), b"rs");
    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(rt.join("ReShade.log")).unwrap(), b"log");
    assert!(!park.join("ReShade.ini").exists());
    assert!(ini_text(&fx).contains("LoadDLL=ReShade64.dll"));
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn disable_leaves_generated_an_enabled_mod_claims() {
    let fx = setup("share", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    write_mod(
        &fx,
        "addon",
        "reshade_addon",
        "preload",
        vec![file("ShaderToggler.addon64")],
    );
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"tuned").unwrap();
    fs::write(rt.join("ShaderToggler.addon64"), b"add").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "addon", false, true)
        .await
        .unwrap();

    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(rt.join("ReShade64.dll")).unwrap(), b"rs");
    assert!(!rt.join("ShaderToggler.addon64").exists());
    assert_eq!(
        fs::read(parked(&fx, "addon", "runtime").join("ShaderToggler.addon64")).unwrap(),
        b"add"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_deletes_park_and_generated() {
    let fx = setup("drop", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"tuned").unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", false, true)
        .await
        .unwrap();
    assert!(parked(&fx, "reshade", "runtime")
        .join("ReShade.ini")
        .is_file());

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "reshade", true)
        .await
        .unwrap();

    let gdir = crate::game::game_dir(&fx.data, &crate::game::GameId::parse(&fx.gid).unwrap());
    assert!(!gdir.join("disabled").exists());
    assert!(!rt.join("ReShade.ini").exists());
    assert!(!rt.join("ReShade64.dll").exists());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_hands_shared_generated_to_the_other_disabled_mod() {
    let fx = setup("hand", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    write_mod(
        &fx,
        "addon",
        "reshade_addon",
        "preload",
        vec![file("ShaderToggler.addon64")],
    );
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"tuned").unwrap();
    fs::write(rt.join("ShaderToggler.addon64"), b"add").unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "addon", false, true)
        .await
        .unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", false, true)
        .await
        .unwrap();
    assert_eq!(
        fs::read(parked(&fx, "reshade", "runtime").join("ReShade.ini")).unwrap(),
        b"tuned"
    );

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "reshade", true)
        .await
        .unwrap();

    assert!(!parked(&fx, "reshade", "runtime").exists());
    assert_eq!(
        fs::read(parked(&fx, "addon", "runtime").join("ReShade.ini")).unwrap(),
        b"tuned"
    );
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "addon", true, true)
        .await
        .unwrap();
    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"tuned");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn install_disable_parks_game_dir_generated() {
    let fx = setup("inst", true).await;
    let game = fx.game.clone();
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "install",
        vec![file("ReShade64.dll")],
    );
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, "reshade");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(game.join("ReShade.ini"), b"tuned").unwrap();
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"old").unwrap();
    fs::write(rt.join("ReShade.log"), b"log").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", false, true)
        .await
        .unwrap();

    assert!(!game.join("ReShade.ini").exists());
    assert!(game.is_dir());
    assert_eq!(
        fs::read(parked(&fx, "reshade", "game").join("ReShade.ini")).unwrap(),
        b"tuned"
    );
    assert!(!rt.join("ReShade64.dll").exists());
    assert!(!rt.join("ReShade.log").exists());
    assert_eq!(
        fs::read(parked(&fx, "reshade", "runtime").join("ReShade64.dll")).unwrap(),
        b"old"
    );

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", true, true)
        .await
        .unwrap();
    assert_eq!(fs::read(game.join("ReShade.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(game.join("ReShade64.dll")).unwrap(), b"rs");
    assert!(!rt.join("ReShade64.dll").exists());
    let _ = fs::remove_dir_all(&fx.dir);
}
