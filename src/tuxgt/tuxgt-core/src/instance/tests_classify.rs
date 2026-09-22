use super::testing::*;
use super::*;
use crate::{tool_status, ExtTool};
use std::fs;

#[test]
fn classify_single_files() {
    let root = temp_config().join("class1");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("plug.dll"), b"dll").unwrap();
    fs::write(root.join("notes.txt"), b"txt").unwrap();
    fs::write(root.join("mod.addon64"), b"addon").unwrap();
    let dll = classify_package(&root.join("plug.dll")).unwrap();
    assert!(dll.temps.is_empty());
    assert!(matches!(
        dll.kind,
        ClassifyKind::Single {
            injectable: true,
            ..
        }
    ));
    let txt = classify_package(&root.join("notes.txt")).unwrap();
    assert!(matches!(
        txt.kind,
        ClassifyKind::Single {
            injectable: false,
            ..
        }
    ));
    let addon = classify_package(&root.join("mod.addon64")).unwrap();
    assert!(matches!(
        addon.kind,
        ClassifyKind::Single {
            injectable: true,
            ..
        }
    ));
    assert!(classify_package(&root.join("nope.dll")).is_err());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn classify_recipe_file_and_unparseable_toml() {
    let root = temp_config().join("class2");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("good.toml"),
            "id = \"cls\"\ntype = \"custom\"\nlabel = \"cls\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
        )
        .unwrap();
    let c = classify_package(&root.join("good.toml")).unwrap();
    assert!(matches!(c.kind, ClassifyKind::Recipe { .. }));
    fs::write(root.join("bad.toml"), "id = \"broken\"\ntype = [1]\n").unwrap();
    let b = classify_package(&root.join("bad.toml")).unwrap();
    assert!(matches!(
        b.kind,
        ClassifyKind::Single {
            injectable: false,
            ..
        }
    ));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn classify_folders_follow_tree() {
    let root = temp_config().join("class3");
    let one = root.join("one");
    fs::create_dir_all(&one).unwrap();
    fs::write(one.join("solo.dll"), b"dll").unwrap();
    let c = classify_package(&one).unwrap();
    assert!(matches!(
        c.kind,
        ClassifyKind::Single {
            injectable: true,
            ..
        }
    ));
    let with_recipe = root.join("wr");
    fs::create_dir_all(&with_recipe).unwrap();
    fs::write(
            with_recipe.join("r.toml"),
            "id = \"cls\"\ntype = \"custom\"\nlabel = \"cls\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
        )
        .unwrap();
    fs::write(with_recipe.join("payload.dll"), b"dll").unwrap();
    let c = classify_package(&with_recipe).unwrap();
    assert!(matches!(c.kind, ClassifyKind::Recipe { .. }));
    fs::write(
            with_recipe.join("s.toml"),
            "id = \"cls2\"\ntype = \"custom\"\nlabel = \"cls2\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
        )
        .unwrap();
    assert!(classify_package(&with_recipe).is_err());
    let multi = root.join("multi");
    fs::create_dir_all(&multi).unwrap();
    fs::write(multi.join("a.dll"), b"a").unwrap();
    fs::write(multi.join("b.txt"), b"b").unwrap();
    let c = classify_package(&multi).unwrap();
    assert!(matches!(c.kind, ClassifyKind::Folder { .. }));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn classify_password_archive_inside_folder_uses_password() {
    if !tool_status(&ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return;
    }
    let root = temp_config().join("class-password-folder");
    let folder = root.join("folder");
    fs::create_dir_all(&folder).unwrap();
    let source = root.join("a.dll");
    fs::write(&source, b"payload").unwrap();
    let archive = folder.join("protected.zip");
    let created = std::process::Command::new("7z")
        .args([
            "a",
            "-tzip",
            "-psecret",
            archive.to_str().unwrap(),
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    assert!(matches!(
        classify_package_with_password(&folder, None),
        Err(crate::Error::ArchivePasswordRequired)
    ));
    let classified = classify_package_with_password(&folder, Some("secret")).unwrap();
    assert!(matches!(classified.kind, ClassifyKind::Single { .. }));
    for temp in classified.temps {
        let _ = fs::remove_dir_all(temp);
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn classify_archive_unpacks_to_folder_rules() {
    if !std::process::Command::new("tar")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    let root = temp_config().join("class4");
    let _ = fs::remove_dir_all(&root);
    let pkg = root.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("a.dll"), b"a").unwrap();
    fs::write(pkg.join("b.txt"), b"b").unwrap();
    let archive = root.join("pack.tar.gz");
    let st = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&pkg)
        .arg(".")
        .output()
        .unwrap();
    assert!(st.status.success());
    let c = classify_package(&archive).unwrap();
    assert!(matches!(c.kind, ClassifyKind::Folder { .. }));
    assert_eq!(c.temps.len(), 1);
    assert!(c.temps[0].is_dir());
    // The Add flow scans the unpacked dir before dropping temps: the
    // scan must succeed on classify output.
    let dir = match &c.kind {
        ClassifyKind::Folder { dir } => dir.clone(),
        other => panic!("{other:?}"),
    };
    let scanned = scan_package(&dir, "custom", &root).unwrap();
    let srcs: Vec<&str> = scanned.iter().map(|f| f.src.as_str()).collect();
    assert!(
        srcs.contains(&"a.dll") && srcs.contains(&"b.txt"),
        "{srcs:?}"
    );
    for t in &c.temps {
        fs::remove_dir_all(t).unwrap();
    }
    assert!(!c.temps[0].exists());
    let _ = fs::remove_dir_all(&root);
}
#[test]
fn classify_scan_matches_archive_add_for_dual_nested() {
    if !std::process::Command::new("tar")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    // The failing photo-zip shape: a doubled top-level dir holding a
    // preset plus a reshade-shaders tree.
    let root = temp_config();
    let _ = fs::remove_dir_all(root.join("Photo"));
    let outer = root.join("Photo").join("Photo");
    fs::create_dir_all(outer.join("reshade-shaders").join("Shaders")).unwrap();
    fs::write(outer.join("preset.ini"), b"p").unwrap();
    fs::write(outer.join("ReadMe.txt"), b"r").unwrap();
    fs::write(
        outer.join("reshade-shaders").join("Shaders").join("x.fx"),
        b"f",
    )
    .unwrap();
    let archive = root.join("photo.tgz");
    let st = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&root)
        .arg("Photo")
        .output()
        .unwrap();
    assert!(st.status.success());
    // The Add flow scans the classify temp dir, then Saves from the
    // archive: checked srcs must agree and match the stored payload.
    let cls = classify_package(&archive).unwrap();
    let dir = match &cls.kind {
        ClassifyKind::Folder { dir } => dir.clone(),
        other => panic!("{other:?}"),
    };
    let scanned = scan_package(&dir, "effect", &root).unwrap();
    for t in &cls.temps {
        fs::remove_dir_all(t).unwrap();
    }
    let cfg = root.join("cfg");
    fs::create_dir_all(&cfg).unwrap();
    let inst = add_mod_from(
        &cfg,
        "effect",
        "photo-pack",
        &archive,
        None,
        None,
        &root,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let mut got: Vec<&str> = scanned
        .iter()
        .filter(|f| f.keep)
        .map(|f| f.src.as_str())
        .collect();
    got.sort();
    let mut keep: Vec<&str> = inst.payload[0].keep.iter().map(String::as_str).collect();
    keep.sort();
    assert_eq!(got, keep, "{got:?} vs {keep:?}");
    let readme = scanned.iter().find(|f| f.src == "ReadMe.txt").unwrap();
    assert!(!readme.keep);
    let stored = user_mods_dir(&root).join("photo-pack");
    for k in &inst.payload[0].keep {
        assert!(stored.join(k).is_file(), "{k}");
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn recipe_import_copies_local_payload() {
    let dir = temp_config();
    let data = dir.join("data");
    fs::create_dir_all(&data).unwrap();
    let orig = dir.join("orig");
    fs::create_dir_all(&orig).unwrap();
    fs::write(orig.join("plug.dll"), b"dll").unwrap();
    let recipe = format!(
            "id = \"copied\"\ntype = \"custom\"\nlabel = \"copied\"\n[source]\ntype = \"local\"\npath = \"{}\"\n",
            orig.to_str().unwrap()
        );
    let src = dir.join("copied.toml");
    fs::write(&src, recipe).unwrap();
    let m = add_mod(&dir, &src, &data).unwrap();
    let stored = user_mods_dir(&data).join("copied");
    let sp = match &m.source {
        SourceRef::Local { path } => path.as_str(),
        other => panic!("{other:?}"),
    };
    assert_eq!(sp, stored.to_str().unwrap());
    assert!(stored.join("plug.dll").is_file());
    let text = fs::read_to_string(user_mods_dir(&data).join("copied.toml")).unwrap();
    assert!(!text.contains(orig.to_str().unwrap()), "{text}");
    // Deleting the original still rescans from the copy.
    fs::remove_dir_all(&orig).unwrap();
    let re = rescan_mod(&dir, "copied", None, None, &data).unwrap();
    assert!(re.payload[0].keep.iter().any(|k| k == "plug.dll"));
    // Removing the mod drops its payload copy too.
    remove_mod(&dir, &data, "copied").unwrap();
    assert!(!stored.exists());
}
#[test]
fn add_from_roundtrips_include() {
    let dir = temp_config();
    let pkg = dir.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    let m = add_mod_from(
        &dir,
        "custom",
        "incmod",
        &pkg,
        None,
        None,
        &dir,
        vec!["plug.dll".into()],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(&m.include[..], ["plug.dll"]);
    let text = fs::read_to_string(user_mods_dir(&dir).join("incmod.toml")).unwrap();
    assert!(text.contains("include = [\"plug.dll\"]"), "{text}");
    let re = rescan_mod(&dir, "incmod", None, None, &dir).unwrap();
    assert_eq!(&re.include[..], ["plug.dll"]);
    // The GUI Rescan form's include dests replace the recipe's (`Some`),
    // and the CLI rescan after one (`None`) keeps what the recipe holds.
    let re = rescan_mod(&dir, "incmod", None, Some(&[]), &dir).unwrap();
    assert!(re.include.is_empty());
    let text = fs::read_to_string(user_mods_dir(&dir).join("incmod.toml")).unwrap();
    let back = parse_recipe(&text, false).unwrap();
    assert!(back.include.is_empty(), "{text}");
    let re = rescan_mod(&dir, "incmod", None, Some(&["plug.dll".into()]), &dir).unwrap();
    assert_eq!(&re.include[..], ["plug.dll"]);
    let re = rescan_mod(&dir, "incmod", None, None, &dir).unwrap();
    assert_eq!(&re.include[..], ["plug.dll"]);
}
