use super::testing::*;
use super::*;
use std::fs;
use std::sync::Mutex as StdMutex;

#[test]
fn fetch_progress_percent_needs_a_total() {
    assert_eq!(
        FetchProgress {
            bytes: 5,
            total: None
        }
        .percent(),
        None
    );
    assert_eq!(
        FetchProgress {
            bytes: 5,
            total: Some(0)
        }
        .percent(),
        None
    );
    assert_eq!(
        FetchProgress {
            bytes: 5,
            total: Some(10)
        }
        .percent(),
        Some(50.0)
    );
    // A body longer than the advertised length cannot paint past 100%.
    assert_eq!(
        FetchProgress {
            bytes: 20,
            total: Some(10)
        }
        .percent(),
        Some(100.0)
    );
}

#[tokio::test]
async fn fetch_url_reports_progress_then_cache_hit_reports_nothing() {
    let body: &'static [u8] = b"0123456789abcdef";
    let port = serve_http(body, body.len(), body.len(), 1);
    let dir = std::env::temp_dir().join(format!("tuxgt-e102-prog-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let url = format!("http://127.0.0.1:{port}/optiscaler.7z");
    let seen = StdMutex::new(Vec::new());
    let sink = |p: FetchProgress| seen.lock().unwrap().push(p);
    let asset = fetch_url(&dir, &url, None, Some("optiscaler"), Some(&sink), false)
        .await
        .unwrap();
    assert_eq!(asset.bytes, body.len() as u64);
    let seen = seen.into_inner().unwrap();
    let first = seen.first().expect("a report before the first chunk");
    assert_eq!(first.total, Some(body.len() as u64));
    // Monotonic, ending on the whole file at 100%.
    assert!(seen.windows(2).all(|w| w[1].bytes >= w[0].bytes));
    let last = seen.last().unwrap();
    assert_eq!(last.bytes, body.len() as u64);
    assert_eq!(last.percent(), Some(100.0));
    // Cache hit: no transfer, so nothing to report — a caller's bar stays
    // indeterminate instead of parking at 0%.
    let quiet = StdMutex::new(Vec::new());
    let quiet_sink = |p: FetchProgress| quiet.lock().unwrap().push(p);
    fetch_url(
        &dir,
        &url,
        None,
        Some("optiscaler"),
        Some(&quiet_sink),
        false,
    )
    .await
    .unwrap();
    assert!(quiet.into_inner().unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn parse_reshade_ini_skips_comments_and_picks_url64() {
    let effects = "\
[00]\n\
PackageName=Standard effects\n\
PackageDescription=utils\n\
InstallPath=.\\reshade-shaders\\Shaders\n\
TextureInstallPath=.\\reshade-shaders\\Textures\n\
DownloadUrl=https://github.com/crosire/reshade-shaders/archive/slim.zip\n\
RepositoryUrl=https://github.com/crosire/reshade-shaders/tree/slim\n\
\n\
[03]\n\
PackageName=OtisFX by Otis_Inf\n\
PackageDescription=photo\n\
InstallPath=.\\reshade-shaders\\Shaders\\OtisFX\n\
TextureInstallPath=.\\reshade-shaders\\Textures\\OtisFX\n\
DownloadUrl=https://github.com/FransBouma/OtisFX/archive/master.zip\n\
RepositoryUrl=https://github.com/FransBouma/OtisFX\n\
DenyEffectFiles=Template.fx\n\
\n\
[09]\n\
PackageName=qUINT by Marty McFly\n\
PackageDescription=qUINT\n\
InstallPath=.\\reshade-shaders\\Shaders\\qUINT\n\
TextureInstallPath=.\\reshade-shaders\\Textures\n\
DownloadUrl=https://github.com/martymcmodding/qUINT/archive/master.zip\n\
RepositoryUrl=https://github.com/martymcmodding/qUINT\n\
";
    let addons = "\
# [00]\n\
# PackageName=Framerate Limiter by crosire\n\
# DownloadUrl64=https://github.com/crosire/reshade-docs/releases/latest/download/framerate_limit.addon64\n\
\n\
[00]\n\
PackageName=Swap chain override by crosire\n\
PackageDescription=windowed\n\
DownloadUrl32=https://github.com/crosire/reshade-docs/releases/latest/download/swapchain_override.addon32\n\
DownloadUrl64=https://github.com/crosire/reshade-docs/releases/latest/download/swapchain_override.addon64\n\
RepositoryUrl=https://github.com/crosire/reshade/tree/main/examples/16-swapchain_override\n\
\n\
[09]\n\
PackageName=Example toggler by someone\n\
PackageDescription=toggle\n\
DownloadUrl64=https://github.com/example/ExampleAddon/releases/download/1.2.2/ExampleAddon_v122.zip\n\
RepositoryUrl=https://github.com/example/ExampleAddon\n\
\n\
[14]\n\
PackageName=PyHook by dwojtasik\n\
PackageDescription=python\n\
RepositoryUrl=https://github.com/dwojtasik/PyHook\n\
";
    let pkgs = parse_reshade_packages(effects, addons);
    assert_eq!(
        pkgs.len(),
        6,
        "{:?}",
        pkgs.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    assert!(pkgs
        .iter()
        .all(|p| p.name != "Framerate Limiter by crosire"));
    let std = pkgs.iter().find(|p| p.name == "Standard effects").unwrap();
    assert_eq!(std.kind, ReshadePackageKind::Effect);
    assert_eq!(std.shader_dir, None);
    assert_eq!(std.texture_dir, None);
    assert_eq!(
        std.url.as_deref(),
        Some("https://github.com/crosire/reshade-shaders/archive/slim.zip")
    );
    let otis = pkgs.iter().find(|p| p.name.starts_with("OtisFX")).unwrap();
    assert_eq!(otis.shader_dir.as_deref(), Some("OtisFX"));
    assert_eq!(otis.texture_dir.as_deref(), Some("OtisFX"));
    assert_eq!(&otis.deny_files[..], ["Template.fx"]);
    let quint = pkgs.iter().find(|p| p.name.starts_with("qUINT")).unwrap();
    assert_eq!(quint.shader_dir.as_deref(), Some("qUINT"));
    assert_eq!(quint.texture_dir, None);
    let swap = pkgs
        .iter()
        .find(|p| p.name.starts_with("Swap chain"))
        .unwrap();
    assert_eq!(swap.kind, ReshadePackageKind::Addon);
    assert!(swap
        .url
        .as_deref()
        .unwrap()
        .ends_with("swapchain_override.addon64"));
    assert!(!swap.url.as_deref().unwrap().ends_with(".addon32"));
    let py = pkgs.iter().find(|p| p.name.starts_with("PyHook")).unwrap();
    assert!(py.url.is_none());
    assert!(!py.mintable_for_arch("64"));
    assert!(!py.mintable_for_arch("32"));
    assert_eq!(
            parse_github_release_url(
                "https://github.com/crosire/reshade-docs/releases/latest/download/swapchain_override.addon64"
            ),
            Some((
                "crosire".into(),
                "reshade-docs".into(),
                None,
                "swapchain_override.addon64".into()
            ))
        );
    assert_eq!(
        parse_github_release_url(
            "https://github.com/example/ExampleAddon/releases/download/1.2.2/ExampleAddon_v122.zip"
        ),
        Some((
            "example".into(),
            "ExampleAddon".into(),
            Some("1.2.2".into()),
            "ExampleAddon_v122.zip".into()
        ))
    );
    assert!(parse_github_release_url(
        "https://github.com/example/ExampleShaders/archive/master.zip"
    )
    .is_none());
}

#[test]
fn parse_reshade_effect_files_and_preview() {
    let effects = "[00]\nPackageName=X\nEffectFiles=A.fx,B.fx,Template.fx\nDenyEffectFiles=Template.fx\nDownloadUrl=https://example.com/x.zip\n";
    let pkgs = parse_reshade_packages(effects, "");
    assert_eq!(pkgs.len(), 1);
    assert_eq!(&pkgs[0].effect_files[..], ["A.fx", "B.fx", "Template.fx"]);
    let (names, total) = preview_effect_files(&pkgs[0]);
    assert_eq!(
        (names, total),
        (vec!["A.fx".to_string(), "B.fx".to_string()], 2)
    );
}

#[test]
fn cap_names_cases() {
    assert_eq!(cap_names(Vec::new(), 30), (Vec::<String>::new(), 0));
    let exact: Vec<String> = (0..30).map(|i| format!("f{i}")).collect();
    assert_eq!(cap_names(exact.clone(), 30), (exact, 30));
    let over: Vec<String> = (0..35).map(|i| format!("f{i}")).collect();
    let (names, total) = cap_names(over, 30);
    assert_eq!(total, 35);
    assert_eq!(names.len(), 30);
}

#[test]
fn reshade_package_already_in_catalog() {
    let fx = ReshadePackage {
        kind: ReshadePackageKind::Effect,
        name: "Example shaders by example".into(),
        description: String::new(),
        url: Some("https://github.com/example/ExampleShaders/archive/master.zip".into()),
        url32: None,
        repository_url: Some("https://github.com/example/ExampleShaders".into()),
        shader_dir: Some("ExampleShaders".into()),
        texture_dir: Some("ExampleShaders".into()),
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    };
    let listed = [crate::instance::Mod {
        id: "my-effect".into(),
        mod_type: "effect".into(),
        label: "Example shaders".into(),
        description: String::new(),
        source: crate::instance::SourceRef::Github {
            owner: "example".into(),
            repo: "ExampleShaders".into(),
            asset_glob: "Shaders.zip".into(),
            tag: None,
            prerelease: false,
        },
        plans_allowed: Box::default(),
        sha256: None,
        payload: Box::default(),
        games: Box::default(),
        appids: Box::default(),
        requires: Box::default(),
        dests: Default::default(),
        slot: None,
        include: Box::default(),
        env: Default::default(),
        shader_dir: None,
        texture_dir: None,
        effect_files: Box::default(),
        official: false,
        registry: None,
        enabled: true,
    }];
    assert!(package_in_catalog_for_arch(&fx, &listed, "64"));
    let cot6_a = ReshadePackage {
            kind: ReshadePackageKind::Addon,
            name: "Editor History by seri14".into(),
            description: String::new(),
            url: Some("https://github.com/cot6/reshade-addons/releases/download/setup-release-reference/ReShade64-EditorHistory-By-seri14.zip".into()),
        url32: None,
            repository_url: Some("https://github.com/cot6/reshade-addons".into()),
            shader_dir: None,
            texture_dir: None,
            deny_files: Box::default(),
            effect_files: Box::default(),
            in_catalog: false,
            in_catalog_32: false,
        };
    let mut listed_cot6 = listed.clone();
    listed_cot6[0].id = "editor-history-by-seri14-x64".into();
    listed_cot6[0].source = crate::instance::SourceRef::Github {
        owner: "cot6".into(),
        repo: "reshade-addons".into(),
        asset_glob: "ReShade64-EditorHistory-By-seri14.zip".into(),
        tag: Some("setup-release-reference".into()),
        prerelease: false,
    };
    listed_cot6[0].official = false;
    assert!(package_in_catalog_for_arch(&cot6_a, &listed_cot6, "64"));
    let cot6_b = ReshadePackage {
            kind: ReshadePackageKind::Addon,
            name: "Screenshot by seri14".into(),
            description: String::new(),
            url: Some("https://github.com/cot6/reshade-addons/releases/download/setup-release-reference/ReShade64-Screenshot-By-seri14.zip".into()),
        url32: None,
            repository_url: Some("https://github.com/cot6/reshade-addons".into()),
            shader_dir: None,
            texture_dir: None,
            deny_files: Box::default(),
            effect_files: Box::default(),
            in_catalog: false,
            in_catalog_32: false,
        };
    assert!(!package_in_catalog_for_arch(&cot6_b, &listed_cot6, "64"));
    let toggler = ReshadePackage {
        kind: ReshadePackageKind::Addon,
        name: "Example toggler by someone".into(),
        description: String::new(),
        url: Some(
            "https://github.com/example/ExampleAddon/releases/download/1.2.2/ExampleAddon_v122.zip"
                .into(),
        ),
        url32: None,
        repository_url: Some("https://github.com/example/ExampleAddon".into()),
        shader_dir: None,
        texture_dir: None,
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    };
    listed_cot6[0].source = crate::instance::SourceRef::Github {
        owner: "example".into(),
        repo: "ExampleAddon".into(),
        asset_glob: "ExampleAddon_v122.zip".into(),
        tag: None,
        prerelease: false,
    };
    assert!(package_in_catalog_for_arch(&toggler, &listed_cot6, "64"));
}

#[test]
fn parse_reshade_downloadurl32_for_32bit() {
    let ini = "[00]\nPackageName=X\nDownloadUrl=https://example.com/x.zip\nDownloadUrl32=https://example.com/x32.zip\nDownloadUrl64=https://example.com/x64.zip\n";
    let pkgs = parse_reshade_packages(ini, "");
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].url.as_deref(), Some("https://example.com/x64.zip"));
    assert_eq!(
        pkgs[0].url32.as_deref(),
        Some("https://example.com/x32.zip")
    );
    assert_eq!(
        pkgs[0].url_for_arch("32").unwrap(),
        "https://example.com/x32.zip"
    );
    assert_eq!(
        pkgs[0].url_for_arch("64").unwrap(),
        "https://example.com/x64.zip"
    );
    assert_eq!(
        pkgs[0].url_for_arch("").unwrap(),
        "https://example.com/x64.zip"
    );
}

#[test]
fn reshade_package_mint_picks_32_url() {
    // R38: one package, both URLs → two co-installable qualified mints in
    // the same catalog (replaces the old distinct-name workaround).
    let pkg = crate::download::ReshadePackage {
        kind: crate::download::ReshadePackageKind::Effect,
        name: "x".into(),
        description: "d".into(),
        url: Some("https://example.com/x64.zip".into()),
        url32: Some("https://example.com/x32.zip".into()),
        repository_url: None,
        shader_dir: None,
        texture_dir: None,
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    };
    // Mint and check the resulting Mod's source URL (RecipeSpec source is private)
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-mint32-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let target64 = crate::instance::MintTarget {
        appid: Some(10),
        title: "Game 64".into(),
    };
    let spec64 =
        crate::instance::RecipeSpec::reshade_package_for_game(&pkg, "64", &target64).unwrap();
    let m64 = crate::instance::mint_recipe(&cfg, &data, spec64).unwrap();
    assert_eq!(m64.id, "x-x64");
    match m64.source {
        crate::instance::SourceRef::ManualUrl { url } => {
            assert_eq!(url, "https://example.com/x64.zip")
        }
        _ => panic!("{m64:?}"),
    }
    // Same package, same catalog: the 32-bit variant coexists, bound to
    // its own target game.
    let target32 = crate::instance::MintTarget {
        appid: Some(11),
        title: "Game 32".into(),
    };
    let spec32 =
        crate::instance::RecipeSpec::reshade_package_for_game(&pkg, "32", &target32).unwrap();
    let m32 = crate::instance::mint_recipe(&cfg, &data, spec32).unwrap();
    assert_eq!(m32.id, "x-x32");
    assert_eq!(&m32.appids[..], [11]);
    assert!(
        m32.games.is_empty(),
        "appid target must not also glob titles"
    );
    match m32.source {
        crate::instance::SourceRef::ManualUrl { url } => {
            assert_eq!(url, "https://example.com/x32.zip")
        }
        _ => panic!("{m32:?}"),
    }
    // A second mint of the same arch refuses: the variant is taken.
    let again =
        crate::instance::RecipeSpec::reshade_package_for_game(&pkg, "32", &target32).unwrap();
    assert!(crate::instance::mint_recipe(&cfg, &data, again).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// R38: a 32-bit target must fail closed without `DownloadUrl32`. The
/// old `url_for_arch` fell back to the 64-bit `url`, which would mint
/// 64-bit bytes for a 32-bit game under a 32-bit label.
#[test]
fn url_for_arch_32_never_falls_back_to_64() {
    let ini = "[00]\nPackageName=X\nDownloadUrl64=https://example.com/x64.zip\n";
    let pkgs = parse_reshade_packages(ini, "");
    assert_eq!(pkgs.len(), 1);
    assert_eq!(
        pkgs[0].url_for_arch("64").map(String::as_str),
        Some("https://example.com/x64.zip")
    );
    assert!(
        pkgs[0].url_for_arch("32").is_none(),
        "32-bit must not borrow the 64-bit URL"
    );
    // A 32-bit mint of that package refuses instead of writing a recipe.
    let target = crate::instance::MintTarget {
        appid: Some(1),
        title: "X".into(),
    };
    let err = match crate::instance::RecipeSpec::reshade_package_for_game(&pkgs[0], "32", &target) {
        Ok(_) => panic!("32-bit mint must refuse a package with no DownloadUrl32"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("DownloadUrl32"),
        "error must name the missing 32-bit URL, got: {err}"
    );
    // 64-bit still mints fine for the same package.
    assert!(crate::instance::RecipeSpec::reshade_package_for_game(&pkgs[0], "64", &target).is_ok());
}

/// R38: a title-only target binds by glob; a target with neither an
/// AppID nor a title is refused so no recipe is ever minted unbound.
#[test]
fn reshade_mint_target_binding_rules() {
    let pkg = |name: &str| crate::download::ReshadePackage {
        kind: crate::download::ReshadePackageKind::Addon,
        name: name.into(),
        description: String::new(),
        url: Some("https://example.com/x64.zip".into()),
        url32: Some("https://example.com/x32.zip".into()),
        repository_url: None,
        shader_dir: None,
        texture_dir: None,
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    };
    let by_title = crate::instance::MintTarget {
        appid: None,
        title: "Portal 2".into(),
    };
    let spec =
        crate::instance::RecipeSpec::reshade_package_for_game(&pkg("t"), "32", &by_title).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-r38-target-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("cfg")).unwrap();
    std::fs::create_dir_all(dir.join("data")).unwrap();
    let m = crate::instance::mint_recipe(&dir.join("cfg"), &dir.join("data"), spec).unwrap();
    assert_eq!(&m.games[..], ["*Portal 2*"]);
    assert!(m.appids.is_empty());
    assert_eq!(m.mod_type, "reshade_addon");
    // The binding is persisted, so the 32-bit recipe is not game-wide.
    let written = std::fs::read_to_string(dir.join("data/mods/user/t-x32.toml")).unwrap();
    assert!(written.contains(r#"games = ["*Portal 2*"]"#), "{written}");
    let _ = std::fs::remove_dir_all(&dir);

    for bad in [
        crate::instance::MintTarget {
            appid: Some(0),
            title: "X".into(),
        },
        crate::instance::MintTarget {
            appid: None,
            title: "  ".into(),
        },
    ] {
        assert!(
            crate::instance::RecipeSpec::reshade_package_for_game(&pkg("b"), "32", &bad).is_err(),
            "unbindable target must be refused"
        );
    }
}
