use super::testing::*;
use super::*;
use crate::{tool_status, ExtTool};
use std::fs;
use std::sync::atomic::Ordering;

#[test]
fn rescan_does_not_touch_manifests() {
    let (root, pkg) = scratch_pkg();
    let cfg = root.join("cfg");
    let data = root.join("data");
    fs::create_dir_all(&cfg).unwrap();
    fs::create_dir_all(&data).unwrap();
    add_mod_from(
        &cfg,
        "optiscaler",
        "optiscaler-custom",
        &pkg,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let man = crate::FileManifest {
        game: "manual:standalone:x".into(),
        instance: "optiscaler-custom".into(),
        mod_type: "optiscaler".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![crate::PlannedFile {
            source: "x".into(),
            dest: "old.dll".into(),
            sha256: "00".into(),
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    };
    crate::write_manifest(&data, &man).unwrap();
    fs::write(pkg.join("later.bin"), b"l").unwrap();
    rescan_mod(&cfg, "optiscaler-custom", None, None, &data).unwrap();
    let back = crate::read_manifest(&data, "manual:standalone:x", "optiscaler-custom")
        .unwrap()
        .unwrap();
    assert_eq!(back.files.len(), 1);
    assert_eq!(back.files[0].dest, "old.dll");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn scan_strips_single_top_dir() {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("tuxgt-e49-strip-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&root);
    let inner = root.join("OptiScaler_v3");
    fs::create_dir_all(&inner).unwrap();
    fs::write(inner.join("OptiScaler.dll"), b"dll").unwrap();
    let files = scan_package(&root, "optiscaler", &root).unwrap();
    assert!(files.iter().any(|f| f.src == "OptiScaler.dll"));
    assert!(!files.iter().any(|f| f.src.contains("OptiScaler_v3")));
    let _ = fs::remove_dir_all(&root);
}
#[test]
fn slot_and_include_parse_and_validate() {
    let ok = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\nslot = \"dxgi\"\ninclude = [\"foo.dll\"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(ok.slot.as_deref(), Some("dxgi"));
    assert_eq!(&ok.include[..], ["foo.dll"]);
    let bad_slot = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\nslot = \"dinput8\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        );
    assert!(bad_slot.is_err());
    let bad_include = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\ninclude = [\"\"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        );
    assert!(bad_include.is_err());
}

#[test]
fn slot_infers_from_remap_dests() {
    let explicit = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\nslot = \"winmm\"\n[dests]\n\"a.dll\" = \"dxgi.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(explicit.slot.as_deref(), Some("winmm"));
    let inferred = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[dests]\n\"a.dll\" = \"dxgi.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(inferred.slot.as_deref(), Some("dxgi"));
    let unknown = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[dests]\n\"a.dll\" = \"mydriver.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(unknown.slot, None);
    let conflict = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[dests]\n\"a.dll\" = \"dxgi.dll\"\n\"b.dll\" = \"d3d12.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(conflict.slot, None);
    let forced = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\ninclude = [\"dxgi.dll\"]\n[dests]\n\"a.dll\" = \"dxgi.dll\"\n\"b.dll\" = \"version.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(forced.slot.as_deref(), Some("version"));
    // Subdir dests never vote even when slot-named.
    let subdir = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[dests]\n\"a.dll\" = \"bin/dxgi.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(subdir.slot, None);
}

#[test]
fn add_from_roundtrips_inferred_slot() {
    let dir = temp_config();
    let pkg = dir.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    let mut files = scan_package(&pkg, "custom", &dir).unwrap();
    files[0].dest = "dxgi.dll".into();
    let m = add_mod_from(
        &dir,
        "custom",
        "slotmod",
        &pkg,
        None,
        Some(&files),
        &dir,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(m.slot.as_deref(), Some("dxgi"));
    let text = fs::read_to_string(user_mods_dir(&dir).join("slotmod.toml")).unwrap();
    assert!(text.contains("slot = \"dxgi\""), "{text}");
    let re = rescan_mod(&dir, "slotmod", None, None, &dir).unwrap();
    assert_eq!(re.slot.as_deref(), Some("dxgi"));
}

#[test]
fn add_from_password_archive_roundtrips() {
    if !tool_status(&ExtTool {
        name: "7z",
        probe: &["7z"],
    })
    .found
    {
        return;
    }
    let dir = temp_config();
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("plug.dll"), b"protected payload").unwrap();
    let archive = dir.join("protected.zip");
    let created = std::process::Command::new("7z")
        .args([
            "a",
            "-tzip",
            "-psecret",
            archive.to_str().unwrap(),
            src.join("plug.dll").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    let mod_inst = add_mod_from_with_password(
        &dir,
        "custom",
        "protected-pack",
        &archive,
        None,
        None,
        &dir,
        Vec::new(),
        Vec::new(),
        Some("secret"),
    )
    .unwrap();
    assert_eq!(mod_inst.id, "protected-pack");
    assert_eq!(
        fs::read(user_mods_dir(&dir).join("protected-pack").join("plug.dll")).unwrap(),
        b"protected payload"
    );
    let recipe = fs::read_to_string(user_mods_dir(&dir).join("protected-pack.toml")).unwrap();
    assert!(!recipe.contains("secret"), "password must not be persisted");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scan_companions_default_include() {
    let file = |src: &str, dest: &str, keep: bool| PackageFile {
        src: src.into(),
        dest: dest.into(),
        keep,
        is_new: false,
    };
    // OptiScaler claiming dest stays Load; companions default Include.
    let files = vec![
        file("OptiScaler.dll", "dxgi.dll", true),
        file("nvapi64.dll", "nvapi64.dll", true),
        file("OptiScaler.ini", "OptiScaler.ini", true),
    ];
    assert_eq!(
        scan_default_include("optiscaler", &files),
        vec!["nvapi64.dll".to_string()]
    );
    // Dropped companions are not defaulted.
    let files = vec![
        file("OptiScaler.dll", "dxgi.dll", true),
        file("nvapi64.dll", "nvapi64.dll", false),
    ];
    assert!(scan_default_include("optiscaler", &files).is_empty());
    // Single DLL custom pack: nothing to include.
    let files = vec![file("plug.dll", "plug.dll", true)];
    assert!(scan_default_include("custom", &files).is_empty());
    // Non-load types never default include.
    let files = vec![file("x.fx", "reshade-shaders/Shaders/x.fx", true)];
    assert!(scan_default_include("effect", &files).is_empty());
}

#[test]
fn package_slot_rewrites_claiming_dest() {
    let file = |src: &str, dest: &str| PackageFile {
        src: src.into(),
        dest: dest.into(),
        keep: true,
        is_new: false,
    };
    let mut files = vec![
        file("OptiScaler.dll", "dxgi.dll"),
        file("nvapi64.dll", "nvapi64.dll"),
    ];
    assert!(apply_package_slot(&mut files, "winmm").unwrap());
    assert_eq!(files[0].dest, "winmm.dll");
    assert_eq!(files[1].dest, "nvapi64.dll");
    // Subdir companion first never steals the claim from the sibling.
    let mut files = vec![
        file("bin/D3D12Core.dll", "bin/D3D12Core.dll"),
        file("OptiScaler.dll", "dxgi.dll"),
    ];
    assert!(apply_package_slot(&mut files, "winmm").unwrap());
    assert_eq!(files[0].dest, "bin/D3D12Core.dll");
    assert_eq!(files[1].dest, "winmm.dll");
    // Subdir dests never claim the proxy even when slot-named.
    let mut files = vec![file("bin/dxgi.dll", "bin/dxgi.dll")];
    assert!(!apply_package_slot(&mut files, "winmm.dll").unwrap());
    assert_eq!(files[0].dest, "bin/dxgi.dll");
    // No DLL: no claiming dest, untouched.
    let mut files = vec![file("x.addon64", "x.addon64")];
    assert!(!apply_package_slot(&mut files, "winmm").unwrap());
    assert_eq!(files[0].dest, "x.addon64");
    // Unknown slot errors.
    let mut files = vec![file("OptiScaler.dll", "dxgi.dll")];
    assert!(apply_package_slot(&mut files, "dinput8").is_err());
}
