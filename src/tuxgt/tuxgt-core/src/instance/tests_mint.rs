use super::testing::*;
use super::*;
use std::fs;

fn mint_family(
    cfg: &std::path::Path,
    data: &std::path::Path,
    tpl: &ModTemplate,
    fam: &TemplateFamily,
    asset: &str,
    title: &str,
    label: &str,
    appid: Option<u32>,
) -> crate::Result<Mod> {
    let spec = RecipeSpec::family(tpl, fam, asset, title, label, appid)?;
    mint_recipe(cfg, data, spec)
}

fn mint_pkg(
    cfg: &std::path::Path,
    data: &std::path::Path,
    pkg: &crate::download::ReshadePackage,
) -> crate::Result<Mod> {
    let spec = RecipeSpec::reshade_package(pkg)?;
    mint_recipe(cfg, data, spec)
}

#[test]
fn family_mint_roundtrip_and_refusals() {
    let data = temp_config();
    let cfg = temp_config();
    let tpl = family_tpl();
    let fam = tpl.family.as_ref().unwrap();
    // AppID given: appids set, games empty; the written recipe lists
    // through list_mods (round-trip).
    let m = mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "renodx-tst-doom.addon64",
        "DOOM Eternal",
        "Family X: DOOM Eternal",
        Some(1245620),
    )
    .unwrap();
    assert_eq!(m.id, "renodx-tst-doom");
    assert_eq!(&m.appids[..], [1245620]);
    assert!(m.games.is_empty());
    match &m.source {
        SourceRef::Github {
            asset_glob,
            tag,
            prerelease,
            ..
        } => {
            assert_eq!(asset_glob, "renodx-tst-doom.addon64");
            assert_eq!(tag.as_deref(), None);
            assert!(prerelease);
        }
        other => panic!("{other:?}"),
    }
    // Non-empty family drop becomes a payload rule.
    assert_eq!(m.payload.len(), 1);
    assert_eq!(&m.payload[0].drop[..], ["dxgi.dll"]);
    let list = list_mods(&cfg, &data).unwrap();
    assert!(list.mods.iter().any(|i| i.id == "renodx-tst-doom"));
    // Second mint of the same asset: id collision refused.
    assert!(mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "renodx-tst-doom.addon64",
        "DOOM Eternal",
        "Family X: DOOM Eternal",
        Some(1245620),
    )
    .is_err());
    // Official id shadowing is refused (stem parses to `reshade`).
    assert!(mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "reshade.addon64",
        "ReShade",
        "Family X: ReShade",
        None,
    )
    .is_err());
    // No AppID: games glob from the title, no appids; zero appid refused.
    let g = mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "renodx-fx.addon64",
        "FX",
        "Family X: FX",
        None,
    )
    .unwrap();
    assert_eq!(&g.games[..], ["*FX*"]);
    assert!(g.appids.is_empty());
    assert!(mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "renodx-zz.addon64",
        "ZZ",
        "Family X: ZZ",
        Some(0),
    )
    .is_err());
    // Dots sanitize away (extras sanitizer), so a dotted stem now mints.
    let dotted = mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "renodx-1.2.addon64",
        "X",
        "Family X: X",
        None,
    )
    .unwrap();
    assert_eq!(dotted.id, "renodx-12");
    // A slug that cannot parse is an error, never a silent rename.
    assert!(mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "1-2.addon64",
        "X",
        "Family X: X",
        None,
    )
    .is_err());
}

#[test]
fn family_slug_variant_truncation_and_collision() {
    // Over-long stems keep trailing test/dev/x32 tokens.
    assert_eq!(
        package_slug("Luma-Blue_Reflection_Second_Light-Test").unwrap(),
        "luma-blue-reflection-second-test"
    );
    assert_eq!(
        package_slug("Luma-Burnout_Paradise_Remastered-Test-x32").unwrap(),
        "luma-burnout-paradise-r-test-x32"
    );
    assert_eq!(
        package_slug("Luma-Burnout_Paradise_Remastered-x32").unwrap(),
        "luma-burnout-paradise-remast-x32"
    );
    // Base and Test variants no longer collide.
    assert_ne!(
        package_slug("Luma-Middle-earth_Shadow_of_War-Test").unwrap(),
        package_slug("Luma-Middle-earth_Shadow_of_War").unwrap()
    );
    // Distinct assets colliding on a listed id get -2, same asset refused.
    let data = temp_config();
    let cfg = temp_config();
    let tpl = family_tpl();
    let fam = tpl.family.as_ref().unwrap();
    let first = mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "x-ab.addon64",
        "AB",
        "Family X: AB",
        None,
    )
    .unwrap();
    assert_eq!(first.id, "x-ab");
    assert!(mint_family(
        &cfg,
        &data,
        &tpl,
        fam,
        "x-ab.addon64",
        "AB",
        "Family X: AB",
        None
    )
    .is_err());
}

fn extras_pkg(name: &str, url: &str) -> crate::download::ReshadePackage {
    crate::download::ReshadePackage {
        kind: crate::download::ReshadePackageKind::Effect,
        name: name.into(),
        description: "d".into(),
        url: Some(url.into()),
        url32: None,
        repository_url: Some("https://github.com/FransBouma/OtisFX".into()),
        shader_dir: Some("OtisFX".into()),
        texture_dir: Some("OtisFX".into()),
        deny_files: vec!["Template.fx".into()].into_boxed_slice(),
        effect_files: vec!["B.fx".into(), "A.fx".into(), "Template.fx".into()].into_boxed_slice(),
        in_catalog: false,
    }
}

