use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use std::fs;

#[tokio::test]
async fn load_order_assign_reinstall_reorder() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-loadorder-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    let pkg_a = dir.join("pkg-a");
    let pkg_b = dir.join("pkg-b");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&pkg_a).unwrap();
    fs::create_dir_all(&pkg_b).unwrap();
    fs::write(pkg_a.join("plug.dll"), b"a-v1").unwrap();
    fs::write(pkg_b.join("plug.dll"), b"b-v1").unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:loadorder3";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("LoadOrder"),
            exe_path: Some(data.join("game.exe").to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    crate::add_mod_from(
        &cfg,
        "custom",
        "mod-a",
        &pkg_a,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    crate::add_mod_from(
        &cfg,
        "custom",
        "mod-b",
        &pkg_b,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    fs::write(data.join("game.exe"), b"MZ").unwrap();
    // Fresh installs assign max + 1: later install wins.
    let a = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "mod-a",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(a.load_order, 0);
    let b = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "mod-b",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(b.load_order, 1);
    // Reinstall keeps its value.
    let a2 = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "mod-a",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(a2.load_order, 0);
    // Validation: unknown / missing / duplicate ids name the id.
    let err = set_load_order(&pool, &data, gid, &["mod-a".into(), "nope".into()])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    assert!(err.to_string().contains("nope"), "{err}");
    let err = set_load_order(&pool, &data, gid, &["mod-a".into()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mod-b"), "{err}");
    let err = set_load_order(&pool, &data, gid, &["mod-a".into(), "mod-a".into()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("mod-a"), "{err}");
    // Reorder flips the winner.
    let ordered = set_load_order(&pool, &data, gid, &["mod-b".into(), "mod-a".into()])
        .await
        .unwrap();
    assert_eq!(ordered[0].instance, "mod-b");
    assert_eq!(ordered[0].load_order, 0);
    assert_eq!(ordered[1].instance, "mod-a");
    assert_eq!(ordered[1].load_order, 1);
    let _ = fs::remove_dir_all(&dir);
}
