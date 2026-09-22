use super::testing::*;
use super::*;
use std::fs;

fn r38_pkg(name: &str, url: &str, url32: Option<&str>) -> crate::download::ReshadePackage {
    crate::download::ReshadePackage {
        kind: crate::download::ReshadePackageKind::Effect,
        name: name.into(),
        description: "d".into(),
        url: Some(url.into()),
        url32: url32.map(str::to_string),
        repository_url: None,
        shader_dir: None,
        texture_dir: None,
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    }
}

fn mint_for(
    cfg: &std::path::Path,
    data: &std::path::Path,
    pkg: &crate::download::ReshadePackage,
    arch: &str,
    appid: Option<u32>,
    title: &str,
) -> crate::Result<Mod> {
    let target = MintTarget {
        appid,
        title: title.into(),
    };
    let spec = RecipeSpec::reshade_package_for_game(pkg, arch, &target)?;
    mint_recipe(cfg, data, spec)
}

fn manifest_path_for(data: &std::path::Path, game: &str, instance: &str) -> std::path::PathBuf {
    let id = crate::game::GameId::parse(game).unwrap();
    crate::game::game_dir(data, &id)
        .join("manifests")
        .join(format!("{instance}.toml"))
}

fn write_manifest_for(data: &std::path::Path, game: &str, instance: &str) {
    let path = manifest_path_for(data, game, instance);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!(
            "game = \"{game}\"\ninstance = \"{instance}\"\ntype = \"effect\"\nadapter = \"preload\"\nenabled = true\n"
        ),
    )
    .unwrap();
}

/// R38: `B-x32` / `B-x64` with the suffix before truncation — the tag is
/// never cut off, both variants fit 32 columns and stay distinct.
#[test]
fn arch_identity_suffix_before_truncation() {
    assert_eq!(arch_slug("x", "32").unwrap(), "x-x32");
    assert_eq!(arch_slug("x", "64").unwrap(), "x-x64");
    assert_eq!(arch_slug("x", "").unwrap(), "x-x64");
    let base = "a".repeat(32);
    let q = arch_slug(&base, "64").unwrap();
    assert!(q.len() <= 32, "{q}");
    assert!(q.ends_with("-x64"), "{q}");
    assert!(arch_slug("", "64").is_err());
    // Mint level: one long package name, both archs, distinct valid ids.
    let data = temp_config();
    let cfg = temp_config();
    let pkg = r38_pkg(
        "Luma Burnout Paradise Remastered Test Pack",
        "https://example.com/p64.zip",
        Some("https://example.com/p32.zip"),
    );
    let m64 = mint_for(&cfg, &data, &pkg, "64", Some(1), "G1").unwrap();
    let m32 = mint_for(&cfg, &data, &pkg, "32", Some(2), "G2").unwrap();
    assert_ne!(m64.id, m32.id);
    for m in [&m64, &m32] {
        assert!(m.id.len() <= 32, "{}", m.id);
        assert!(valid_id(&m.id), "{}", m.id);
    }
    assert!(m64.id.ends_with("-x64"), "{}", m64.id);
    assert!(m32.id.ends_with("-x32"), "{}", m32.id);
}

