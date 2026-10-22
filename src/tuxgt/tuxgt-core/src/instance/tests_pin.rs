use super::testing::{repo_mods_dir, temp_config};
use super::*;
use std::fs;
use std::path::Path;

pub(super) const RE_68: &str = "https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe";
const RE_67: &str = "https://reshade.me/downloads/ReShade_Setup_6.7.0_Addon.exe";
const OPTI: &str =
    "https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/OptiScaler_0.9.4.7z";

pub(super) fn user_tomls(data: &Path) -> Vec<String> {
    let dir = data.join("mods").join("user");
    let mut names = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "toml") {
                names.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    names
}

pub(super) fn write_user(data: &Path, text: &str) {
    let dir = user_mods_dir(data);
    fs::create_dir_all(&dir).unwrap();
    let id = text
        .lines()
        .find_map(|l| l.strip_prefix("id = \"").and_then(|r| r.strip_suffix('"')))
        .unwrap();
    fs::write(dir.join(format!("{id}.toml")), text).unwrap();
}

fn prov(source: &str) -> crate::ModProvenance {
    crate::ModProvenance {
        source: source.into(),
        asset_sha256: "ab".repeat(32),
        asset_bytes: 4,
        fetched_at: 1,
    }
}

pub(super) fn seed_payload(data: &Path, official: bool, id: &str, source: &str) {
    let dir = payload_dir(data, official, None, id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("payload.bin"), b"bytes").unwrap();
    write_payload_provenance(&dir, &prov(source));
}

fn manifest(game: &str, instance: &str, source: &str) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        files: Box::default(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        include: Box::default(),
        harvested: Default::default(),
        provenance: prov(source),
    }
}

#[test]
fn parse_accepts_pin_of_on_manual_and_tagged_github() {
    let manual = r#"
id = "reshade-6-8-0"
type = "reshade"
label = "ReShade 6.8.0"
pin_of = "reshade"
[source]
type = "manual_url"
url = "https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe"
"#;
    let m = parse_recipe(manual, false).unwrap();
    assert_eq!(m.pin_of.as_deref(), Some("reshade"));
    assert!(m.sha256.is_none());
    let text = export_recipe_text(&m, Path::new("/tmp"), None).unwrap();
    assert!(text.contains("pin_of = \"reshade\""), "{text}");
    let gh = r#"
id = "optiscaler-0-9-4"
type = "optiscaler"
label = "OptiScaler 0.9.4"
pin_of = "optiscaler"
[source]
type = "github"
owner = "optiscaler"
repo = "OptiScaler"
asset_glob = "OptiScaler_0.9.4.7z"
tag = "v0.9.4"
"#;
    let g = parse_recipe(gh, false).unwrap();
    assert_eq!(g.pin_of.as_deref(), Some("optiscaler"));
}

