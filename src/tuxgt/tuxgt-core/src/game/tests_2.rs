use super::*;
use crate::testing::{seed_game, SeedGame};

async fn played(pool: &sqlx::SqlitePool, n: u32, at: i64) {
    let id = format!("steam::{n}");
    let game = n.to_string();
    let name = format!("Game {n}");
    seed_game(
        pool,
        SeedGame {
            id: &id,
            manager: "steam",
            store: "",
            game_id: &game,
            name: Some(&name),
            last_played: Some(at),
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn recent_games_orders_by_last_played_desc_and_caps() {
    let dir = std::env::temp_dir().join(format!("tuxgt-recents-cap-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    for (n, at) in [
        (1, 30),
        (2, 10),
        (3, 70),
        (4, 20),
        (5, 60),
        (6, 40),
        (7, 50),
    ] {
        played(&pool, n, at).await;
    }
    let rows = recent_games(&pool, 5).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["steam::3", "steam::5", "steam::7", "steam::6", "steam::1"],
        "last_played desc, capped at 5"
    );
    assert_eq!(rows[0].display_name(), "Game 3");
    assert_ne!(rows[0].display_name(), rows[0].id.as_str());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn recent_games_skips_unplayed_and_hidden() {
    let dir = std::env::temp_dir().join(format!("tuxgt-recents-hide-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "steam::1",
            manager: "steam",
            store: "",
            game_id: "1",
            name: Some("Played Visible"),
            last_played: Some(10),
            ..Default::default()
        },
    )
    .await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::2",
            manager: "steam",
            store: "",
            game_id: "2",
            name: Some("Never Played"),
            ..Default::default()
        },
    )
    .await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::3",
            manager: "steam",
            store: "",
            game_id: "3",
            name: Some("Detected Hidden"),
            last_played: Some(30),
            detected_hidden: Some(1),
            ..Default::default()
        },
    )
    .await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::4",
            manager: "steam",
            store: "",
            game_id: "4",
            name: Some("Override Hidden"),
            last_played: Some(40),
            override_hidden: Some(1),
            ..Default::default()
        },
    )
    .await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::5",
            manager: "steam",
            store: "",
            game_id: "5",
            name: Some("Override Visible"),
            last_played: Some(50),
            detected_hidden: Some(1),
            override_hidden: Some(0),
            ..Default::default()
        },
    )
    .await;
    let rows = recent_games(&pool, 5).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["steam::5", "steam::1"],
        "override-visible wins like the Library; the rest stay out"
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn recent_games_empty_when_nothing_played() {
    let dir = std::env::temp_dir().join(format!("tuxgt-recents-empty-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "steam::1",
            manager: "steam",
            store: "",
            game_id: "1",
            name: Some("Fresh"),
            ..Default::default()
        },
    )
    .await;
    let rows = recent_games(&pool, 5).await.unwrap();
    assert!(rows.is_empty());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn touch_last_played_surfaces_in_recents() {
    let dir = std::env::temp_dir().join(format!("tuxgt-recents-touch-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "steam::1",
            manager: "steam",
            store: "",
            game_id: "1",
            name: Some("Played Now"),
            ..Default::default()
        },
    )
    .await;
    assert!(recent_games(&pool, 5).await.unwrap().is_empty());
    touch_last_played(&pool, "steam::1").await.unwrap();
    let rows = recent_games(&pool, 5).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "steam::1");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
