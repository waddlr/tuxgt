use super::*;
use crate::testing::{seed_game, SeedGame};

#[test]
fn install_links_is_a_distinct_install_kind() {
    assert_eq!(
        validate_adapter(" install-links ").unwrap(),
        ADAPTER_INSTALL_LINKS
    );
    assert!(is_install(ADAPTER_INSTALL));
    assert!(is_install(ADAPTER_INSTALL_LINKS));
    assert!(!is_install(ADAPTER_PRELOAD));
    assert!(is_links(ADAPTER_INSTALL_LINKS));
    assert!(!is_links(ADAPTER_INSTALL));
    assert!(!is_links(ADAPTER_PRELOAD));
    assert!(validate_adapter("install-copy").is_err());
}

async fn adapter_game(detected_api: Option<&str>) -> (std::path::PathBuf, sqlx::SqlitePool) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-install-links-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:links",
            manager: "manual",
            store: "standalone",
            game_id: "links",
            detected_api,
            ..Default::default()
        },
    )
    .await;
    (dir, pool)
}

#[tokio::test]
async fn install_links_persists_like_install() {
    let (_dir, pool) = adapter_game(Some("dx12")).await;
    set_game_adapter(&pool, "manual:standalone:links", ADAPTER_INSTALL_LINKS)
        .await
        .unwrap();
    assert_eq!(
        game_adapter(&pool, "manual:standalone:links")
            .await
            .unwrap(),
        ADAPTER_INSTALL_LINKS
    );
}

#[tokio::test]
async fn install_links_is_refused_for_vulkan() {
    let (_dir, pool) = adapter_game(Some("vulkan")).await;
    let err = set_game_adapter(&pool, "manual:standalone:links", ADAPTER_INSTALL_LINKS)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("vulkan"), "{err}");
    assert_eq!(
        game_adapter(&pool, "manual:standalone:links")
            .await
            .unwrap(),
        ADAPTER_PRELOAD
    );
}
