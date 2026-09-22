use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{sha256_file, Error};
use std::fs;

/// Mint a custom pack whose payload already holds `pkg_files` (the recipe
/// pins the mint-time file list, so configs must ship in the pack — files
/// added to the payload dir later are dropped by the payload filter until
/// a rescan).
async fn config_game(
    tag: &str,
    pkg_files: &[(&str, &[u8])],
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
    sqlx::SqlitePool,
    String,
    String,
) {
    let dir = std::env::temp_dir().join(format!("tuxgt-cfg-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pkg = dir.join("pkg");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&pkg).unwrap();
    for (name, bytes) in pkg_files {
        fs::write(pkg.join(name), bytes).unwrap();
    }
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:cfg{tag}");
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            name: Some("CFG"),
            ..Default::default()
        },
    )
    .await;
    let iid = format!("cfgplug{tag}");
    crate::add_mod_from(&cfg, "custom", &iid, &pkg, None, None, &data, Vec::new(), Vec::new())
        .unwrap();
    (dir, data, cfg, pool, gid, iid)
}

fn write_payload(data: &std::path::Path, iid: &str, name: &str, bytes: &[u8]) {
    fs::write(data.join("mods").join("user").join(iid).join(name), bytes).unwrap();
}

#[test]
fn config_text_allowlist() {
    for rel in [
        "Mod.ini",
        "dir/OptiScaler.INI",
        "preset.Cfg",
        "settings.conf",
        "mod.TOML",
        "data.json",
        "layout.XML",
        "notes.txt",
    ] {
        assert!(is_config_text(rel), "{rel}");
    }
    // DLLs never edit, whatever the case.
    assert!(!is_config_text("dxgi.dll"));
    assert!(!is_config_text("ReShade64.DLL"));
    // No extension or a trailing dot has no config type.
    assert!(!is_config_text("plug"));
    assert!(!is_config_text("foo."));
    // Repo junk stays out even with a text body.
    assert!(!is_config_text("README.md"));
    assert!(!is_config_text(".github/workflows/x.ini"));
    // Non-allowlisted extensions stay out.
    assert!(!is_config_text("shader.fx"));
    assert!(!is_config_text("lib.7z"));
}

