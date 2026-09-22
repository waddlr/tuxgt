use super::*;
use crate::game::GameId;
use crate::plugin::{PluginHost, FIRST_PARTY};
use crate::testing::{seed_game, SeedGame};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static SETUP_SEQ: AtomicU64 = AtomicU64::new(0);

pub(crate) async fn setup() -> (SqlitePool, PathBuf, PluginHost, String) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-session-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SETUP_SEQ.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    crate::instance::seed_official_share_from_repo(&dir);
    let pool = crate::open_db(&dir).await.unwrap();
    let host = PluginHost::load_with(FIRST_PARTY, dir.join("config")).unwrap();
    seed_game(
        &pool,
        SeedGame {
            id: "steam::814380",
            manager: "steam",
            game_id: "814380",
            name: Some("sekiro"),
            exe_path: Some("/games/sekiro/sekiro.exe"),
            prefix_path: Some("/pfx/sekiro"),
            ..Default::default()
        },
    )
    .await;
    seed_game(
        &pool,
        SeedGame {
            id: "heroic:gog:abc",
            manager: "heroic",
            store: "gog",
            game_id: "abc",
            name: Some("flag"),
            exe_path: Some("/games/flag/flag.exe"),
            prefix_path: Some("/pfx/flag"),
            ..Default::default()
        },
    )
    .await;
    (pool, dir, host, "steam::814380".into())
}

pub(crate) fn steam_conf(dir: &Path) -> PathBuf {
    protonfixes_conf(dir, &GameId::parse("steam::814380").unwrap())
}

pub(crate) fn opti_manifest(game: &str, instance: &str, mod_type: &str) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: mod_type.into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}