/// R38: migration renames the recipe file, payload dir, `mods.toml` bit,
/// every manifest, and every `requires` edge; reloads stay consistent.
#[test]
fn migrate_legacy_renames_everything() {
    let data = temp_config();
    let cfg = temp_config();
    let pkg = r38_pkg(
        "X",
        "https://example.com/x64.zip",
        Some("https://example.com/x32.zip"),
    );
    let user = user_mods_dir(&data);
    fs::create_dir_all(&user).unwrap();
    // Legacy un-suffixed recipe as the old 64-bit mint wrote it.
    fs::write(
        user.join("x.toml"),
        "id = \"x\"\ntype = \"effect\"\nlabel = \"X\"\nshader_dir = \"X\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/x64.zip\"\n",
    )
    .unwrap();
    fs::create_dir_all(user.join("x")).unwrap();
    fs::write(user.join("x/payload.bin"), b"bytes").unwrap();
    // A dependent, a disabled bit, and an installed manifest on the old id.
    fs::write(user.join("dep.toml"), ureq("dep", "requires = [\"x\"]\n")).unwrap();
    disable_mod(&cfg, "x", &data).unwrap();
    let game = "manual:standalone:g";
    write_manifest_for(&data, game, "x");

    let got = migrate_reshade_legacy(&cfg, &data, &pkg).unwrap();
    assert_eq!(got.as_deref(), Some("x-x64"));

    assert!(!user.join("x.toml").exists());
    let text = fs::read_to_string(user.join("x-x64.toml")).unwrap();
    let back = parse_recipe(&text, false).unwrap();
    assert_eq!(back.id, "x-x64");
    assert_eq!(back.shader_dir.as_deref(), Some("X"));
    assert!(matches!(
        back.source,
        SourceRef::ManualUrl { ref url } if url == "https://example.com/x64.zip"
    ));
    assert!(!user.join("x").exists());
    assert_eq!(fs::read(user.join("x-x64/payload.bin")).unwrap(), b"bytes");
    // Reload round trip: the catalog lists the new id, still disabled.
    let list = list_mods(&cfg, &data).unwrap();
    assert!(list.mods.iter().any(|m| m.id == "x-x64" && !m.enabled));
    assert!(!list.mods.iter().any(|m| m.id == "x"));
    assert!(list.problems.is_empty(), "{:?}", list.problems);
    let mods_toml = fs::read_to_string(cfg.join("mods.toml")).unwrap();
    assert!(mods_toml.contains("x-x64"), "{mods_toml}");
    assert!(!mods_toml.contains("\"x\""), "{mods_toml}");
    // Manifest + requires edges followed the id.
    assert!(!manifest_path_for(&data, game, "x").exists());
    let m = crate::download::read_manifest(&data, game, "x-x64")
        .unwrap()
        .unwrap();
    assert_eq!(m.instance, "x-x64");
    let got = crate::download::game_manifests(&data, game).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].instance, "x-x64");
    let dep = parse_recipe(&fs::read_to_string(user.join("dep.toml")).unwrap(), false).unwrap();
    assert_eq!(&dep.requires[..], ["x-x64"]);
}

/// R38: a taken `B-x64` refuses with every file unchanged.
#[test]
fn migrate_conflict_changes_nothing() {
    let data = temp_config();
    let cfg = temp_config();
    let pkg = r38_pkg(
        "X",
        "https://example.com/x64.zip",
        Some("https://example.com/x32.zip"),
    );
    // The 64-bit variant already exists as a qualified mint.
    mint_for(&cfg, &data, &pkg, "64", Some(7), "G").unwrap();
    let user = user_mods_dir(&data);
    let legacy = "id = \"x\"\ntype = \"effect\"\nlabel = \"X\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/x64.zip\"\n";
    fs::write(user.join("x.toml"), legacy).unwrap();
    let game = "manual:standalone:g";
    write_manifest_for(&data, game, "x");
    let manifest_before = fs::read_to_string(manifest_path_for(&data, game, "x")).unwrap();
    disable_mod(&cfg, "x", &data).unwrap();
    let mods_before = fs::read_to_string(cfg.join("mods.toml")).unwrap();

    let err = migrate_reshade_legacy(&cfg, &data, &pkg).unwrap_err();
    assert!(err.to_string().contains("x-x64"), "{err}");

    assert_eq!(fs::read_to_string(user.join("x.toml")).unwrap(), legacy);
    assert_eq!(
        fs::read_to_string(manifest_path_for(&data, game, "x")).unwrap(),
        manifest_before
    );
    assert_eq!(
        fs::read_to_string(cfg.join("mods.toml")).unwrap(),
        mods_before
    );
    assert!(!user.join("x-x64").exists() || user.join("x-x64.toml").exists());
    let list = list_mods(&cfg, &data).unwrap();
    assert!(list.mods.iter().any(|m| m.id == "x"));
    assert!(list.mods.iter().any(|m| m.id == "x-x64"));
}

