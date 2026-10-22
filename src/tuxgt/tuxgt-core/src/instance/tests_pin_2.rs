use super::testing::temp_config;
use super::tests_pin::{seed_payload, user_tomls, write_user, RE_68};
use super::*;
use std::fs;

#[tokio::test]
async fn unknown_parent_is_invalid_instance() {
    let data = temp_config();
    let err = pin_version(&data, &data, "no-such-mod", None)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::InvalidInstance(_)), "{err}");
    assert!(user_tomls(&data).is_empty());
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn hex_stem_manual_url_writes_nothing() {
    let data = temp_config();
    let hex = "ab".repeat(32);
    write_user(
        &data,
        &format!(
            "id = \"hexparent\"\ntype = \"custom\"\nlabel = \"Hex\"\n[source]\ntype = \"manual_url\"\nurl = \"https://example.com/{hex}.zip\"\n"
        ),
    );
    let before = user_tomls(&data);
    let err = pin_version(&data, &data, "hexparent", None)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::InvalidInstance(_)), "{err}");
    assert_eq!(user_tomls(&data), before);
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn mismatched_payload_provenance_is_unavailable() {
    let data = temp_config();
    seed_payload(&data, true, "reshade", RE_68);
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    seed_payload(&data, false, &made.id, "https://example.com/other.exe");
    let pool = crate::open_db(&data).await.unwrap();
    match crate::check_catalog_update(&pool, &data, &data, &made.id)
        .await
        .unwrap()
    {
        crate::CatalogStatus::Unknown { reason } => {
            assert!(reason.contains("pinned version unavailable"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    let _ = fs::remove_dir_all(&data);
}

#[tokio::test]
async fn child_drops_proton_env() {
    let data = temp_config();
    let official = data.join("mods/official/reshade.toml");
    let text = fs::read_to_string(&official).unwrap().replace(
        "plans_allowed = [\"preload\", \"install\"]",
        "plans_allowed = [\"preload\", \"install\", \"proton_env\"]",
    );
    fs::write(&official, text).unwrap();
    seed_payload(&data, true, "reshade", RE_68);
    let made = pin_version(&data, &data, "reshade", None).await.unwrap();
    let child = fs::read_to_string(data.join(format!("mods/user/{}.toml", made.id))).unwrap();
    assert!(!child.contains("proton_env"), "{child}");
    let parsed = parse_recipe(&child, false).unwrap();
    assert!(!parsed.plans_allowed.contains(&Plan::ProtonEnv));
    assert!(parsed.plans_allowed.contains(&Plan::Preload));
    assert!(parsed.plans_allowed.contains(&Plan::Install));
    let _ = fs::remove_dir_all(&data);
}
