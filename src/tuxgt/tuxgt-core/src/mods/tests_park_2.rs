//! Review holes: tuned OptiScaler.ini, live handoff, stale-ini park.
use super::tests_park::{file, parked, runtime, setup, write_mod};
use super::*;
use std::fs;

#[tokio::test]
async fn install_disable_parks_tuned_optiscaler_ini() {
    let fx = setup("opti", true).await;
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, "opti");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(stage.join("OptiScaler.ini"), b"stock").unwrap();
    let dll_sha = crate::sha256_file(&stage.join("OptiScaler.dll")).unwrap();
    let ini_sha = crate::sha256_file(&stage.join("OptiScaler.ini")).unwrap();
    let planned = |dest: &str, sha: &str| crate::PlannedFile {
        source: dest.into(),
        dest: dest.into(),
        sha256: sha.into(),
        enabled: true,
        load: None,
    };
    write_mod(
        &fx,
        "opti",
        "optiscaler",
        "install",
        vec![
            planned("OptiScaler.dll", &dll_sha),
            planned("OptiScaler.ini", &ini_sha),
        ],
    );
    let game = fx.game.clone();
    fs::write(game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(game.join("OptiScaler.ini"), b"tuned").unwrap();
    fs::write(game.join("OptiScaler.log"), b"log").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "opti", false, true)
        .await
        .unwrap();

    assert!(!game.join("OptiScaler.dll").exists());
    assert!(!game.join("OptiScaler.ini").exists());
    assert!(!game.join("OptiScaler.log").exists());
    assert!(game.is_dir());
    let park = parked(&fx, "opti", "game");
    assert_eq!(fs::read(park.join("OptiScaler.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(park.join("OptiScaler.log")).unwrap(), b"log");
    assert!(!park.join("OptiScaler.dll").exists());

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "opti", true, true)
        .await
        .unwrap();
    assert_eq!(fs::read(game.join("OptiScaler.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(game.join("OptiScaler.log")).unwrap(), b"log");
    assert_eq!(fs::read(game.join("OptiScaler.dll")).unwrap(), b"dll");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_removes_tuned_optiscaler_ini_left_foreign() {
    let fx = setup("optidrop", true).await;
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, "opti");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(stage.join("OptiScaler.ini"), b"stock").unwrap();
    let dll_sha = crate::sha256_file(&stage.join("OptiScaler.dll")).unwrap();
    let ini_sha = crate::sha256_file(&stage.join("OptiScaler.ini")).unwrap();
    write_mod(
        &fx,
        "opti",
        "optiscaler",
        "install",
        vec![
            crate::PlannedFile {
                source: "OptiScaler.dll".into(),
                dest: "OptiScaler.dll".into(),
                sha256: dll_sha,
                enabled: true,
                load: None,
            },
            crate::PlannedFile {
                source: "OptiScaler.ini".into(),
                dest: "OptiScaler.ini".into(),
                sha256: ini_sha,
                enabled: true,
                load: None,
            },
        ],
    );
    fs::write(fx.game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(fx.game.join("OptiScaler.ini"), b"tuned").unwrap();
    fs::write(fx.game.join("OptiScaler.log"), b"log").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "opti", true)
        .await
        .unwrap();

    assert!(!fx.game.join("OptiScaler.ini").exists());
    assert!(!fx.game.join("OptiScaler.log").exists());
    assert!(!fx.game.join("OptiScaler.dll").exists());
    assert!(fx.game.is_dir());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_moves_live_generated_to_disabled_claimant() {
    let fx = setup("livehand", false).await;
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

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "reshade", true)
        .await
        .unwrap();

    assert!(!rt.join("ReShade.ini").exists());
    assert!(!rt.join("ReShade64.dll").exists());
    assert_eq!(
        fs::read(parked(&fx, "addon", "runtime").join("ReShade.ini")).unwrap(),
        b"tuned"
    );
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "addon", true, true)
        .await
        .unwrap();
    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"tuned");
    assert_eq!(fs::read(rt.join("ShaderToggler.addon64")).unwrap(), b"add");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_moves_live_dest_to_disabled_claimant() {
    let fx = setup("livedest", false).await;
    write_mod(&fx, "a", "custom", "preload", vec![file("shared.dll")]);
    write_mod(&fx, "b", "custom", "preload", vec![file("shared.dll")]);
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("shared.dll"), b"s").unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "b", false, true)
        .await
        .unwrap();
    assert_eq!(fs::read(rt.join("shared.dll")).unwrap(), b"s");

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "a", true)
        .await
        .unwrap();

    assert!(!rt.join("shared.dll").exists());
    assert_eq!(
        fs::read(parked(&fx, "b", "runtime").join("shared.dll")).unwrap(),
        b"s"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn over_budget_disable_does_not_park() {
    let tag = "parkbudg";
    let fx = super::testing::seed(tag, 96, 0, &[]).await;
    let dll_name = "only-side.dll";
    crate::write_manifest(
        &fx.data,
        &crate::FileManifest {
            game: fx.gid.clone(),
            instance: "side".into(),
            mod_type: "custom".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 1,
            include: Box::default(),
            files: vec![crate::PlannedFile {
                source: dll_name.into(),
                dest: dll_name.into(),
                sha256: "aa".into(),
                enabled: true,
                load: None,
            }]
            .into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
    let rt = crate::stage::runtime_dir(&fx.data, &fx.gid);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join(dll_name), b"stay").unwrap();
    let gdir = crate::game::game_dir(&fx.data, &crate::game::GameId::parse(&fx.gid).unwrap());
    let ini = crate::prewire::managed_ini(&gdir);
    let _ = crate::prewire_game(&fx.data, &fx.pool, &fx.gid).await;
    let before = fs::read(&ini).unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "side", false, true)
        .await
        .unwrap();

    assert_eq!(fs::read(&ini).unwrap(), before);
    assert_eq!(fs::read(rt.join(dll_name)).unwrap(), b"stay");
    assert!(!gdir.join("disabled").join("side").exists());
    assert!(
        !crate::need_manifest(&fx.data, &fx.gid, "side")
            .unwrap()
            .enabled
    );
    let _ = fs::remove_dir_all(&fx.dir);
}
