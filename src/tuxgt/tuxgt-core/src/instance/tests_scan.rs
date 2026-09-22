use super::testing::*;
use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::Ordering;

#[test]
fn scan_package_keeps_extras_and_type_dests() {
    let (root, pkg) = scratch_pkg();
    let files = scan_package(&pkg, "optiscaler", &root).unwrap();
    let by: BTreeMap<_, _> = files.iter().map(|f| (f.src.as_str(), f)).collect();
    assert!(by["OptiScaler.dll"].keep);
    assert_eq!(by["OptiScaler.dll"].dest, "dxgi.dll");
    assert!(by["extra.ini"].keep);
    assert_eq!(by["extra.ini"].dest, "extra.ini");
    assert!(!by["setup.bat"].keep);
    assert!(!by["notes.md"].keep);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn package_support_files_start_unchecked_and_can_be_reselected() {
    let (root, pkg) = scratch_pkg();
    fs::create_dir_all(pkg.join("docs")).unwrap();
    for (path, bytes) in [
        ("setup-linux.sh", "sh"),
        ("notes.txt", "txt"),
        ("docs/readMe.md", "md"),
        ("docs/LICENSE", "license"),
        ("docs/license-MIT.txt", "license txt"),
        ("docs/keep.cfg", "cfg"),
        ("setup.BAT", "bat"),
        ("keys.reg", "reg"),
    ] {
        fs::write(pkg.join(path), bytes).unwrap();
    }

    for mod_type in ["custom", "effect", "optiscaler"] {
        let files = scan_package(&pkg, mod_type, &root).unwrap();
        let by: BTreeMap<_, _> = files.iter().map(|f| (f.src.as_str(), f)).collect();
        assert!(!by["setup-linux.sh"].keep, "{mod_type}");
        assert!(!by["setup.BAT"].keep, "{mod_type}");
        assert!(!by["keys.reg"].keep, "{mod_type}");
        assert!(!by["notes.txt"].keep, "{mod_type}");
        assert!(!by["docs/readMe.md"].keep, "{mod_type}");
        assert!(!by["docs/LICENSE"].keep, "{mod_type}");
        assert!(!by["docs/license-MIT.txt"].keep, "{mod_type}");
        assert!(by["docs/keep.cfg"].keep, "{mod_type}");
    }

    let mut files = scan_package(&pkg, "custom", &root).unwrap();
    files
        .iter_mut()
        .find(|f| f.src == "notes.txt")
        .unwrap()
        .keep = true;
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    let inst = add_mod_from(
        &cfg,
        "custom",
        "custom-support-files",
        &pkg,
        None,
        Some(&files),
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert!(inst.payload[0].keep.iter().any(|k| k == "notes.txt"));
    let (_, rescanned) = rescan_preview(&cfg, "custom-support-files", &root).unwrap();
    assert!(rescanned.iter().any(|f| f.src == "notes.txt" && f.keep));
    let _ = fs::remove_dir_all(&root);
}
#[test]
fn reshade_wrapper_prefix_cases() {
    let dual = vec![
        "Photo/Photo/preset.ini".to_string(),
        "Photo/Photo/reshade-shaders/Shaders/x.fx".to_string(),
        "Photo/Photo/reshade-shaders/Textures/n.png".to_string(),
    ];
    assert_eq!(
        reshade_wrapper_prefix(&dual),
        Some("Photo/Photo/".to_string())
    );
    let rooted = vec![
        "preset.ini".to_string(),
        "reshade-shaders/Shaders/x.fx".to_string(),
    ];
    assert_eq!(reshade_wrapper_prefix(&rooted), None);
    let direct = vec!["Shaders/x.fx".to_string(), "Textures/n.png".to_string()];
    assert_eq!(reshade_wrapper_prefix(&direct), None);
    let none: Vec<String> = vec!["a.dll".to_string(), "b.txt".to_string()];
    assert_eq!(reshade_wrapper_prefix(&none), None);
    let empty: Vec<String> = Vec::new();
    assert_eq!(reshade_wrapper_prefix(&empty), None);
}

#[test]
fn rebase_reshade_srcs_friend_zip_shape() {
    let mut files = vec![
        "Photo/Photo/preset.ini".to_string(),
        "Photo/Photo/reshade-shaders/Shaders/Clarity.fx".to_string(),
        "Photo/Photo/reshade-shaders/Shaders/qUINT/qUINT_common.fxh".to_string(),
    ];
    rebase_reshade_srcs(&mut files);
    assert_eq!(
        files,
        vec![
            "preset.ini".to_string(),
            "reshade-shaders/Shaders/Clarity.fx".to_string(),
            "reshade-shaders/Shaders/qUINT/qUINT_common.fxh".to_string(),
        ]
    );
    // Idempotent.
    rebase_reshade_srcs(&mut files);
    assert_eq!(files.len(), 3);
}

#[test]
fn strip_to_reshade_root_fs_rebases_dual_wrapper() {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tuxgt-reshade-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let nested = dir.join("W").join("W");
    fs::create_dir_all(nested.join("reshade-shaders").join("Shaders")).unwrap();
    fs::write(nested.join("preset.ini"), b"p").unwrap();
    fs::write(
        nested.join("reshade-shaders").join("Shaders").join("x.fx"),
        b"f",
    )
    .unwrap();
    strip_to_reshade_root_fs(&dir).unwrap();
    assert!(dir.join("preset.ini").is_file());
    assert!(dir
        .join("reshade-shaders")
        .join("Shaders")
        .join("x.fx")
        .is_file());
    assert!(!dir.join("W").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn strip_to_reshade_root_fs_keeps_shaders_direct() {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tuxgt-shaders-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("Shaders")).unwrap();
    fs::write(dir.join("Shaders").join("x.fx"), b"f").unwrap();
    strip_to_reshade_root_fs(&dir).unwrap();
    assert!(dir.join("Shaders").join("x.fx").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn add_mod_from_dual_nested_effect_roundtrip() {
    let (root, _) = scratch_pkg();
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    let src = root.join("friend");
    let nested = src.join("W").join("W");
    fs::create_dir_all(nested.join("reshade-shaders").join("Shaders")).unwrap();
    fs::write(nested.join("preset.ini"), b"p").unwrap();
    fs::write(
        nested.join("reshade-shaders").join("Shaders").join("x.fx"),
        b"f",
    )
    .unwrap();
    let inst = add_mod_from(
        &cfg,
        "effect",
        "friend-pack",
        &src,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let keep = &inst.payload[0].keep;
    assert!(keep.contains(&"preset.ini".to_string()), "{keep:?}");
    assert!(
        keep.contains(&"reshade-shaders/Shaders/x.fx".to_string()),
        "{keep:?}"
    );
    assert!(inst.dests.is_empty(), "{:?}", inst.dests);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn add_from_writes_keep_and_dests_no_shadow() {
    let (root, pkg) = scratch_pkg();
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    assert!(add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler",
        &pkg,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new()
    )
    .is_err());
    let inst = add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        Some("Custom"),
        None,
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(inst.id, "optiscaler-custom");
    assert!(!inst.official);
    assert_eq!(inst.label, "Custom");
    assert!(matches!(&inst.source, SourceRef::Local { .. }));
    assert!(inst.payload[0].keep.iter().any(|k| k == "OptiScaler.dll"));
    assert!(inst.payload[0].keep.iter().any(|k| k == "extra.ini"));
    assert!(!inst.payload[0].keep.iter().any(|k| k.ends_with(".bat")));
    assert_eq!(
        inst.dests.get("OptiScaler.dll").map(String::as_str),
        Some("dxgi.dll")
    );
    assert!(!inst.dests.contains_key("extra.ini"));
    let text = fs::read_to_string(user_mods_dir(&root).join("optiscaler-custom.toml")).unwrap();
    assert!(text.contains("[dests]"));
    assert!(text.contains("source"));
    assert!(!text.contains("proton_env"));
    assert!(add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new()
    )
    .is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn add_from_rejects_duplicate_optiscaler_dests() {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("tuxgt-e49-two-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&root);
    let pkg = root.join("pkg");
    fs::create_dir_all(pkg.join("a")).unwrap();
    fs::create_dir_all(pkg.join("b")).unwrap();
    fs::write(pkg.join("a").join("OptiScaler.dll"), b"a").unwrap();
    fs::write(pkg.join("b").join("OptiScaler.dll"), b"b").unwrap();
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    let err = add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("duplicate dest"), "{err}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rescan_user_only_and_marks_new_files() {
    let (root, pkg) = scratch_pkg();
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let stored = user_mods_dir(&root).join("optiscaler-custom");
    fs::write(stored.join("new.bin"), b"n").unwrap();
    let (prev, files) = rescan_preview(&cfg, "optiscaler-custom", &root).unwrap();
    assert_eq!(prev.id, "optiscaler-custom");
    let by: BTreeMap<_, _> = files.iter().map(|f| (f.src.as_str(), f)).collect();
    assert!(by["OptiScaler.dll"].keep && !by["OptiScaler.dll"].is_new);
    assert!(by["new.bin"].keep && by["new.bin"].is_new);
    assert!(!by["setup.bat"].keep && !by["setup.bat"].is_new);
    assert!(!by["notes.md"].keep && !by["notes.md"].is_new);
    rescan_mod(&cfg, "optiscaler-custom", None, None, &root).unwrap();
    let listed = list_mods(&cfg, &root).unwrap();
    let inst = listed
        .mods
        .iter()
        .find(|i| i.id == "optiscaler-custom")
        .unwrap();
    assert!(inst.payload[0].keep.iter().any(|k| k == "new.bin"));
    assert!(rescan_preview(&cfg, "optiscaler", &root).is_err());
    assert!(rescan_mod(&cfg, "optiscaler", None, None, &root).is_err());
    let _ = fs::remove_dir_all(&root);
}
