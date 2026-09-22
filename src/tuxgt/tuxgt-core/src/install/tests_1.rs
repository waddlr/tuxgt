use super::testing::*;
use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;
use std::fs;

#[tokio::test]
async fn prefix_root_missing_is_install_error() {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e73-prefix-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let data = dir.join("data");
    let pool = crate::open_db(&data).await.unwrap();
    let gid = "manual:standalone:e73native";
    seed_game(
        &pool,
        SeedGame {
            id: gid,
            name: Some("E73 native"),
            ..Default::default()
        },
    )
    .await;
    let err = prefix_root(&pool, gid).await.unwrap_err();
    assert!(matches!(err, Error::Install(_)), "{err}");
    let pfx = dir.join("pfx");
    fs::create_dir_all(&pfx).unwrap();
    sqlx::query("UPDATE games SET prefix_path = ? WHERE id = ?")
        .bind(pfx.to_str().unwrap())
        .bind(gid)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(prefix_root(&pool, gid).await.unwrap(), pfx);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_conflicts_groups_enabled_only() {
    let game = "manual:standalone:loadconf1";
    let (data, _, _) = setup(game);
    let mut a = test_manifest(game, "aaa", "shared.fx", "h1");
    a.adapter = "preload".into();
    a.load_order = 0;
    let mut b = test_manifest(game, "bbb", "shared.fx", "h2");
    b.adapter = "preload".into();
    b.load_order = 1;
    b.files[0].enabled = false; // omitted dest: no group
    crate::write_manifest(&data, &a).unwrap();
    crate::write_manifest(&data, &b).unwrap();
    assert!(load_conflicts(&data, game).unwrap().is_empty());
    // Enable the rival dest: one group, load order (winner last).
    b.files[0].enabled = true;
    crate::write_manifest(&data, &b).unwrap();
    let groups = load_conflicts(&data, game).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].dest, "shared.fx");
    assert_eq!(groups[0].adapter, "preload");
    assert_eq!(
        &groups[0].instances[..],
        ["aaa".to_string(), "bbb".to_string()]
    );
    // Disabled manifest drops out; cross-adapter same string ungrouped.
    b.enabled = false;
    crate::write_manifest(&data, &b).unwrap();
    assert!(load_conflicts(&data, game).unwrap().is_empty());
    b.enabled = true;
    b.adapter = "install".into();
    crate::write_manifest(&data, &b).unwrap();
    assert!(load_conflicts(&data, game).unwrap().is_empty());
    let _ = fs::remove_dir_all(&data);
}
