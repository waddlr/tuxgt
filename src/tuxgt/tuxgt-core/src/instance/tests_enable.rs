use super::testing::*;
use super::*;
use crate::Error;
use std::fs;

#[test]
fn payload_rules_parse_and_validate() {
    let ok = parse_recipe(
            "id = \"x\"\ntype = \"reshade\"\nlabel = \"x\"\n[[payload]]\narch = \"64\"\nkeep = [\"ReShade64.dll\"]\ndrop = [\"*.json\"]\n[[payload]]\napi = \"vulkan\"\ndrop = [\"*vk.dll\"]\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(ok.payload.len(), 2);
    assert_eq!(ok.payload[0].arch.as_deref(), Some("64"));
    assert_eq!(&ok.payload[0].keep[..], ["ReShade64.dll"]);
    assert_eq!(ok.payload[1].api.as_deref(), Some("vulkan"));
    let bad_arch = parse_recipe(
            "id = \"x\"\ntype = \"reshade\"\nlabel = \"x\"\n[[payload]]\narch = \"arm\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        );
    assert!(bad_arch.is_err());
    let plain = parse_recipe(
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert!(plain.payload.is_empty());
}

#[test]
fn official_reshade_has_arch_payload() {
    let list = official_mods(&official_mods_dir(&temp_config())).unwrap();
    let reshade = list.iter().find(|i| i.id == "reshade").unwrap();
    assert_eq!(reshade.payload.len(), 2);
    assert!(reshade
        .payload
        .iter()
        .any(|r| r.arch.as_deref() == Some("64")));
    assert!(
        matches!(&reshade.source, SourceRef::ManualUrl { url } if url == "https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe")
    );
    let opti = list.iter().find(|i| i.id == "optiscaler").unwrap();
    assert_eq!(opti.payload.len(), 1);
    assert!(opti.payload[0].keep.is_empty());
    for g in ["*.bat", "*.reg", "*.md", "*README*.txt"] {
        assert!(opti.payload[0].drop.iter().any(|d| d == g), "{g}");
    }
}

#[test]
fn official_optiscaler_is_github_latest() {
    let list = official_mods(&official_mods_dir(&temp_config())).unwrap();
    let opti = list.iter().find(|i| i.id == "optiscaler").unwrap();
    assert_eq!(opti.mod_type, "optiscaler");
    match &opti.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            assert!(!prerelease);
            assert_eq!(owner, "optiscaler");
            assert_eq!(repo, "OptiScaler");
            assert_eq!(asset_glob, "OptiScaler_*.7z");
            assert_eq!(tag.as_deref(), None);
        }
        other => panic!("{other:?}"),
    }
    assert!(opti.official);
    assert!(opti.plans_allowed.contains(&Plan::ProtonEnv));
}

#[test]
fn enable_disable_persists_and_filters() {
    let dir = temp_config();
    let listed = list_mods(&dir, &dir).unwrap();
    assert!(listed.mods.iter().all(|i| i.enabled));
    assert!(!disabled_path(&dir).exists());

    disable_mod(&dir, "reshade", &dir).unwrap();
    disable_mod(&dir, "reshade", &dir).unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    let r = listed.mods.iter().find(|i| i.id == "reshade").unwrap();
    assert!(r.official);
    assert!(!r.enabled);
    assert!(listed
        .mods
        .iter()
        .any(|i| i.id == "optiscaler" && i.enabled));
    let for_game = mods_for_game(&dir, "No Such Game", None, &dir).unwrap();
    assert!(!for_game.mods.iter().any(|i| i.id == "reshade"));
    assert!(for_game.mods.iter().any(|i| i.id == "optiscaler"));

    enable_mod(&dir, "reshade", &dir).unwrap();
    enable_mod(&dir, "reshade", &dir).unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert!(
        listed
            .mods
            .iter()
            .find(|i| i.id == "reshade")
            .unwrap()
            .enabled
    );
    assert!(mods_for_game(&dir, "No Such Game", None, &dir)
        .unwrap()
        .mods
        .iter()
        .any(|i| i.id == "reshade"));

    assert!(matches!(
        disable_mod(&dir, "no-such", &dir).unwrap_err(),
        Error::UnknownInstance(_)
    ));
    assert!(remove_mod(&dir, &dir, "reshade").is_err());
    assert!(disabled_path(&dir).exists());
}

#[test]
fn stale_disabled_ids_kept_across_write() {
    let dir = temp_config();
    fs::create_dir_all(&dir).unwrap();
    fs::write(disabled_path(&dir), "disabled = [\"reshade\", \"ghost\"]\n").unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert!(!listed.mods.iter().any(|i| i.id == "ghost"));
    assert!(
        !listed
            .mods
            .iter()
            .find(|i| i.id == "reshade")
            .unwrap()
            .enabled
    );

    disable_mod(&dir, "optiscaler", &dir).unwrap();
    let parsed: ModsFile =
        toml::from_str(&fs::read_to_string(disabled_path(&dir)).unwrap()).unwrap();
    assert!(parsed.disabled.iter().any(|id| id == "ghost"));
    assert!(parsed.disabled.iter().any(|id| id == "reshade"));
    assert!(parsed.disabled.iter().any(|id| id == "optiscaler"));

    enable_mod(&dir, "reshade", &dir).unwrap();
    let parsed: ModsFile =
        toml::from_str(&fs::read_to_string(disabled_path(&dir)).unwrap()).unwrap();
    assert!(parsed.disabled.iter().any(|id| id == "ghost"));
    assert!(!parsed.disabled.iter().any(|id| id == "reshade"));
    assert!(
        list_mods(&dir, &dir)
            .unwrap()
            .mods
            .iter()
            .find(|i| i.id == "reshade")
            .unwrap()
            .enabled
    );
}

#[test]
fn disable_user_instance_does_not_delete_sample_recipe() {
    let dir = temp_config();
    let src = dir.join("src.toml");
    fs::create_dir_all(&dir).unwrap();
    fs::write(&src, sample_recipe("fork", "\"preload\"")).unwrap();
    add_mod(&dir, &src, &dir).unwrap();
    disable_mod(&dir, "fork", &dir).unwrap();
    assert!(user_mods_dir(&dir).join("fork.toml").exists());
    assert!(
        !list_mods(&dir, &dir)
            .unwrap()
            .mods
            .iter()
            .find(|i| i.id == "fork")
            .unwrap()
            .enabled
    );
    assert!(!mods_for_game(&dir, "No Such Game", None, &dir)
        .unwrap()
        .mods
        .iter()
        .any(|i| i.id == "fork"));
    enable_mod(&dir, "fork", &dir).unwrap();
    assert!(
        list_mods(&dir, &dir)
            .unwrap()
            .mods
            .iter()
            .find(|i| i.id == "fork")
            .unwrap()
            .enabled
    );
}

#[test]
fn dests_table_parses_and_unknown_recipe_keys_denied() {
    let ok = parse_recipe(
            "id = \"x\"\ntype = \"optiscaler\"\nlabel = \"x\"\n[dests]\n\"OptiScaler.dll\" = \"dxgi.dll\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        )
        .unwrap();
    assert_eq!(
        ok.dests.get("OptiScaler.dll").map(String::as_str),
        Some("dxgi.dll")
    );
    let extra = parse_recipe(
            "id = \"x\"\ntype = \"optiscaler\"\nlabel = \"x\"\nnope = 1\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        );
    assert!(extra.is_err());
    let empty_dest = parse_recipe(
            "id = \"x\"\ntype = \"optiscaler\"\nlabel = \"x\"\n[dests]\n\"a.dll\" = \"\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            false,
        );
    assert!(empty_dest.is_err());
}
