use super::testing::*;
use super::*;
use crate::Error;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn migrate_prefix_moves_legacy_layout() {
    let dir = temp_config();
    let data = dir.join("data");
    let cfg = dir.join("config");
    fs::create_dir_all(data.join("share").join("mods")).unwrap();
    fs::create_dir_all(cfg.join("mods")).unwrap();
    fs::write(
        data.join("share").join("mods").join("reshade.toml"),
        fs::read_to_string(official_mods_dir(&dir).join("reshade.toml")).unwrap(),
    )
    .unwrap();
    fs::write(
            cfg.join("mods").join("mine.toml"),
            "id = \"mine\"\ntype = \"custom\"\nlabel = \"mine\"\n[source]\ntype = \"local\"\npath = \"x\"\n",
        )
        .unwrap();
    let pkg = data.join("packages").join("mine");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    migrate_prefix(&data, &cfg);
    assert!(official_mods_dir(&data).join("reshade.toml").is_file());
    assert!(user_mods_dir(&data).join("mine.toml").is_file());
    assert!(user_mods_dir(&data).join("mine").join("plug.dll").is_file());
    assert!(!data.join("share").join("mods").exists());
    assert!(!cfg.join("mods").exists());
    assert!(!data.join("packages").exists());
}

fn seed_templates(data: &Path) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/templates");
    let dest = data.join("share").join("templates");
    fs::create_dir_all(&dest).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        fs::copy(e.path(), dest.join(e.file_name())).unwrap();
    }
}

#[test]
fn templates_parse_from_fixture_dir() {
    let data = temp_config();
    seed_templates(&data);
    let ts = list_templates(&data).unwrap();
    // E63: the 6 template-form templates + 2 family templates.
    assert_eq!(ts.len(), 8);
    let by: BTreeMap<_, _> = ts.iter().map(|t| (t.id.as_str(), t)).collect();
    assert_eq!(by["reshade-addon"].mod_type, "reshade_addon");
    assert_eq!(by["reshade-addon"].mode, "include");
    assert_eq!(&by["reshade-addon"].requires[..], ["reshade"]);
    assert_eq!(by["reshade-shader"].mod_type, "effect");
    assert_eq!(by["reshade-texture"].mod_type, "texture");
    assert_eq!(by["custom-optiscaler"].mode, "type");
    assert_eq!(by["custom-reshade"].mode, "type");
    assert_eq!(by["custom-blank"].mode, "custom");
    assert!(by["custom-blank"].requires.is_empty());
    let f = by["family-renodx"].family.as_ref().unwrap();
    assert_eq!(f.owner, "clshortfuse");
    assert_eq!(f.repo, "renodx");
    assert_eq!(f.asset_glob, "renodx-*.addon64");
    assert!(f.prerelease);
    assert!(f.drop.is_empty());
    let l = by["family-luma"].family.as_ref().unwrap();
    assert_eq!(l.owner, "Filoppi");
    assert_eq!(l.repo, "Luma-Framework");
    assert_eq!(l.asset_glob, "Luma-*.zip");
    assert!(!l.prerelease);
    assert_eq!(&l.drop[..], ["dxgi.dll"]);
    assert_eq!(by["family-luma"].mod_type, "reshade_addon");
    // Missing dir lists none (fresh prefix before deploy).
    assert!(list_templates(&temp_config()).unwrap().is_empty());
}

#[test]
fn packaged_reload_picks_up_changed_and_gone_files() {
    let data = temp_config();
    assert!(list_mods(&data, &data)
        .unwrap()
        .mods
        .iter()
        .any(|m| m.id == "reshade"));
    // A changed packaged file re-parses on re-list (restart equivalent).
    let reshade = official_mods_dir(&data).join("reshade.toml");
    let text = fs::read_to_string(&reshade)
        .unwrap()
        .replace("label = \"ReShade\"", "label = \"ReShade (edited)\"");
    fs::write(&reshade, text).unwrap();
    let relisted = list_mods(&data, &data).unwrap();
    assert!(relisted.mods.iter().any(|m| m.label == "ReShade (edited)"));
    // A gone file drops from the catalog: no ghost rows.
    fs::remove_file(&reshade).unwrap();
    let gone = list_mods(&data, &data).unwrap();
    assert!(!gone.mods.iter().any(|m| m.id == "reshade"));
}

