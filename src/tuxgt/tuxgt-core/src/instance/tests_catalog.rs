use super::testing::*;
use super::*;
use crate::parse_mod_type;
use std::fs;

#[test]
fn officials_parse() {
    let list = official_mods(&official_mods_dir(&temp_config())).unwrap();
    assert_eq!(list.len(), n_official());
    assert!(list.iter().all(|i| i.official));
    let opti = list.iter().find(|i| i.id == "optiscaler").unwrap();
    assert!(opti.plans_allowed.contains(&Plan::ProtonEnv));
    let reshade = list.iter().find(|i| i.id == "reshade").unwrap();
    assert!(!reshade.plans_allowed.contains(&Plan::ProtonEnv));
}

#[test]
fn official_catalog_covers_types_and_games() {
    let list = official_mods(&official_mods_dir(&temp_config())).unwrap();
    assert_eq!(list.len(), 3);
    for (id, ty) in [
        ("reshade", "reshade"),
        ("optiscaler", "optiscaler"),
        ("d3dcompiler-47", "custom"),
    ] {
        let i = list.iter().find(|i| i.id == id).unwrap();
        assert_eq!(i.mod_type, ty, "{id}");
        assert!(parse_mod_type(&i.mod_type).is_ok(), "{id}");
    }
    // Per-game catalog is empty now; every official is any-game.
    assert!(list.iter().all(|i| i.games.is_empty()));
}

#[test]
fn mods_for_game_filters_by_display_name() {
    let dir = temp_config();
    let list = mods_for_game(&dir, "Cyberpunk 2077", None, &dir).unwrap();
    let ids: Vec<&str> = list.mods.iter().map(|i| i.id.as_str()).collect();
    // Per-game catalog is empty now; Cyberpunk matches only any-game officials.
    assert!(ids.contains(&"reshade"), "{ids:?}");
    let per_game: Vec<&str> = list
        .mods
        .iter()
        .filter(|i| !i.games.is_empty())
        .map(|i| i.id.as_str())
        .collect();
    assert!(per_game.is_empty(), "{ids:?}");
    // Case-insensitive.
    let lower = mods_for_game(&dir, "cyberpunk 2077", None, &dir).unwrap();
    assert_eq!(lower.mods.len(), list.mods.len());
    // Unknown name keeps only any-game recipes.
    let none = mods_for_game(&dir, "No Such Game", None, &dir).unwrap();
    assert!(none.mods.iter().all(|i| i.games.is_empty()));
    assert!(none.mods.iter().any(|i| i.id == "reshade"));
    // Empty name = no per-game recipe.
    let empty = mods_for_game(&dir, "", None, &dir).unwrap();
    assert!(empty.mods.iter().all(|i| i.games.is_empty()));
}