#[tokio::test]
async fn config_paths_resolve_and_gate() {
    let (dir, data, cfg, _pool, _gid, iid) =
        config_game("paths", &[("plug.dll", b"plug-v1"), ("Mod.ini", b"v1")]).await;
    let p = payload_config_path(&cfg, &data, &iid, "Mod.ini").unwrap();
    assert!(p.is_file());
    assert!(payload_config_path(&cfg, &data, "no-such-mod", "Mod.ini").is_err());
    assert!(payload_config_path(&cfg, &data, &iid, "missing.ini").is_err());
    assert!(payload_config_path(&cfg, &data, &iid, "../evil.ini").is_err());
    assert!(payload_config_path(&cfg, &data, &iid, "/abs.ini").is_err());
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn payload_drift_detects_edit() {
    let (dir, data, cfg, pool, gid, iid) =
        config_game("drift", &[("plug.dll", b"plug-v1"), ("Mod.ini", b"v1")]).await;
    install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    assert!(payload_drift(&data, &gid, &iid).unwrap().is_empty());
    write_payload(&data, &iid, "Mod.ini", b"v2");
    assert_eq!(payload_drift(&data, &gid, &iid).unwrap(), ["Mod.ini"]);
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn push_updates_insync_and_preserves_touched() {
    let (dir, data, cfg, pool, gid, iid) = config_game(
        "push",
        &[("plug.dll", b"plug-v1"), ("Mod.ini", b"v1"), ("Keep.cfg", b"k1")],
    )
    .await;
    install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    let staged = |n: &str| crate::stage::stage_dir(&data, &gid, &iid).join(n);
    // Per-game touch on Keep.cfg; payload edit on Mod.ini.
    fs::write(staged("Keep.cfg"), b"user").unwrap();
    write_payload(&data, &iid, "Mod.ini", b"v2");
    let rep = push_global_edits(&data, &gid, &iid).unwrap();
    assert_eq!(rep.updated, ["Mod.ini"]);
    assert_eq!(rep.preserved, ["Keep.cfg"]);
    assert_eq!(fs::read(staged("Mod.ini")).unwrap(), b"v2");
    assert_eq!(fs::read(staged("Keep.cfg")).unwrap(), b"user");
    // Manifest sha refreshed to the edited payload bytes.
    let m = crate::read_manifest(&data, &gid, &iid).unwrap().unwrap();
    let sha = m
        .files
        .iter()
        .find(|f| f.dest == "Mod.ini")
        .unwrap()
        .sha256
        .clone();
    assert_eq!(sha, sha256_file(&data.join("mods").join("user").join(&iid).join("Mod.ini")).unwrap());
    assert_ne!(
        sha,
        m.files
            .iter()
            .find(|f| f.dest == "Keep.cfg")
            .unwrap()
            .sha256
            .clone()
    );
    // Fan-out reaches this game.
    let all = push_global_edits_all(&data, &iid);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].0, gid);
    assert!(all[0].1.is_ok());
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_tolerates_preexisting_touches() {
    let (dir, data, cfg, pool, gid, iid) =
        config_game("keep", &[("plug.dll", b"plug-v1"), ("Mod.ini", b"v1")]).await;
    install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    let staged = crate::stage::stage_dir(&data, &gid, &iid).join("Mod.ini");
    fs::write(&staged, b"user").unwrap();
    install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    assert_eq!(fs::read(&staged).unwrap(), b"user");
    // Manifest still records the depot bytes, not the touch.
    let m = crate::read_manifest(&data, &gid, &iid).unwrap().unwrap();
    assert_eq!(
        m.files.iter().find(|f| f.dest == "Mod.ini").unwrap().sha256,
        sha256_file(&data.join("mods").join("user").join(&iid).join("Mod.ini")).unwrap()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_new_touch_midflight_still_errors() {
    let (dir, data, cfg, pool, gid, iid) =
        config_game("new", &[("plug.dll", b"plug-v1"), ("A.ini", b"a1")]).await;
    install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap();
    // B.ini lands in the payload AND the source pack (rescan keeps it), plus
    // as a hand-dropped staged file the pre-scan never saw (unmanaged under
    // the prior manifest).
    fs::write(dir.join("pkg").join("B.ini"), b"b-depot").unwrap();
    write_payload(&data, &iid, "B.ini", b"b-depot");
    crate::rescan_mod(&cfg, &iid, None, None, &data).unwrap();
    let staged_b = crate::stage::stage_dir(&data, &gid, &iid).join("B.ini");
    fs::write(&staged_b, b"b-hand").unwrap();
    let err = install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap_err();
    match err {
        Error::StagedModified(msg) => assert!(msg.contains("B.ini"), "{msg}"),
        other => panic!("expected StagedModified, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_fresh_with_hand_dropped_file_still_errors() {
    let (dir, data, cfg, pool, gid, iid) =
        config_game("fresh", &[("plug.dll", b"plug-v1"), ("A.ini", b"a1")]).await;
    // No prior manifest: a hand-dropped staged file is not a pre-existing
    // touch to preserve — the strict rule still fires.
    let sdir = crate::stage::stage_dir(&data, &gid, &iid);
    fs::create_dir_all(&sdir).unwrap();
    fs::write(sdir.join("A.ini"), b"b-hand").unwrap();
    let err = install_instance(&pool, &data, &cfg, &gid, &iid, &InstallOpts::default(), None)
        .await
        .unwrap_err();
    match err {
        Error::StagedModified(msg) => assert!(msg.contains("A.ini"), "{msg}"),
        other => panic!("expected StagedModified, got {other:?}"),
    }
    assert!(crate::read_manifest(&data, &gid, &iid).unwrap().is_none());
    let _ = fs::remove_dir_all(&dir);
}
