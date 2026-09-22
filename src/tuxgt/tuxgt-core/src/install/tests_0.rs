use super::*;
use crate::{Error, FileManifest};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn foreign_stems() {
    assert!(foreign_dest("dxgi.dll"));
    assert!(foreign_dest("D3D12.DLL"));
    assert!(foreign_dest("SpecialK.dll"));
    assert!(foreign_dest("sub/winmm.dll"));
    assert!(!foreign_dest("ReShade64.dll"));
    assert!(!foreign_dest("Example.addon64"));
    assert!(!foreign_dest("OptiScaler.ini"));
}

fn test_manifest(game: &str, instance: &str, dest: &str, sha: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![crate::download::PlannedFile {
            source: "cache/k/f#x".into(),
            dest: dest.into(),
            sha256: sha.into(),
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}

fn setup(game: &str) -> (PathBuf, PathBuf, PathBuf) {
    let data = std::env::temp_dir().join(format!(
        "tuxgt-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&data);
    let stage = crate::stage::stage_dir(&data, game, "reshade");
    fs::create_dir_all(&stage).unwrap();
    let root = data.join("gamedir");
    fs::create_dir_all(&root).unwrap();
    (data, stage, root)
}

#[test]
fn backup_first_wins_and_identical_skips() {
    use crate::sha256_file;
    let game = "manual:standalone:eeee0001";
    let (data, stage, root) = setup(game);
    fs::write(stage.join("ReShade64.dll"), b"v1").unwrap();
    let h1 = sha256_file(&stage.join("ReShade64.dll")).unwrap();
    // Foreign original present.
    fs::write(root.join("ReShade64.dll"), b"orig").unwrap();
    let mut m = test_manifest(game, "reshade", "ReShade64.dll", &h1);
    let tracked = BTreeMap::new();
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked,
    )
    .unwrap();
    assert!(!ops[0].needs_confirm); // ReShade64 is not a foreign stem
    apply_copies(&data, game, &mut m, &root, None, &ops, &tracked).unwrap();
    assert_eq!(fs::read(root.join("ReShade64.dll")).unwrap(), b"v1");
    assert_eq!(m.backups.len(), 1);
    let first_backup = m.backups["ReShade64.dll"].clone();
    assert_eq!(fs::read(data.join(&first_backup)).unwrap(), b"orig");
    // Update to v2: backup entry stays the original.
    fs::write(stage.join("ReShade64.dll"), b"v2").unwrap();
    let h2 = sha256_file(&stage.join("ReShade64.dll")).unwrap();
    m.files[0].sha256 = h2.clone();
    let mut tracked2 = BTreeMap::new();
    tracked2.insert("ReShade64.dll".into(), h1.clone());
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked2,
    )
    .unwrap();
    apply_copies(&data, game, &mut m, &root, None, &ops, &tracked2).unwrap();
    assert_eq!(m.backups["ReShade64.dll"], first_backup);
    assert_eq!(fs::read(data.join(&first_backup)).unwrap(), b"orig");
    // Re-install of identical bytes: no new backup, same entry.
    let mut tracked3 = BTreeMap::new();
    tracked3.insert("ReShade64.dll".into(), h2.clone());
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked3,
    )
    .unwrap();
    assert!(!ops[0].needs_confirm);
    apply_copies(&data, game, &mut m, &root, None, &ops, &tracked3).unwrap();
    assert_eq!(m.backups["ReShade64.dll"], first_backup);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn remove_restores_deletes_and_leaves_foreign() {
    let game = "manual:standalone:eeee0002";
    let (data, stage, root) = setup(game);
    fs::write(stage.join("dxgi.dll"), b"ours").unwrap();
    let h_ours = crate::sha256_file(&stage.join("dxgi.dll")).unwrap();
    fs::write(stage.join("other.dll"), b"other").unwrap();
    let h_other = crate::sha256_file(&stage.join("other.dll")).unwrap();
    let mut m = FileManifest {
        game: game.into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "dxgi.dll".into(),
                sha256: h_ours.clone(),
                enabled: true,
            },
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "other.dll".into(),
                sha256: h_other.clone(),
                enabled: true,
            },
        ]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    // dxgi: foreign original backed up; other: fresh install, no backup.
    fs::write(root.join("dxgi.dll"), b"foreign-orig").unwrap();
    let tracked = BTreeMap::new();
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked,
    )
    .unwrap();
    assert!(
        ops.iter()
            .find(|o| o.dest == "dxgi.dll")
            .unwrap()
            .needs_confirm
    );
    apply_copies(&data, game, &mut m, &root, None, &ops, &tracked).unwrap();
    // A planned dest missing from staging fails the plan, not the disk.
    let mut files = std::mem::take(&mut m.files).into_vec();
    files.push(crate::download::PlannedFile {
        source: "s".into(),
        dest: "gone.dll".into(),
        sha256: "x".into(),
        enabled: true,
    });
    m.files = files.into_boxed_slice();
    assert!(plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked
    )
    .is_err());
    m.files = std::mem::take(&mut m.files)
        .into_vec()
        .into_iter()
        .filter(|f| f.dest != "gone.dll")
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let mut tracked = BTreeMap::new();
    tracked.insert("dxgi.dll".into(), h_ours.clone());
    tracked.insert("other.dll".into(), h_other.clone());
    // User overwrites other.dll after install: foreign now.
    fs::write(root.join("other.dll"), b"user-edit").unwrap();
    let out = remove_copies(
        &data,
        &mut m.backups,
        m.files.iter().map(|f| f.dest.as_str()),
        &root,
        None,
        &tracked,
    )
    .unwrap();
    assert_eq!(out.restored, vec!["dxgi.dll".to_string()]);
    assert_eq!(fs::read(root.join("dxgi.dll")).unwrap(), b"foreign-orig");
    assert!(out.deleted.is_empty());
    assert_eq!(out.left_foreign, vec!["other.dll".to_string()]);
    assert!(root.join("other.dll").is_file());
    assert!(m.backups.is_empty());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn plan_rejects_traversal() {
    let game = "manual:standalone:eeee0003";
    let (data, stage, root) = setup(game);
    fs::write(stage.join("x.dll"), b"x").unwrap();
    let tracked = BTreeMap::new();
    for dest in ["../evil.dll", "/abs.dll", "a\\b.dll"] {
        let m = test_manifest(game, "reshade", dest, "h");
        assert!(
            plan_copies(
                &data,
                game,
                "reshade",
                m.files.iter(),
                &root,
                None,
                &tracked
            )
            .is_err(),
            "{dest}"
        );
    }
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn prefix_rel_accepts_two_roots() {
    assert_eq!(
        prefix_rel("pfx:windows/system32/foo.dll").unwrap(),
        "windows/system32/foo.dll"
    );
    assert_eq!(
        prefix_rel("pfx:windows/syswow64/bar.dll").unwrap(),
        "windows/syswow64/bar.dll"
    );
    assert!(is_prefix_dest("pfx:windows/system32/foo.dll"));
    assert!(!is_prefix_dest("foo.dll"));
}

#[test]
fn prefix_rel_rejects_bad_roots() {
    for dest in [
        "pfx:",
        "pfx:windows/system32/",
        "pfx:windows/system32",
        "pfx:windows/system33/foo.dll",
        "pfx:WINDOWS/system32/foo.dll",
        "pfx:windows/system32/../evil.dll",
        "pfx:windows/system32/a\\b.dll",
        "pfx:windows/system32/a//b.dll",
        "pfx:windows/system32/a:b.dll",
        "pfx:windows/system32/foo.dll/",
        "foo.dll",
    ] {
        assert!(prefix_rel(dest).is_err(), "{dest}");
    }
    assert!(validate_prefix_dests(["foo.dll"].into_iter(), "install").is_ok());
    assert!(validate_prefix_dests(["foo.dll"].into_iter(), "preload").is_ok());
}

#[test]
fn prefix_forbidden_stems_are_install_errors() {
    for dest in [
        "pfx:windows/system32/dxgi.dll",
        "pfx:windows/syswow64/dxgi.dll",
        "pfx:windows/system32/d3d11.dll",
        "pfx:windows/system32/D3D12.DLL",
        "pfx:windows/system32/winmm.dll",
    ] {
        let err = validate_prefix_dests([dest].into_iter(), "install").unwrap_err();
        assert!(matches!(err, Error::InvalidInstance(_)), "{dest}: {err}");
    }
    // Preload adapter never takes prefix dests, even innocent ones.
    let err =
        validate_prefix_dests(["pfx:windows/system32/foo.dll"].into_iter(), "preload").unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    assert!(validate_prefix_dests(["pfx:windows/system32/foo.dll"].into_iter(), "install").is_ok());
}

#[test]
fn prefix_copy_backup_restore() {
    let game = "manual:standalone:eeee0004";
    let (data, stage, root) = setup(game);
    let prefix = data.join("pfx");
    let sys32 = prefix.join("drive_c").join("windows").join("system32");
    fs::create_dir_all(&sys32).unwrap();
    let dest = "pfx:windows/system32/foo.dll";
    fs::create_dir_all(stage.join("pfx:windows").join("system32")).unwrap();
    fs::write(stage.join(dest), b"v1").unwrap();
    let h1 = crate::sha256_file(&stage.join(dest)).unwrap();
    fs::write(sys32.join("foo.dll"), b"wine-builtin").unwrap();
    let mut m = test_manifest(game, "reshade", dest, &h1);
    let tracked = BTreeMap::new();
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        Some(&prefix),
        &tracked,
    )
    .unwrap();
    assert!(ops[0].is_prefix);
    assert!(!ops[0].needs_confirm);
    apply_copies(&data, game, &mut m, &root, Some(&prefix), &ops, &tracked).unwrap();
    assert_eq!(fs::read(sys32.join("foo.dll")).unwrap(), b"v1");
    assert_eq!(m.backups.len(), 1);
    assert_eq!(
        fs::read(data.join(&m.backups[dest])).unwrap(),
        b"wine-builtin"
    );
    // Uninstall restores the Wine builtin.
    let mut tracked2 = BTreeMap::new();
    tracked2.insert(dest.into(), h1.clone());
    let out = remove_copies(
        &data,
        &mut m.backups,
        [dest].into_iter(),
        &root,
        Some(&prefix),
        &tracked2,
    )
    .unwrap();
    assert_eq!(out.restored, vec![dest.to_string()]);
    assert_eq!(fs::read(sys32.join("foo.dll")).unwrap(), b"wine-builtin");
    assert!(m.backups.is_empty());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn prefix_drive_c_layouts() {
    // WINEPREFIX shape (Heroic): drive_c sits directly under the prefix.
    assert_eq!(
        prefix_drive_c(Path::new("/prefixes/game")),
        PathBuf::from("/prefixes/game/drive_c")
    );
    // A stored path already ending in `pfx` is the Wine root itself.
    assert_eq!(
        prefix_drive_c(Path::new("/steam/steamapps/compatdata/814380/pfx")),
        PathBuf::from("/steam/steamapps/compatdata/814380/pfx/drive_c")
    );
    // Steam compatdata layout: the Wine root hangs one level down.
    assert_eq!(
        prefix_drive_c(Path::new("/steam/steamapps/compatdata/814380")),
        PathBuf::from("/steam/steamapps/compatdata/814380/pfx/drive_c")
    );
}

#[test]
fn prefix_copy_lands_under_steam_pfx_drive_c() {
    let game = "manual:standalone:eeee0006";
    let (data, stage, root) = setup(game);
    // Steam layout: compatdata/<appid>/pfx/drive_c/windows/system32.
    let compat = data.join("steamapps").join("compatdata").join("814380");
    let sys32 = compat
        .join("pfx")
        .join("drive_c")
        .join("windows")
        .join("system32");
    fs::create_dir_all(&sys32).unwrap();
    let dest = "pfx:windows/system32/foo.dll";
    fs::create_dir_all(stage.join("pfx:windows").join("system32")).unwrap();
    fs::write(stage.join(dest), b"v1").unwrap();
    let h1 = crate::sha256_file(&stage.join(dest)).unwrap();
    fs::write(sys32.join("foo.dll"), b"wine-builtin").unwrap();
    let mut m = test_manifest(game, "reshade", dest, &h1);
    let tracked = BTreeMap::new();
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        Some(&compat),
        &tracked,
    )
    .unwrap();
    assert!(ops[0].is_prefix);
    apply_copies(&data, game, &mut m, &root, Some(&compat), &ops, &tracked).unwrap();
    assert_eq!(fs::read(sys32.join("foo.dll")).unwrap(), b"v1");
    assert!(compat.join("drive_c").symlink_metadata().is_err());
    assert_eq!(
        fs::read(data.join(&m.backups[dest])).unwrap(),
        b"wine-builtin"
    );
    // Uninstall restores the Wine builtin in the Steam tree.
    let mut tracked2 = BTreeMap::new();
    tracked2.insert(dest.into(), h1.clone());
    let out = remove_copies(
        &data,
        &mut m.backups,
        [dest].into_iter(),
        &root,
        Some(&compat),
        &tracked2,
    )
    .unwrap();
    assert_eq!(out.restored, vec![dest.to_string()]);
    assert_eq!(fs::read(sys32.join("foo.dll")).unwrap(), b"wine-builtin");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn prefix_without_prefix_is_install_error() {
    let game = "manual:standalone:eeee0005";
    let (data, stage, root) = setup(game);
    let dest = "pfx:windows/system32/foo.dll";
    fs::create_dir_all(stage.join("pfx:windows").join("system32")).unwrap();
    fs::write(stage.join(dest), b"v1").unwrap();
    let h1 = crate::sha256_file(&stage.join(dest)).unwrap();
    let m = test_manifest(game, "reshade", dest, &h1);
    let tracked = BTreeMap::new();
    let err = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        &tracked,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Install(_)), "{err}");
    let _ = fs::remove_dir_all(&data);
}