#[test]
fn reshade_package_mint_roundtrip_and_refusals() {
    let data = temp_config();
    let cfg = temp_config();
    let pkg = extras_pkg(
        "OtisFX by Otis_Inf",
        "https://github.com/FransBouma/OtisFX/archive/master.zip",
    );
    let m = mint_pkg(&cfg, &data, &pkg).unwrap();
    assert_eq!(m.id, "otisfx-by-otis-inf");
    assert_eq!(m.mod_type, "effect");
    assert_eq!(m.label, "OtisFX by Otis_Inf");
    assert!(m.games.is_empty());
    assert!(m.appids.is_empty());
    assert_eq!(m.shader_dir.as_deref(), Some("OtisFX"));
    assert_eq!(m.texture_dir.as_deref(), Some("OtisFX"));
    assert_eq!(
        &m.payload[0].drop[..],
        ["Template.fx".to_string(), "*/Template.fx".to_string()]
    );
    match &m.source {
        SourceRef::ManualUrl { url } => {
            assert!(url.ends_with("/archive/master.zip"), "{url}");
        }
        other => panic!("{other:?}"),
    }
    let text = fs::read_to_string(user_mods_dir(&data).join("otisfx-by-otis-inf.toml")).unwrap();
    let back = parse_recipe(&text, false).unwrap();
    assert_eq!(back.id, m.id);
    assert_eq!(back.shader_dir, m.shader_dir);
    assert_eq!(&back.effect_files[..], &m.effect_files[..]);
    // Recipe `EffectFiles` minus the recipe's own drops, sorted.
    assert_eq!(
        crate::mods::effect_names_for(&back),
        vec!["A.fx".to_string(), "B.fx".to_string()]
    );
    assert!(mint_pkg(&cfg, &data, &pkg).is_err());
    let no_url = crate::download::ReshadePackage {
        url: None,
        ..pkg.clone()
    };
    assert!(mint_pkg(&cfg, &data, &no_url).is_err());
    let gh = extras_pkg(
            "Swap chain override by crosire",
            "https://github.com/crosire/reshade-docs/releases/latest/download/swapchain_override.addon64",
        );
    let mut gh = gh;
    gh.kind = crate::download::ReshadePackageKind::Addon;
    gh.shader_dir = None;
    gh.texture_dir = None;
    gh.deny_files = Box::default();
    gh.repository_url = Some("https://github.com/crosire/reshade".into());
    let a = mint_pkg(&cfg, &data, &gh).unwrap();
    assert_eq!(a.mod_type, "reshade_addon");
    assert!(a.shader_dir.is_none());
    // Addon mints carry no EffectFiles list.
    assert!(a.effect_files.is_empty());
    match a.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            assert_eq!(owner, "crosire");
            assert_eq!(repo, "reshade-docs");
            assert_eq!(asset_glob, "swapchain_override.addon64");
            assert_eq!(tag, None);
            assert!(!prerelease);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn shader_dir_recipe_keys() {
    let ok = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\nshader_dir = \"OtisFX\"\ntexture_dir = \"OtisFX\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    let m = parse_recipe(ok, false).unwrap();
    assert_eq!(m.shader_dir.as_deref(), Some("OtisFX"));
    assert_eq!(m.texture_dir.as_deref(), Some("OtisFX"));
    let empty = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\nshader_dir = \"\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert!(parse_recipe(empty, false).is_err());
    let slash = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\nshader_dir = \"a/b\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert!(parse_recipe(slash, false).is_err());
    let addon = "id = \"x\"\ntype = \"reshade_addon\"\nlabel = \"x\"\nshader_dir = \"OtisFX\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert!(parse_recipe(addon, false).is_err());
    let listed = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\neffect_files = [\"A.fx\", \"B.fx\"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    let m = parse_recipe(listed, false).unwrap();
    assert_eq!(&m.effect_files[..], ["A.fx", "B.fx"]);
    for bad in ["\"a/b.fx\"", "\"\"", "\"a\\\\b.fx\""] {
        let text = format!(
                "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\neffect_files = [{bad}]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n"
            );
        assert!(parse_recipe(&text, false).is_err(), "{bad}");
    }
}

#[test]
fn local_and_manual_url_parse() {
    let local = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert!(matches!(local.source, SourceRef::Local { .. }));
    let url = parse_recipe(
            "id = \"y\"\ntype = \"custom\"\nlabel = \"y\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/x.zip\"\n",
            false,
        )
        .unwrap();
    assert!(matches!(url.source, SourceRef::ManualUrl { .. }));
    assert_eq!(resolve_source(&url).type_str(), "manual_url");
}

#[test]
fn rejects_bad_recipes() {
    let cases: Vec<String> = vec![
            sample_recipe("bad id!", "\"preload\""),
            "id = \"x\"\ntype = \"injector\"\nlabel = \"x\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n".into(),
            sample_recipe("x", "\"preload\", \"warp\""),
            sample_recipe("x", ""),
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\nextra = 1\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n".into(),
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"nexus\"\n".into(),
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"github\"\nowner = \"a\"\n".into(),
        ];
    for text in &cases {
        assert!(parse_recipe(text, false).is_err(), "{text}");
    }
}

#[test]
fn fork_never_gets_proton_env() {
    assert!(parse_recipe(&sample_recipe("fork", "\"preload\", \"proton_env\""), false).is_err());
    assert!(parse_recipe(&sample_recipe("fork", "\"preload\""), false).is_ok());
}