#[test]
fn export_recipe_only_github_roundtrips_via_add() {
    let data = temp_config();
    let src = data.join("export-gh-src.toml");
    fs::write(&src, sample_recipe("export-gh", "\"preload\", \"install\"")).unwrap();
    let added = add_mod(&data, &src, &data).unwrap();
    assert!(matches!(added.source, SourceRef::Github { .. }));
    let out = data.join("export-gh.toml");
    export_mod(&data, &data, "export-gh", &out, false).unwrap();
    let text = fs::read_to_string(&out).unwrap();
    assert!(text.contains(r#"type = "github""#), "{text}");
    // Fresh prefix: the exported recipe imports as-is (source untouched).
    let fresh = temp_config();
    let back = add_mod(&fresh, &out, &fresh).unwrap();
    assert_eq!(back.id, "export-gh");
    match back.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            assert_eq!(owner, "someone");
            assert_eq!(repo, "OptiScaler-fork");
            assert_eq!(asset_glob, "OptiScaler_*.zip");
            assert_eq!(tag, None);
            assert!(!prerelease);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn export_official_and_unknown_ids_error() {
    let data = temp_config();
    let out = data.join("reshade.toml");
    let err = export_mod(&data, &data, "reshade", &out, false).unwrap_err();
    assert!(err.to_string().contains("official"), "{err}");
    let tar_out = data.join("reshade.tar.gz");
    let err = export_mod(&data, &data, "reshade", &tar_out, true).unwrap_err();
    assert!(err.to_string().contains("official"), "{err}");
    assert!(!out.exists() && !tar_out.exists());
    let err = export_mod(&data, &data, "no-such-mod", &out, false).unwrap_err();
    assert!(matches!(err, Error::UnknownInstance(_)), "{err}");
}

#[test]
fn export_recipe_only_rewrites_local_path_relative() {
    let data = temp_config();
    let pkg = data.join("export-local-pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    let made = add_mod_from(
        &data,
        "custom",
        "export-local",
        &pkg,
        Some("Local"),
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert!(matches!(made.source, SourceRef::Local { .. }));
    let out = data.join("export-local.toml");
    export_mod(&data, &data, "export-local", &out, false).unwrap();
    let text = fs::read_to_string(&out).unwrap();
    // Still parses, and the stored absolute path is now relative.
    let back = parse_recipe(&text, false).unwrap();
    match back.source {
        SourceRef::Local { path } => {
            assert!(!Path::new(&path).is_absolute(), "{path}");
            assert!(path.contains("export-local"), "{path}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn export_files_tar_roundtrips_via_add() {
    let data = temp_config();
    let pkg = data.join("export-tar-pkg");
    fs::create_dir_all(pkg.join("sub")).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    fs::write(pkg.join("sub").join("notes.txt"), b"txt").unwrap();
    add_mod_from(
        &data,
        "custom",
        "export-tar",
        &pkg,
        Some("Tar"),
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let out = data.join("export-tar.tar.gz");
    match export_mod(&data, &data, "export-tar", &out, true) {
        Ok(()) => {}
        Err(Error::MissingTool(name)) => {
            assert_eq!(name, "tar");
            return;
        }
        Err(e) => panic!("unexpected export error: {e}"),
    }
    assert!(out.is_file());
    let ex = data.join("export-tar-ex");
    fs::create_dir_all(&ex).unwrap();
    let st = std::process::Command::new("tar")
        .args(["-xzf", &out.to_string_lossy(), "-C", &ex.to_string_lossy()])
        .status()
        .unwrap();
    assert!(st.success());
    assert!(ex.join("export-tar.toml").is_file());
    assert!(ex.join("payload").join("plug.dll").is_file());
    // Fresh prefix: extract + `mods add` on the bundled recipe.
    let fresh = temp_config();
    let back = add_mod(&fresh, &ex.join("export-tar.toml"), &fresh).unwrap();
    assert_eq!(back.id, "export-tar");
    match &back.source {
        SourceRef::Local { path } => assert!(Path::new(path).is_dir(), "{path}"),
        other => panic!("{other:?}"),
    }
    assert!(
        existing_keep(&back).contains(&"plug.dll".to_string()),
        "{:?}",
        back.payload
    );
}
