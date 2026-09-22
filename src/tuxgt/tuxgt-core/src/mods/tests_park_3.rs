//! Second-review holes: restored backups, dest keep, cross-root handoff.
use super::tests_park::{file, parked, runtime, setup, write_mod};
use super::*;
use std::fs;

fn planned(dest: &str, sha: &str) -> crate::PlannedFile {
    crate::PlannedFile {
        source: dest.into(),
        dest: dest.into(),
        sha256: sha.into(),
        enabled: true,
        load: None,
    }
}

fn stage_opti(fx: &super::tests_park::Fx) -> (String, String) {
    let stage = crate::stage::stage_dir(&fx.data, &fx.gid, "opti");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(stage.join("OptiScaler.ini"), b"stock").unwrap();
    let dll = crate::sha256_file(&stage.join("OptiScaler.dll")).unwrap();
    let ini = crate::sha256_file(&stage.join("OptiScaler.ini")).unwrap();
    (dll, ini)
}

#[tokio::test]
async fn disable_leaves_restored_optiscaler_backup() {
    let fx = setup("optibak", true).await;
    let (dll_sha, ini_sha) = stage_opti(&fx);
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
    let mut m = crate::need_manifest(&fx.data, &fx.gid, "opti").unwrap();
    m.backups.insert("OptiScaler.ini".into(), "bak/ini".into());
    crate::write_manifest(&fx.data, &m).unwrap();
    fs::create_dir_all(fx.data.join("bak")).unwrap();
    fs::write(fx.data.join("bak/ini"), b"original").unwrap();
    fs::write(fx.game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(fx.game.join("OptiScaler.ini"), b"stock").unwrap();
    fs::write(fx.game.join("OptiScaler.log"), b"log").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "opti", false, true)
        .await
        .unwrap();

    assert_eq!(
        fs::read(fx.game.join("OptiScaler.ini")).unwrap(),
        b"original"
    );
    assert!(!parked(&fx, "opti", "game").join("OptiScaler.ini").exists());
    assert_eq!(
        fs::read(parked(&fx, "opti", "game").join("OptiScaler.log")).unwrap(),
        b"log"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_leaves_restored_optiscaler_backup() {
    let fx = setup("optibak2", true).await;
    let (dll_sha, ini_sha) = stage_opti(&fx);
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
    let mut m = crate::need_manifest(&fx.data, &fx.gid, "opti").unwrap();
    m.backups.insert("OptiScaler.ini".into(), "bak/ini".into());
    crate::write_manifest(&fx.data, &m).unwrap();
    fs::create_dir_all(fx.data.join("bak")).unwrap();
    fs::write(fx.data.join("bak/ini"), b"original").unwrap();
    fs::write(fx.game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(fx.game.join("OptiScaler.ini"), b"stock").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "opti", true)
        .await
        .unwrap();

    assert_eq!(
        fs::read(fx.game.join("OptiScaler.ini")).unwrap(),
        b"original"
    );
    assert!(fx.game.is_dir());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn disable_leaves_ini_an_enabled_dest_claims() {
    let fx = setup("destkeep", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    write_mod(&fx, "other", "custom", "preload", vec![file("ReShade.ini")]);
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"theirs").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", false, true)
        .await
        .unwrap();

    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"theirs");
    assert!(!rt.join("ReShade64.dll").exists());
    assert_eq!(
        fs::read(parked(&fx, "reshade", "runtime").join("ReShade64.dll")).unwrap(),
        b"rs"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_moves_live_ini_to_enabled_install_root() {
    let fx = setup("cross", true).await;
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
        "install",
        vec![file("ShaderToggler.addon64")],
    );
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"tuned").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "reshade", true)
        .await
        .unwrap();

    assert!(!rt.join("ReShade.ini").exists());
    assert_eq!(fs::read(fx.game.join("ReShade.ini")).unwrap(), b"tuned");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn enable_prefers_the_live_root_park_when_both_sides_have_the_ini() {
    let fx = setup("both", true).await;
    let (dll_sha, ini_sha) = stage_opti(&fx);
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
    fs::create_dir_all(parked(&fx, "opti", "game")).unwrap();
    fs::create_dir_all(parked(&fx, "opti", "runtime")).unwrap();
    fs::write(
        parked(&fx, "opti", "game").join("OptiScaler.ini"),
        b"from-game",
    )
    .unwrap();
    fs::write(
        parked(&fx, "opti", "runtime").join("OptiScaler.ini"),
        b"from-runtime",
    )
    .unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "opti", true, true)
        .await
        .unwrap();

    assert_eq!(
        fs::read(fx.game.join("OptiScaler.ini")).unwrap(),
        b"from-game"
    );
    let _ = fs::remove_dir_all(&fx.dir);

    let fx = setup("bothpre", false).await;
    write_mod(
        &fx,
        "reshade",
        "reshade",
        "preload",
        vec![file("ReShade64.dll")],
    );
    fs::create_dir_all(parked(&fx, "reshade", "game")).unwrap();
    fs::create_dir_all(parked(&fx, "reshade", "runtime")).unwrap();
    fs::write(
        parked(&fx, "reshade", "game").join("ReShade.ini"),
        b"from-game",
    )
    .unwrap();
    fs::write(
        parked(&fx, "reshade", "runtime").join("ReShade.ini"),
        b"from-runtime",
    )
    .unwrap();
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "reshade", true, true)
        .await
        .unwrap();
    assert_eq!(
        fs::read(runtime(&fx).join("ReShade.ini")).unwrap(),
        b"from-runtime"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_removes_empty_runtime() {
    let fx = setup("emptyrt", false).await;
    write_mod(&fx, "side", "custom", "preload", vec![file("only.dll")]);
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("only.dll"), b"d").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "side", true)
        .await
        .unwrap();

    assert!(!rt.exists());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_confirm_names_only_non_generated_foreign_dests() {
    let fx = setup("confirm", true).await;
    let (dll_sha, ini_sha) = stage_opti(&fx);
    let extra_src = fx.dir.join("extra-src");
    fs::write(&extra_src, b"tracked").unwrap();
    let extra_sha = crate::sha256_file(&extra_src).unwrap();
    write_mod(
        &fx,
        "opti",
        "optiscaler",
        "install",
        vec![
            planned("OptiScaler.dll", &dll_sha),
            planned("OptiScaler.ini", &ini_sha),
            planned("extra.dll", &extra_sha),
        ],
    );
    fs::write(fx.game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(fx.game.join("OptiScaler.ini"), b"tuned").unwrap();
    fs::write(fx.game.join("extra.dll"), b"foreign").unwrap();

    let err = uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "opti", false)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("extra.dll"), "{msg}");
    assert!(!msg.contains("OptiScaler.ini"), "{msg}");
    assert!(fx.game.join("OptiScaler.ini").is_file());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_diverged_ini_does_not_ask_confirm() {
    let fx = setup("noconf", true).await;
    let (dll_sha, ini_sha) = stage_opti(&fx);
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
    fs::write(fx.game.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(fx.game.join("OptiScaler.ini"), b"tuned").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "opti", false)
        .await
        .unwrap();

    assert!(!fx.game.join("OptiScaler.ini").exists());
    assert!(fx.game.is_dir());
    let _ = fs::remove_dir_all(&fx.dir);
}

fn backup_dll(fx: &super::tests_park::Fx, instance: &str) {
    let mut m = crate::need_manifest(&fx.data, &fx.gid, instance).unwrap();
    m.backups.insert("mod.dll".into(), "bak/dll".into());
    crate::write_manifest(&fx.data, &m).unwrap();
    fs::create_dir_all(fx.data.join("bak")).unwrap();
    fs::write(fx.data.join("bak/dll"), b"original").unwrap();
    fs::write(fx.game.join("mod.dll"), b"tracked").unwrap();
}

#[tokio::test]
async fn disable_parks_runtime_copy_of_a_restored_dest() {
    let fx = setup("rtrest", true).await;
    let sha = {
        let tracked = fx.dir.join("tracked.dll");
        fs::write(&tracked, b"tracked").unwrap();
        crate::sha256_file(&tracked).unwrap()
    };
    write_mod(
        &fx,
        "mod",
        "custom",
        "install",
        vec![planned("mod.dll", &sha)],
    );
    backup_dll(&fx, "mod");
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("mod.dll"), b"rt").unwrap();

    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "mod", false, true)
        .await
        .unwrap();

    assert_eq!(fs::read(fx.game.join("mod.dll")).unwrap(), b"original");
    assert!(!rt.join("mod.dll").exists());
    assert_eq!(
        fs::read(parked(&fx, "mod", "runtime").join("mod.dll")).unwrap(),
        b"rt"
    );
    assert!(!parked(&fx, "mod", "game").join("mod.dll").exists());
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn uninstall_hands_runtime_copy_of_a_restored_dest() {
    let fx = setup("rtrest2", true).await;
    let sha = {
        let tracked = fx.dir.join("tracked.dll");
        fs::write(&tracked, b"tracked").unwrap();
        crate::sha256_file(&tracked).unwrap()
    };
    write_mod(
        &fx,
        "main",
        "custom",
        "install",
        vec![planned("mod.dll", &sha)],
    );
    write_mod(
        &fx,
        "other",
        "custom",
        "preload",
        vec![planned("mod.dll", &sha)],
    );
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "other", false, true)
        .await
        .unwrap();
    backup_dll(&fx, "main");
    let rt = runtime(&fx);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("mod.dll"), b"rt").unwrap();

    uninstall_instance(&fx.pool, &fx.data, &fx.dir, &fx.gid, "main", true)
        .await
        .unwrap();

    assert_eq!(fs::read(fx.game.join("mod.dll")).unwrap(), b"original");
    assert!(!rt.join("mod.dll").exists());
    assert_eq!(
        fs::read(parked(&fx, "other", "runtime").join("mod.dll")).unwrap(),
        b"rt"
    );
    let _ = fs::remove_dir_all(&fx.dir);
}
