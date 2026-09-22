use super::testing::*;
use super::*;
use crate::Error;
use std::fs;

#[test]
fn requires_parse_ok_self_and_bad_rejected() {
    let ok = parse_recipe(&ureq("fork", "requires = [\"reshade\"]\n"), false).unwrap();
    assert_eq!(&ok.requires[..], ["reshade"]);
    let plain = parse_recipe(&ureq("fork", ""), false).unwrap();
    assert!(plain.requires.is_empty());
    assert!(parse_recipe(&ureq("fork", "requires = [\"fork\"]\n"), false).is_err());
    assert!(parse_recipe(&ureq("fork", "requires = [\"Bad!\"]\n"), false).is_err());
    assert!(parse_recipe(&ureq("fork", "requires = [\"\"]\n"), false).is_err());
}

#[test]
fn unknown_requires_is_problem_row_and_add_error() {
    let dir = temp_config();
    let inst_dir = user_mods_dir(&dir);
    fs::create_dir_all(&inst_dir).unwrap();
    fs::write(
        inst_dir.join("badreq.toml"),
        ureq("badreq", "requires = [\"no-such-mod\"]\n"),
    )
    .unwrap();
    fs::write(
        inst_dir.join("okreq.toml"),
        ureq("okreq", "requires = [\"reshade\"]\n"),
    )
    .unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert!(listed.mods.iter().any(|i| i.id == "okreq"));
    assert!(!listed.mods.iter().any(|i| i.id == "badreq"));
    assert!(listed
        .problems
        .iter()
        .any(|p| p.file == "badreq.toml" && p.reason.contains("unknown requires")));
    let src = dir.join("src.toml");
    fs::write(&src, ureq("newmod", "requires = [\"no-such-mod\"]\n")).unwrap();
    let err = add_mod(&dir, &src, &dir).unwrap_err();
    assert!(err.to_string().contains("unknown requires"), "{err}");
}

#[test]
fn user_to_user_requires_accepted() {
    let dir = temp_config();
    let inst_dir = user_mods_dir(&dir);
    fs::create_dir_all(&inst_dir).unwrap();
    fs::write(inst_dir.join("b.toml"), ureq("mbase", "")).unwrap();
    fs::write(
        inst_dir.join("a.toml"),
        ureq("mdep", "requires = [\"mbase\"]\n"),
    )
    .unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert!(listed.problems.is_empty(), "{:?}", listed.problems);
    assert_eq!(
        listed
            .mods
            .iter()
            .find(|i| i.id == "mdep")
            .unwrap()
            .requires[..],
        ["mbase"]
    );
}

#[test]
fn rescan_preserves_requires() {
    let dir = temp_config();
    let pkg = dir.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("plug.dll"), b"dll").unwrap();
    let inst = add_mod_from(
        &dir,
        "custom",
        "myplug",
        &pkg,
        None,
        None,
        &dir,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    assert!(inst.requires.is_empty());
    let dest = user_mods_dir(&dir).join("myplug.toml");
    let text = fs::read_to_string(&dest).unwrap();
    fs::write(&dest, format!("requires = [\"reshade\"]\n{text}")).unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert_eq!(
        listed
            .mods
            .iter()
            .find(|i| i.id == "myplug")
            .unwrap()
            .requires[..],
        ["reshade"]
    );
    let re = rescan_mod(&dir, "myplug", None, None, &dir).unwrap();
    assert_eq!(&re.requires[..], ["reshade"]);
    assert!(fs::read_to_string(&dest).unwrap().contains("requires"));
}

