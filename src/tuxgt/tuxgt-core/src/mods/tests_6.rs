use super::testing::*;
use super::*;
use std::fs;

/// Manifest-only second instance (disable never checks staging).
async fn add_instance(fx: &Fx, iid: &str, files: &[(&str, bool)]) {
    let m = crate::FileManifest {
        game: fx.gid.clone(),
        instance: iid.into(),
        mod_type: "reshade_addon".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 1,
        include: Box::default(),
        files: files
            .iter()
            .map(|(dest, on)| crate::PlannedFile {
                source: format!("mods/user/{iid}/{dest}"),
                dest: dest.to_string(),
                sha256: "h".into(),
                enabled: *on,
            })
            .collect(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&fx.data, &m).unwrap();
}

#[tokio::test]
async fn disable_instance_while_over_budget_ratchets_down() {
    let tag = "disoff";
    let fx = seed(tag, 96, 0, &[]).await;
    let bs: Vec<String> = (0..10).map(|i| format!("b-{}", dll(i, tag))).collect();
    let bref: Vec<(&str, bool)> = bs.iter().map(|d| (d.as_str(), true)).collect();
    add_instance(&fx, "beaver", &bref).await;
    let ini = crate::prewire::managed_ini(&crate::game::game_dir(
        &fx.data,
        &crate::game::GameId::parse(&fx.gid).unwrap(),
    ));
    crate::prewire_game(&fx.data, &fx.pool, &fx.gid)
        .await
        .unwrap_err();
    let before = fs::read(&ini).unwrap();
    // Disabling a contributor still over budget lands; the ini stays stale.
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "beaver", false, true)
        .await
        .unwrap();
    assert_eq!(
        fs::read(&ini).unwrap(),
        before,
        "ini written while over budget"
    );
    let b = crate::need_manifest(&fx.data, &fx.gid, "beaver").unwrap();
    assert!(!b.enabled);
    // Disabling the remainder fits: the ini is rewritten without LoadDLLs.
    set_instance_enabled(&fx.pool, &fx.data, &fx.gid, &fx.iid, false, true)
        .await
        .unwrap();
    let text = fs::read_to_string(&ini).unwrap();
    assert!(!text.contains("LoadDLL="), "{text}");
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn disable_off_over_list_while_over_budget_refused_before_write() {
    let tag = "disref";
    let fx = seed(tag, 96, 0, &[]).await;
    add_instance(
        &fx,
        "treeonly",
        &[("reshade-shaders/Shaders/pack/a.fx", true)],
    )
    .await;
    // Removing the tree shrinks only the under-budget list while LoadDLL
    // stays over: refused, nothing moves.
    let man = crate::manifest_path(&fx.data, &fx.gid, "treeonly");
    let before = fs::read(&man).unwrap();
    let err = set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "treeonly", false, true)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("8192-byte budget"), "{msg}");
    assert!(msg.contains("uninstall"), "{msg}");
    assert_eq!(fs::read(&man).unwrap(), before);
    let _ = fs::remove_dir_all(&fx.dir);
}

#[tokio::test]
async fn refused_install_enable_leaves_manifest_untouched() {
    use crate::testing::{seed_game, SeedGame};
    let tag = "conforder";
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-conforder-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let game_dir = dir.join("game");
    fs::create_dir_all(&game_dir).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let gid = format!("manual:standalone:{tag}");
    seed_game(
        &pool,
        SeedGame {
            id: &gid,
            install_dir: Some(game_dir.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    // Disabled install-adapter instance with a staged foreign dest.
    let stage = crate::stage::stage_dir(&data, &gid, "dxmod");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("dxgi.dll"), b"v1").unwrap();
    let sha = crate::sha256_file(&stage.join("dxgi.dll")).unwrap();
    let m = crate::FileManifest {
        game: gid.clone(),
        instance: "dxmod".into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
        enabled: false,
        load_order: 0,
        include: Box::default(),
        files: vec![crate::PlannedFile {
            source: "mods/user/dxmod/dxgi.dll".into(),
            dest: "dxgi.dll".into(),
            sha256: sha,
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&data, &m).unwrap();
    // Foreign original occupies the game-dir dest.
    fs::write(game_dir.join("dxgi.dll"), b"orig").unwrap();
    let man = crate::manifest_path(&data, &gid, "dxmod");
    let before = fs::read(&man).unwrap();
    let err = set_instance_enabled(&pool, &data, &gid, "dxmod", true, false)
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::Error::NeedConfirm(_)),
        "expected NeedConfirm, got {err}"
    );
    assert_eq!(
        fs::read(&man).unwrap(),
        before,
        "refused enable mutated the manifest"
    );
    assert_eq!(fs::read(game_dir.join("dxgi.dll")).unwrap(), b"orig");
    // Confirming proceeds: copies applied, manifest enabled.
    let m = set_instance_enabled(&pool, &data, &gid, "dxmod", true, true)
        .await
        .unwrap();
    assert!(m.enabled);
    assert_eq!(fs::read(game_dir.join("dxgi.dll")).unwrap(), b"v1");
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn enable_on_over_list_while_over_budget_refused_before_write() {
    let tag = "enref";
    let fx = seed(tag, 96, 0, &[]).await;
    add_instance(&fx, "sleepy", &[("zzz.dll", true)]).await;
    let mut m = crate::need_manifest(&fx.data, &fx.gid, "sleepy").unwrap();
    m.enabled = false;
    crate::write_manifest(&fx.data, &m).unwrap();
    // Enabling grows an over-budget list: refused with the enable verb,
    // nothing moves.
    let man = crate::manifest_path(&fx.data, &fx.gid, "sleepy");
    let before = fs::read(&man).unwrap();
    let err = set_instance_enabled(&fx.pool, &fx.data, &fx.gid, "sleepy", true, true)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("cannot enable sleepy"), "{msg}");
    assert!(msg.contains("8192-byte budget"), "{msg}");
    assert_eq!(fs::read(&man).unwrap(), before);
    let _ = fs::remove_dir_all(&fx.dir);
}