#[test]
fn games_and_tag_validate() {
    let bad_games = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\ngames = [\"  \"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert!(parse_recipe(bad_games, false).is_err());
    let ok_games = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\ngames = [\" *Ace Combat 7* \"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert_eq!(
        &parse_recipe(ok_games, false).unwrap().games[..],
        ["*Ace Combat 7*"]
    );
    let bad_tag = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\n[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"a.zip\"\ntag = \"a/b\"\n";
    assert!(parse_recipe(bad_tag, false).is_err());
    let ok_tag = "id = \"x\"\ntype = \"effect\"\nlabel = \"x\"\n[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"a.zip\"\ntag = \"nightly-20260909\"\n";
    match parse_recipe(ok_tag, false).unwrap().source {
        SourceRef::Github { tag, .. } => assert_eq!(tag.as_deref(), Some("nightly-20260909")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn appids_and_prerelease_validate() {
    let github = |extra: &str| {
        format!(
                "id = \"x\"\ntype = \"reshade_addon\"\nlabel = \"x\"\n{extra}[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"a.addon64\"\n"
            )
    };
    // Nonzero entries parse; absent and empty are allowed.
    let m = parse_recipe(&github("appids = [1245620, 1578050]\n"), false).unwrap();
    assert_eq!(&m.appids[..], [1245620, 1578050]);
    assert!(parse_recipe(&github(""), false).unwrap().appids.is_empty());
    // Zero appid rejected.
    assert!(parse_recipe(&github("appids = [0]\n"), false).is_err());
    // prerelease defaults off; prerelease alone parses.
    match parse_recipe(&github(""), false).unwrap().source {
        SourceRef::Github { prerelease, .. } => assert!(!prerelease),
        other => panic!("{other:?}"),
    }
    let pre = "id = \"x\"\ntype = \"reshade_addon\"\nlabel = \"x\"\n[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"a.addon64\"\nprerelease = true\n";
    assert!(parse_recipe(pre, false).is_ok());
    // prerelease contradicts a pinned tag and a sha256 pin.
    let pre_tag = pre.replace(
        "asset_glob = \"a.addon64\"",
        "asset_glob = \"a.addon64\"\ntag = \"nightly-20260909\"",
    );
    assert!(parse_recipe(&pre_tag, false).is_err());
    let pre_sha = pre.replace("[source]", "sha256 = \"aa\"\n[source]");
    assert!(parse_recipe(&pre_sha, false).is_err());
    // prerelease is a github-source key.
    let local_pre = "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\nprerelease = true\n";
    assert!(parse_recipe(local_pre, false).is_err());
    let url_pre = "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/x.zip\"\nprerelease = true\n";
    assert!(parse_recipe(url_pre, false).is_err());
}

#[test]
fn mods_for_game_matches_by_appid() {
    let dir = temp_config();
    fs::create_dir_all(user_mods_dir(&dir)).unwrap();
    fs::write(
            user_mods_dir(&dir).join("appmod.toml"),
            "id = \"appmod\"\ntype = \"reshade_addon\"\nlabel = \"x\"\nappids = [1245620]\n[source]\ntype = \"github\"\nowner = \"o\"\nrepo = \"r\"\nasset_glob = \"a.addon64\"\n",
        )
        .unwrap();
    // Matching AppID: visible even though the name matches no glob.
    let hit = mods_for_game(&dir, "No Such Game", Some(1245620), &dir).unwrap();
    assert!(
        hit.mods.iter().any(|i| i.id == "appmod"),
        "{:?}",
        hit.mods.iter().map(|i| &i.id).collect::<Vec<_>>()
    );
    // Another AppID: hidden.
    let miss = mods_for_game(&dir, "No Such Game", Some(999), &dir).unwrap();
    assert!(!miss.mods.iter().any(|i| i.id == "appmod"));
    // No AppID: hidden (no games glob either).
    let none = mods_for_game(&dir, "No Such Game", None, &dir).unwrap();
    assert!(!none.mods.iter().any(|i| i.id == "appmod"));
    // OR rule: an any-game official still lists beside the AppID hit.
    let both = mods_for_game(&dir, "Cyberpunk 2077", Some(1245620), &dir).unwrap();
    assert!(both.mods.iter().any(|i| i.id == "appmod"));
    assert!(both.mods.iter().any(|i| i.id == "reshade"));
}

#[test]
fn family_templates_parse_and_validate() {
    let data = temp_config();
    let tdir = data.join("share").join("templates");
    fs::create_dir_all(&tdir).unwrap();
    fs::write(
        tdir.join("family-x.toml"),
        r#"id = "family-x"
label = "Family X"
type = "reshade_addon"

[family]
owner = "o"
repo = "r"
asset_glob = "x-*.addon64"
drop = ["dxgi.dll"]
"#,
    )
    .unwrap();
    let ts = list_templates(&data).unwrap();
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].mod_type, "reshade_addon");
    let f = ts[0].family.as_ref().unwrap();
    assert_eq!(f.owner, "o");
    assert_eq!(f.repo, "r");
    assert_eq!(f.asset_glob, "x-*.addon64");
    assert!(!f.prerelease);
    assert_eq!(&f.drop[..], ["dxgi.dll"]);
    // Missing required family keys error like official mods.
    fs::write(
        tdir.join("family-bad.toml"),
        "id = \"family-bad\"\nlabel = \"b\"\ntype = \"reshade_addon\"\n[family]\nowner = \"o\"\n",
    )
    .unwrap();
    assert!(list_templates(&data).is_err());
}