/// A rescan rewrites the recipe from the scanned package: display-only
/// keys the scan cannot see must survive it.
#[test]
fn rescan_preserves_effect_files() {
    let dir = temp_config();
    let src = dir.join("src");
    fs::create_dir_all(src.join("Shaders")).unwrap();
    fs::write(src.join("Shaders").join("a.fx"), b"a").unwrap();
    crate::add_mod_from(
        &dir,
        "effect",
        "myfx",
        &src,
        None,
        None,
        &dir,
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let dest = user_mods_dir(&dir).join("myfx.toml");
    let text = fs::read_to_string(&dest).unwrap();
    fs::write(
        &dest,
        format!("effect_files = [\"A.fx\", \"B.fx\"]\n{text}"),
    )
    .unwrap();
    let re = rescan_mod(&dir, "myfx", None, None, &dir).unwrap();
    assert_eq!(&re.effect_files[..], ["A.fx", "B.fx"]);
    let back = parse_recipe(&fs::read_to_string(&dest).unwrap(), false).unwrap();
    assert_eq!(&back.effect_files[..], ["A.fx", "B.fx"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn env_table_parses_and_validates() {
    let ok = "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[env]\nFOO = \"1\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n";
    assert_eq!(
        parse_recipe(ok, false)
            .unwrap()
            .env
            .get("FOO")
            .map(String::as_str),
        Some("1")
    );
    for bad in [
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[env]\n\"bad key\" = \"1\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[env]\nFOO = \"\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
            "id = \"x\"\ntype = \"custom\"\nlabel = \"x\"\n[env]\nFOO = 1\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
        ] {
            assert!(parse_recipe(bad, false).is_err(), "{bad}");
        }
}

#[test]
fn official_d3dcompiler_47_row() {
    let officials = official_mods(&official_mods_dir(&temp_config())).unwrap();
    let d = officials.iter().find(|i| i.id == "d3dcompiler-47").unwrap();
    assert_eq!(d.mod_type, "custom");
    assert!(d.allows_adapter("preload"));
    assert!(!d.allows_adapter("install"));
    assert!(d.dests.is_empty());
    assert!(
        matches!(&d.source, SourceRef::ManualUrl { url } if url == "https://msdl.microsoft.com/download/symbols/d3dcompiler_47.dll/14C94C2D4b2000/d3dcompiler_47.dll")
    );
    assert_eq!(
        d.env.get("WINEDLLOVERRIDES").map(String::as_str),
        Some("d3dcompiler_47=n")
    );
}

#[test]
fn adapter_gate_follows_plans() {
    let only_preload = parse_recipe(&sample_recipe("fork", "\"preload\""), false).unwrap();
    assert!(only_preload.allows_adapter("preload"));
    assert!(!only_preload.allows_adapter("install"));
    assert!(!only_preload.allows_adapter("proton_env"));
    let officials = official_mods(&official_mods_dir(&temp_config())).unwrap();
    let opti = officials.iter().find(|i| i.id == "optiscaler").unwrap();
    assert!(opti.allows_adapter("preload"));
    assert!(opti.allows_adapter("install"));
}

#[test]
fn shadow_id_rejected() {
    let dir = temp_config();
    let path = dir.join("r.toml");
    fs::create_dir_all(&dir).unwrap();
    fs::write(&path, sample_recipe("reshade", "\"preload\"")).unwrap();
    assert!(add_mod(&dir, &path, &dir).is_err());
}

#[test]
fn add_list_remove_roundtrip() {
    let dir = temp_config();
    let src = dir.join("src.toml");
    fs::create_dir_all(&dir).unwrap();
    fs::write(&src, sample_recipe("fork", "\"preload\"")).unwrap();
    let inst = add_mod(&dir, &src, &dir).unwrap();
    assert!(!inst.official);
    let listed = list_mods(&dir, &dir).unwrap();
    assert_eq!(listed.mods.len(), n_official() + 1);
    assert!(listed.problems.is_empty());
    assert_eq!(listed.mods.last().unwrap().id, "fork");
    assert!(add_mod(&dir, &src, &dir).is_err());
    remove_mod(&dir, &dir, "fork").unwrap();
    assert_eq!(list_mods(&dir, &dir).unwrap().mods.len(), n_official());
    assert!(matches!(
        remove_mod(&dir, &dir, "fork").unwrap_err(),
        Error::UnknownInstance(_)
    ));
    assert!(remove_mod(&dir, &dir, "reshade").is_err());
}

#[test]
fn bad_file_does_not_break_list() {
    let dir = temp_config();
    let inst_dir = user_mods_dir(&dir);
    fs::create_dir_all(&inst_dir).unwrap();
    fs::write(
        inst_dir.join("good.toml"),
        sample_recipe("fork", "\"preload\""),
    )
    .unwrap();
    fs::write(inst_dir.join("bad.toml"), "id = \"broken\"\ntype = [1]\n").unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert_eq!(listed.mods.len(), n_official() + 1);
    assert_eq!(listed.problems.len(), 1);
    assert_eq!(listed.problems[0].file, "bad.toml");
}

#[test]
fn duplicate_user_id_ignored() {
    let dir = temp_config();
    let inst_dir = user_mods_dir(&dir);
    fs::create_dir_all(&inst_dir).unwrap();
    fs::write(
        inst_dir.join("a.toml"),
        sample_recipe("fork", "\"preload\""),
    )
    .unwrap();
    fs::write(
        inst_dir.join("b.toml"),
        sample_recipe("fork", "\"install\""),
    )
    .unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert_eq!(listed.mods.len(), n_official() + 1);
    assert_eq!(listed.problems.len(), 1);
    assert!(listed.problems[0].reason.contains("duplicates another"));
}

#[test]
fn shadow_file_ignored_official_wins() {
    let dir = temp_config();
    let inst_dir = user_mods_dir(&dir);
    fs::create_dir_all(&inst_dir).unwrap();
    fs::write(
        inst_dir.join("custom.toml"),
        sample_recipe("reshade", "\"preload\""),
    )
    .unwrap();
    let listed = list_mods(&dir, &dir).unwrap();
    assert_eq!(listed.mods.len(), n_official());
    assert!(listed.mods.iter().all(|i| i.official));
    assert_eq!(listed.problems.len(), 1);
    assert!(listed.problems[0].reason.contains("shadows an official"));
}