#[test]
fn parse_rejects_bad_pin_of() {
    let base = |extra: &str| {
        format!(
            "id = \"reshade-6-8-0\"\ntype = \"reshade\"\nlabel = \"R\"\n{extra}\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/a.zip\"\n"
        )
    };
    let empty = parse_recipe(&base("pin_of = \"\""), false).unwrap_err();
    assert!(empty.to_string().contains("bad pin_of"), "{empty}");
    let bad = parse_recipe(&base("pin_of = \"ReShade\""), false).unwrap_err();
    assert!(bad.to_string().contains("bad pin_of"), "{bad}");
    let both = parse_recipe(
        &base("pin_of = \"reshade\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""),
        false,
    )
    .unwrap_err();
    assert!(
        both.to_string().contains("pin_of is not a recipe sha"),
        "{both}"
    );
}

#[tokio::test]
async fn pin_reshade_writes_one_user_copy_and_second_call_is_already() {
    let data = temp_config();
    let parent = fs::read(data.join("mods/official/reshade.toml")).unwrap();
    seed_payload(&data, true, "reshade", RE_68);
    let first = pin_version(&data, &data, "reshade", None).await.unwrap();
    assert!(!first.already);
    assert_eq!(first.id, "reshade-6-8-0");
    assert_eq!(first.label, "ReShade 6.8.0");
    assert_eq!(first.version, "6.8.0");
    let text = fs::read_to_string(data.join("mods/user/reshade-6-8-0.toml")).unwrap();
    let copy = parse_recipe(&text, false).unwrap();
    assert_eq!(copy.pin_of.as_deref(), Some("reshade"));
    assert!(copy.sha256.is_none());
    assert!(!text.contains("sha256"));
    match copy.source {
        SourceRef::ManualUrl { url } => assert_eq!(url, RE_68),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs::read(data.join("mods/official/reshade.toml")).unwrap(),
        parent
    );
    assert!(data
        .join("mods/user/reshade-6-8-0/.provenance.toml")
        .is_file());
    let second = pin_version(&data, &data, "reshade", None).await.unwrap();
    assert!(second.already);
    assert_eq!(second.id, "reshade-6-8-0");
    assert_eq!(user_tomls(&data), vec!["reshade-6-8-0.toml".to_string()]);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn pin_refuses_sha_tag_local_provided_and_a_copy() {
    let data = temp_config();
    write_user(
        &data,
        "id = \"localmod\"\ntype = \"custom\"\nlabel = \"L\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
    );
    write_user(
        &data,
        "id = \"given\"\ntype = \"custom\"\nlabel = \"G\"\n[source]\ntype = \"provided\"\nfiles = [\"a.dll\"]\n",
    );
    let before = user_tomls(&data);
    for id in [
        "d3dcompiler-47",
        "optiscaler-y4my4m-v4",
        "localmod",
        "given",
    ] {
        let err = pin_version(&data, &data, id, None).await.unwrap_err();
        assert!(
            matches!(err, crate::Error::InvalidInstance(_)),
            "{id}: {err}"
        );
    }
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    let err = pin_version(&data, &data, &made.id, None).await.unwrap_err();
    assert!(matches!(err, crate::Error::InvalidInstance(_)), "{err}");
    assert_eq!(user_tomls(&data), {
        let mut names = before;
        names.push(format!("{}.toml", made.id));
        names.sort();
        names
    });
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn game_pin_uses_that_install_and_skips_a_newer_payload() {
    let data = temp_config();
    seed_payload(&data, true, "reshade", RE_68);
    let game = "manual:standalone:pin67";
    crate::write_manifest(&data, &manifest(game, "reshade", RE_67)).unwrap();
    let made = pin_version(&data, &data, "reshade", Some(game))
        .await
        .unwrap();
    assert_eq!(made.version, "6.7.0");
    assert_eq!(made.id, "reshade-6-7-0");
    let child = payload_dir(&data, false, None, &made.id);
    assert!(!child.join("payload.bin").exists());
    assert!(!child.join(".provenance.toml").exists());
    let text = fs::read_to_string(data.join(format!("mods/user/{}.toml", made.id))).unwrap();
    assert!(text.contains(RE_67), "{text}");
    assert!(!text.contains("6.8.0"), "{text}");
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn equal_sources_copy_provenance() {
    let data = temp_config();
    seed_payload(&data, true, "reshade", RE_68);
    let game = "manual:standalone:pin68";
    crate::write_manifest(&data, &manifest(game, "reshade", RE_68)).unwrap();
    let made = pin_version(&data, &data, "reshade", Some(game))
        .await
        .unwrap();
    let copied =
        fs::read_to_string(payload_dir(&data, false, None, &made.id).join(".provenance.toml"))
            .unwrap();
    assert!(copied.contains(RE_68), "{copied}");
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn empty_manifest_provenance_writes_nothing() {
    let data = temp_config();
    let game = "manual:standalone:pinemp";
    crate::write_manifest(&data, &manifest(game, "reshade", "")).unwrap();
    let err = pin_version(&data, &data, "reshade", Some(game))
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::InvalidInstance(_)), "{err}");
    assert!(user_tomls(&data).is_empty());
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn parent_url_move_updates_parent_only() {
    let data = temp_config();
    seed_payload(&data, true, "reshade", RE_68);
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    let official = data.join("mods/official/reshade.toml");
    let text = fs::read_to_string(&official)
        .unwrap()
        .replace("6.8.0", "6.9.0");
    fs::write(&official, text).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    match crate::check_catalog_update(&pool, &data, &data, "reshade")
        .await
        .unwrap()
    {
        crate::CatalogStatus::Available { detail } => {
            assert!(detail.contains("source URL moved"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        crate::check_catalog_update(&pool, &data, &data, &made.id)
            .await
            .unwrap(),
        crate::CatalogStatus::UpToDate
    );
    let game = "manual:standalone:pincopy";
    crate::write_manifest(&data, &manifest(game, &made.id, RE_68)).unwrap();
    assert_eq!(
        crate::check_update(&data, &data, game, &made.id)
            .await
            .unwrap(),
        crate::UpdateStatus::UpToDate
    );
    let fresh = temp_config();
    let empty = pin_version(&fresh, &fresh, "reshade", None).await.unwrap();
    let pool2 = crate::open_db(&fresh).await.unwrap();
    match crate::check_catalog_update(&pool2, &fresh, &fresh, &empty.id)
        .await
        .unwrap()
    {
        crate::CatalogStatus::Unknown { reason } => {
            assert!(reason.contains("pinned version unavailable"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    let game2 = "manual:standalone:pinmiss";
    crate::write_manifest(
        &fresh,
        &manifest(game2, &empty.id, "https://example.com/other.exe"),
    )
    .unwrap();
    match crate::check_update(&fresh, &fresh, game2, &empty.id)
        .await
        .unwrap()
    {
        crate::UpdateStatus::Unknown { reason } => {
            assert!(reason.contains("pinned version unavailable"), "{reason}");
        }
        other => panic!("pin check returned {other:?}"),
    }
    let _ = fs::remove_dir_all(&data);
    let _ = fs::remove_dir_all(&fresh);
}

#[tokio::test]
async fn github_download_url_freezes_tag_and_literal_asset() {
    let data = temp_config();
    seed_payload(&data, true, "optiscaler", OPTI);
    let made = pin_version(&data, &data, "optiscaler", None).await.unwrap();
    assert!(made.label.ends_with("0.9.4"), "{}", made.label);
    assert_eq!(made.version, "0.9.4");
    let text = fs::read_to_string(data.join(format!("mods/user/{}.toml", made.id))).unwrap();
    let copy = parse_recipe(&text, false).unwrap();
    assert!(copy.sha256.is_none());
    assert!(!text.contains("sha256"));
    match copy.source {
        SourceRef::Github {
            tag,
            asset_glob,
            prerelease,
            ..
        } => {
            assert_eq!(tag.as_deref(), Some("v0.9.4"));
            assert_eq!(asset_glob, "OptiScaler_0.9.4.7z");
            assert!(!prerelease);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        fs::read(data.join("mods/official/optiscaler.toml")).unwrap(),
        fs::read(repo_mods_dir().join("optiscaler.toml")).unwrap()
    );
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn slug_collision_uses_suffix() {
    let data = temp_config();
    write_user(
        &data,
        "id = \"reshade-6-8-0\"\ntype = \"custom\"\nlabel = \"other\"\n[source]\ntype = \"local\"\npath = \"/tmp/x\"\n",
    );
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    assert_eq!(made.id, "reshade-6-8-0-2");
    assert!(data.join("mods/user/reshade-6-8-0-2.toml").is_file());
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn pin_baseline_does_not_repair_install() {
    let data = temp_config();
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    let game = "manual:standalone:pinbase";
    crate::write_manifest(&data, &manifest(game, &made.id, "")).unwrap();
    let pool = crate::open_db(&data).await.unwrap();
    let base = crate::ensure_update_baseline(&pool, &data, &data, game, &made.id)
        .await
        .unwrap();
    assert!(!base.installed);
    match base.status {
        crate::UpdateStatus::Unknown { reason } => {
            assert!(reason.contains("pinned version unavailable"), "{reason}");
            assert_eq!(crate::short_update_reason(&reason), "gui-pin-unavailable");
        }
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&data);
}