/// R38: no legacy recipe (or a same-slug foreign source) → `None`, and a
/// same-slug hand-written mod is never touched.
#[test]
fn migrate_no_legacy_returns_none() {
    let data = temp_config();
    let cfg = temp_config();
    let pkg = r38_pkg(
        "X",
        "https://example.com/x64.zip",
        Some("https://example.com/x32.zip"),
    );
    assert_eq!(migrate_reshade_legacy(&cfg, &data, &pkg).unwrap(), None);
    // Same slug, another source: not legacy, left alone.
    let user = user_mods_dir(&data);
    fs::create_dir_all(&user).unwrap();
    let foreign = "id = \"x\"\ntype = \"effect\"\nlabel = \"X\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/other.zip\"\n";
    fs::write(user.join("x.toml"), foreign).unwrap();
    assert_eq!(migrate_reshade_legacy(&cfg, &data, &pkg).unwrap(), None);
    assert_eq!(fs::read_to_string(user.join("x.toml")).unwrap(), foreign);
}

/// R38: legacy Github sources migrate on owner/repo + asset match; a
/// same-slug recipe from another asset is left alone.
#[test]
fn migrate_legacy_github_source_rules() {
    let data = temp_config();
    let cfg = temp_config();
    let mut pkg = r38_pkg(
        "Toggler",
        "https://github.com/o/r/releases/download/t/a-x64.zip",
        Some("https://github.com/o/r/releases/download/t/a-x32.zip"),
    );
    pkg.repository_url = Some("https://github.com/o/r".into());
    let user = user_mods_dir(&data);
    fs::create_dir_all(&user).unwrap();
    let recipe = |asset: &str| {
        format!(
            "id = \"toggler\"\ntype = \"reshade_addon\"\nlabel = \"Toggler\"\n[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"{asset}\"\n"
        )
    };
    fs::write(user.join("toggler.toml"), recipe("other.zip")).unwrap();
    assert_eq!(migrate_reshade_legacy(&cfg, &data, &pkg).unwrap(), None);
    assert!(user.join("toggler.toml").exists());
    fs::write(user.join("toggler.toml"), recipe("a-x64.zip")).unwrap();
    let got = migrate_reshade_legacy(&cfg, &data, &pkg).unwrap();
    assert_eq!(got.as_deref(), Some("toggler-x64"));
}

/// R38: both variants list, install (manifests), enable, and select for
/// their own games; neither leaks into the other's game.
#[test]
fn coexistence_installed_enabled_selected() {
    let data = temp_config();
    let cfg = temp_config();
    let pkg = r38_pkg(
        "X",
        "https://example.com/x64.zip",
        Some("https://example.com/x32.zip"),
    );
    mint_for(&cfg, &data, &pkg, "64", Some(10), "Game 64").unwrap();
    mint_for(&cfg, &data, &pkg, "32", Some(11), "Game 32").unwrap();
    let g64 = "manual:standalone:g64";
    let g32 = "manual:standalone:g32";
    write_manifest_for(&data, g64, "x-x64");
    write_manifest_for(&data, g32, "x-x32");
    let got64 = crate::download::game_manifests(&data, g64).unwrap();
    assert_eq!(got64.len(), 1);
    assert_eq!(got64[0].instance, "x-x64");
    let got32 = crate::download::game_manifests(&data, g32).unwrap();
    assert_eq!(got32.len(), 1);
    assert_eq!(got32[0].instance, "x-x32");
    // Per-game selection follows the binding.
    let for64 = mods_for_game(&cfg, "Game 64", Some(10), &data).unwrap();
    assert!(for64.mods.iter().any(|m| m.id == "x-x64"));
    assert!(!for64.mods.iter().any(|m| m.id == "x-x32"));
    let for32 = mods_for_game(&cfg, "Game 32", Some(11), &data).unwrap();
    assert!(for32.mods.iter().any(|m| m.id == "x-x32"));
    assert!(!for32.mods.iter().any(|m| m.id == "x-x64"));
    // Enable/disable is per variant.
    set_mod_offered(&cfg, "x-x32", false, &data).unwrap();
    let list = list_mods(&cfg, &data).unwrap();
    assert!(list.mods.iter().any(|m| m.id == "x-x32" && !m.enabled));
    assert!(list.mods.iter().any(|m| m.id == "x-x64" && m.enabled));
    set_mod_offered(&cfg, "x-x32", true, &data).unwrap();
}
