use super::*;
use crate::open_db;
use crate::plugin::{PluginDesc, PluginHost};
use crate::testing::{seed_game, SeedGame};
use crate::{FileManifest, PlannedEnv};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

pub(crate) fn need_mf(adapter: &str, enabled: bool, env: bool) -> FileManifest {
    FileManifest {
        game: "steam::1".into(),
        instance: adapter.to_string(),
        mod_type: "reshade".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: if env {
            vec![PlannedEnv {
                key: "FOO".into(),
                value: "1".into(),
                enabled: true,
            }]
            .into_boxed_slice()
        } else {
            Box::default()
        },
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

pub(crate) fn test_host(dir: &Path) -> PluginHost {
    PluginHost::load_with(&[], dir).unwrap()
}

pub(crate) fn env_host(dir: &Path) -> PluginHost {
    PluginHost::load_with(
        &[PluginDesc {
            registry: None,
            name: "env",
            tag: None,
            label_id: "plugin-env-label",
            requires: None,
        }],
        dir,
    )
    .unwrap()
}

pub(crate) fn wrapper_host(dir: &Path) -> PluginHost {
    PluginHost::load_with(crate::FIRST_PARTY, dir).unwrap()
}

pub(crate) fn stub_paths(dir: &Path) -> LaunchPaths {
    let launcher = dir.join("tuxgt-launcher");
    std::fs::write(&launcher, b"#!/bin/sh\nexec \"$@\"\n").unwrap();
    LaunchPaths {
        launcher: Some(launcher),
        steam: None,
        heroic: None,
        umu: None,
        wine: None,
    }
}

pub(crate) async fn pool_dir() -> (SqlitePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-e06-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = open_db(&dir).await.unwrap();
    (pool, dir)
}

pub(crate) async fn manual_row(
    id: &str,
    exe: &Path,
    launch_options: Option<&str>,
) -> (SqlitePool, PathBuf, LaunchPaths, PluginHost) {
    let (pool, dir) = pool_dir().await;
    let host = wrapper_host(&dir);
    let paths = stub_paths(&dir);
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: id.rsplit(':').next().unwrap(),
            name: Some("t"),
            exe_path: Some(exe.to_str().unwrap()),
            launch_options,
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    (pool, dir, paths, host)
}
