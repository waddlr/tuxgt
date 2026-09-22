use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::{Error, FileManifest};
use std::fs;

#[test]
fn preview_payload_files_applies_drops() {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let data = std::env::temp_dir().join(format!("tuxgt-preview-{}-{ts}", std::process::id()));
    let cfg = data.join("cfg");
    let _ = std::fs::remove_dir_all(&data);
    crate::instance::seed_official_share_from_repo(&data);
    std::fs::create_dir_all(&cfg).unwrap();
    let src = data.join("src");
    std::fs::create_dir_all(src.join("reshade-shaders").join("Shaders")).unwrap();
    std::fs::write(
        src.join("reshade-shaders").join("Shaders").join("a.fx"),
        b"a",
    )
    .unwrap();
    std::fs::write(
        src.join("reshade-shaders").join("Shaders").join("skip.fx"),
        b"s",
    )
    .unwrap();
    // Repo junk in the tree: the list and gate asserts below prove both exclude it.
    std::fs::write(src.join("reshade-shaders").join("README.md"), b"r").unwrap();
    std::fs::create_dir_all(src.join("reshade-shaders").join(".github")).unwrap();
    std::fs::write(
        src.join("reshade-shaders").join(".github").join("logo.png"),
        b"l",
    )
    .unwrap();
    crate::add_mod_from(
        &cfg,
        "effect",
        "prev-pack",
        &src,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let (names, total) = preview_payload_files(&cfg, &data, "prev-pack").unwrap();
    assert_eq!(
        names,
        vec!["Shaders/a.fx".to_string(), "Shaders/skip.fx".to_string()]
    );
    assert_eq!(total, 2);
    let pack = find_mod(&cfg, &data, "prev-pack").unwrap();
    assert!(payload_has_files(&data, &pack));
    // A listed Mod with no payload gates the Files preview off.
    let reshade = find_mod(&cfg, &data, "reshade").unwrap();
    assert!(!payload_has_files(&data, &reshade));
    // Persist a drop rule and confirm the preview excludes it.
    let toml_path = data.join("mods").join("user").join("prev-pack.toml");
    let mut text = std::fs::read_to_string(&toml_path).unwrap();
    text.push_str("\n[[payload]]\ndrop = [\"*/skip.fx\"]\n");
    std::fs::write(&toml_path, text).unwrap();
    let (names, total) = preview_payload_files(&cfg, &data, "prev-pack").unwrap();
    assert_eq!(names, vec!["Shaders/a.fx".to_string()]);
    assert_eq!(total, 1);
    // One of two files dropped: the gate stays open.
    let pack = find_mod(&cfg, &data, "prev-pack").unwrap();
    assert!(payload_has_files(&data, &pack));
    // Every payload file dropped: the gate closes with the list.
    let mut text = std::fs::read_to_string(&toml_path).unwrap();
    text.push_str("\n[[payload]]\ndrop = [\"*/a.fx\"]\n");
    std::fs::write(&toml_path, text).unwrap();
    let pack = find_mod(&cfg, &data, "prev-pack").unwrap();
    assert!(!payload_has_files(&data, &pack));
    // Unknown id errors; missing payload dir is empty, never an error.
    assert!(preview_payload_files(&cfg, &data, "ghost").is_err());
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn official_optiscaler_payload_drops_windows_helpers() {
    let pfx = std::env::temp_dir().join(format!(
        "tuxgt-e88-payload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&pfx);
    crate::instance::seed_official_share_from_repo(&pfx);
    let inst = find_mod(&pfx, &pfx, "optiscaler").unwrap();
    let dests = [
        "OptiScaler.dll",
        "OptiScaler.ini",
        "setup_windows.bat",
        "setup_linux.sh",
        "!! README_EXTRACT ALL FILES TO GAME FOLDER !!.txt",
        "Licenses/FidelityFX_v1_LICENSE.md",
        "Licenses/DirectX_LICENSE.txt",
        "fakenvapi.ini",
    ];
    assert_eq!(
        filter_payload(&inst.payload, &dests, "64", "dx12"),
        vec![true, true, false, true, false, false, true, true]
    );
}

#[test]
fn luma_payload_drops_only_bundled_reshade() {
    let pfx = std::env::temp_dir().join(format!(
        "tuxgt-e88-payload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&pfx);
    crate::instance::seed_official_share_from_repo(&pfx);
    // Fixture recipe, inlined: mods-contrib is not published, so the
    // payload behavior test cannot copy it from the repo tree.
    std::fs::write(
            crate::instance::official_mods_dir(&pfx).join("luma-crimson-desert.toml"),
            "id = \"luma-crimson-desert\"\ntype = \"reshade_addon\"\nlabel = \"Luma: Crimson Desert\"\ngames = [\"*Crimson Desert*\"]\n[[payload]]\ndrop = [\"dxgi.dll\"]\n[source]\ntype = \"github\"\nowner = \"Filoppi\"\nrepo = \"Luma-Framework\"\nasset_glob = \"Luma-Crimson-Desert.zip\"\n",
        )
        .unwrap();
    let inst = find_mod(&pfx, &pfx, "luma-crimson-desert").unwrap();
    let dests = [
        "dxgi.dll",
        "nvngx_dlss.dll",
        "Luma/d3dcompiler_47.dll",
        "Luma/Global/Luma_Copy_PS.hlsl",
        "Luma/CrimsonDesert/Final_0x1F993880.ps_5_0.hlsl",
        "Luma-Crimson-Desert.addon",
    ];
    assert_eq!(
        filter_payload(&inst.payload, &dests, "", ""),
        vec![false, true, true, true, true, true]
    );
}
#[test]
fn no_rules_keeps_everything() {
    let dests = ["a.dll", "b.json"];
    assert_eq!(filter_payload(&[], &dests, "64", "dx12"), vec![true, true]);
}

#[test]
fn arch_gated_keep_selects_payload() {
    let rules = vec![
        rule(Some("64"), None, &["ReShade64.dll"], &[]),
        rule(Some("32"), None, &["ReShade32.dll"], &[]),
    ];
    let dests = ["ReShade32.dll", "ReShade64.dll", "ReShade64.json"];
    assert_eq!(
        filter_payload(&rules, &dests, "64", "dx12"),
        vec![false, true, false]
    );
    assert_eq!(
        filter_payload(&rules, &dests, "32", "dx11"),
        vec![true, false, false]
    );
}

#[test]
fn unknown_game_falls_back_to_everything() {
    let rules = vec![rule(Some("64"), None, &["ReShade64.dll"], &[])];
    let dests = ["ReShade32.dll", "ReShade64.dll"];
    assert_eq!(filter_payload(&rules, &dests, "", ""), vec![true, true]);
}

#[test]
fn drop_subtracts_from_keep_all() {
    let rules = vec![rule(None, Some("vulkan"), &[], &["*vk.dll"])];
    let dests = ["OptiScaler.dll", "amd_fidelityfx_vk.dll"];
    assert_eq!(
        filter_payload(&rules, &dests, "64", "vulkan"),
        vec![true, false]
    );
    assert_eq!(
        filter_payload(&rules, &dests, "64", "dx12"),
        vec![true, true]
    );
}
#[test]
fn junk_dests_drop_without_recipe_rules() {
    // Live AstrayFX mint carries repo junk no recipe rule names
    // (effect-junk-dests); none of it may enter the manifest.
    let dests = [
        ".github/AstrayFXLogo.png",
        ".github/FUNDING.yml",
        "_config.yml",
        "README.md",
        "readme.txt",
        "Textures/dummy",
        "Shaders/AstrayFX.fx",
        "Textures/noise.png",
        "Shaders/ReadmeHelper.fx",
    ];
    assert_eq!(
        filter_payload(&[], &dests, "64", "dx12"),
        vec![false, false, false, false, false, false, true, true, true]
    );
    // An explicit keep-all (from-package scan) still sheds junk.
    let rules = vec![rule(None, None, &["*"], &[])];
    assert_eq!(
        filter_payload(&rules, &dests, "64", "dx12"),
        vec![false, false, false, false, false, false, true, true, true]
    );
}

#[tokio::test]
async fn install_refuses_disabled_uninstall_still_works() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e46-mods-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("data");
    let cfg = dir.join("config");
    fs::create_dir_all(&cfg).unwrap();
    crate::instance::seed_official_share_from_repo(&data);
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:e46test";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("E46"),
            ..Default::default()
        },
    )
    .await;
    crate::disable_mod(&cfg, "reshade", &data).unwrap();
    let err = install_instance(
        &pool,
        &data,
        &cfg,
        gid,
        "reshade",
        &InstallOpts::default(),
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::InstanceDisabled(ref id) if id == "reshade"));

    let adir = dir.join("addonpkg");
    fs::create_dir_all(&adir).unwrap();
    fs::write(adir.join("my-addon.addon64"), b"addon").unwrap();
    crate::add_mod_from(
        &cfg,
        "reshade_addon",
        "my-addon",
        &adir,
        None,
        None,
        &data,
        Vec::new(),
        vec!["reshade".into()],
    )
    .unwrap();
    let mut wr = InstallOpts::default();
    wr.with_requires = Some("reshade".into());
    let err = install_instance(&pool, &data, &cfg, gid, "my-addon", &wr, None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InstanceDisabled(ref id) if id == "reshade"));

    crate::enable_mod(&cfg, "reshade", &data).unwrap();
    assert!(find_mod(&cfg, &data, "reshade").unwrap().enabled);

    let manifest = FileManifest {
        game: gid.into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    };
    crate::write_manifest(&data, &manifest).unwrap();
    crate::disable_mod(&cfg, "reshade", &data).unwrap();
    uninstall_instance(&pool, &data, &cfg, gid, "reshade", true)
        .await
        .unwrap();
    assert!(crate::read_manifest(&data, gid, "reshade")
        .unwrap()
        .is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn uninstall_removes_runtime_dests_not_others() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-uninst-rt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "steam::1";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("UninstRt"),
            ..Default::default()
        },
    )
    .await;
    let rt = crate::stage::runtime_dir(&data, gid);
    fs::create_dir_all(&rt).unwrap();
    fs::write(rt.join("ShaderToggler.addon64"), b"addon").unwrap();
    fs::write(rt.join("ReShade64.dll"), b"rs").unwrap();
    fs::write(rt.join("ReShade.ini"), b"gen").unwrap();
    let file = |dest: &str| crate::PlannedFile {
        source: dest.into(),
        dest: dest.into(),
        sha256: "aa".into(),
        enabled: true,
    };
    crate::write_manifest(
        &data,
        &FileManifest {
            game: gid.into(),
            instance: "my-addon".into(),
            mod_type: "reshade_addon".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![file("ShaderToggler.addon64")].into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
    crate::write_manifest(
        &data,
        &FileManifest {
            game: gid.into(),
            instance: "reshade".into(),
            mod_type: "reshade".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![file("ReShade64.dll")].into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
        },
    )
    .unwrap();
    uninstall_instance(&pool, &data, &data.join("config"), gid, "my-addon", true)
        .await
        .unwrap();
    assert!(!rt.join("ShaderToggler.addon64").exists());
    assert!(rt.join("ReShade64.dll").is_file());
    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"gen");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn payload_keeps_union_ignores_gates() {
    use crate::PayloadRule;
    let rule = |arch: Option<&str>, keep: &[&str]| PayloadRule {
        arch: arch.map(str::to_string),
        api: None,
        keep: keep.iter().map(|s| s.to_string()).collect(),
        drop: Box::default(),
    };
    // No rule declaring `keep` keeps everything (add-form scan defaults).
    assert!(payload_keeps(&[], "any.ini"));
    assert!(payload_keeps(&[rule(None, &[])], "any.ini"));
    // A gated keep still shows: the preview names a mod, not a game.
    let rules = vec![rule(Some("32"), &["x.ini"])];
    assert!(payload_keeps(&rules, "x.ini"));
    assert!(!payload_keeps(&rules, "y.ini"));
}

#[test]
fn preview_payload_files_hides_unchecked() {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let data = std::env::temp_dir().join(format!("tuxgt-keep-{ts}-{}", std::process::id()));
    let cfg = data.join("cfg");
    let _ = std::fs::remove_dir_all(&data);
    crate::instance::seed_official_share_from_repo(&data);
    std::fs::create_dir_all(&cfg).unwrap();
    let src = data.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("kept.ini"), b"k").unwrap();
    std::fs::write(src.join("dormant.ini"), b"d").unwrap();
    // Add-form uncheck: `keep = false`, dest dropped.
    let files = vec![
        crate::PackageFile {
            src: "kept.ini".into(),
            dest: "kept.ini".into(),
            keep: true,
            is_new: false,
        },
        crate::PackageFile {
            src: "dormant.ini".into(),
            dest: String::new(),
            keep: false,
            is_new: false,
        },
    ];
    crate::add_mod_from(
        &cfg,
        "effect",
        "prev-keep",
        &src,
        None,
        Some(&files),
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let (names, total) = preview_payload_files(&cfg, &data, "prev-keep").unwrap();
    assert_eq!(names, vec!["kept.ini".to_string()]);
    assert_eq!(total, 1);
    // Dormant bytes stay in the payload (add-form recheck/rescan need them).
    let dir = crate::instance::payload_dir(&data, false, None, "prev-keep");
    assert!(dir.join("dormant.ini").is_file());
    let pack = find_mod(&cfg, &data, "prev-keep").unwrap();
    assert!(payload_has_files(&data, &pack));
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn preview_payload_files_returns_full_list() {
    // The GUI truncates for display with an expander; core must not cap,
    // or expanded lists could never show the tail.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let data = std::env::temp_dir().join(format!("tuxgt-cap-{ts}-{}", std::process::id()));
    let cfg = data.join("cfg");
    let _ = std::fs::remove_dir_all(&data);
    crate::instance::seed_official_share_from_repo(&data);
    std::fs::create_dir_all(&cfg).unwrap();
    let src = data.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for i in 0..35 {
        std::fs::write(src.join(format!("f{i:02}.ini")), b"x").unwrap();
    }
    crate::add_mod_from(
        &cfg,
        "effect",
        "prev-cap",
        &src,
        None,
        None,
        &data,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let (names, total) = preview_payload_files(&cfg, &data, "prev-cap").unwrap();
    assert_eq!(names.len(), 35);
    assert_eq!(total, 35);
    let _ = std::fs::remove_dir_all(&data);
}
