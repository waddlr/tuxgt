use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::testing::{seed_game, SeedGame};
use crate::{FileManifest, PlannedFile};

pub(super) struct Fix {
    pub pool: SqlitePool,
    pub dir: PathBuf,
    pub id: String,
    pub game_dir: PathBuf,
    pub install: PathBuf,
}

pub(super) async fn fix(slug: &str) -> Fix {
    open(
        slug,
        "manual",
        "standalone",
        &format!("manual:standalone:{slug}"),
    )
    .await
}

/// Steam id with an empty store, so an empty Apply record restores without
/// touching a real `localconfig.vdf`. `StoreClient::for_game` is Steam.
pub(super) async fn fix_steam(slug: &str) -> Fix {
    open(slug, "steam", "", &format!("steam::{slug}")).await
}

async fn open(slug: &str, manager: &str, store: &str, id: &str) -> Fix {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-profile-{slug}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let install = dir.join("install");
    std::fs::create_dir_all(&install).unwrap();
    let exe = install.join("game.exe");
    std::fs::write(&exe, b"MZ").unwrap();
    let pool = crate::open_db(&dir).await.unwrap();
    seed_game(
        &pool,
        SeedGame {
            id,
            manager,
            store,
            game_id: slug,
            name: Some("Profile"),
            install_dir: Some(install.to_str().unwrap()),
            exe_path: Some(exe.to_str().unwrap()),
            ..Default::default()
        },
    )
    .await;
    let game_dir = crate::game_dir(&dir, &crate::GameId::parse(id).unwrap());
    Fix {
        pool,
        dir,
        id: id.to_string(),
        game_dir,
        install,
    }
}

pub(super) fn manifest(
    game: &str,
    instance: &str,
    adapter: &str,
    enabled: bool,
    files: &[(&str, &str)],
) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "custom".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        files: files
            .iter()
            .map(|(dest, sha)| PlannedFile {
                source: (*dest).into(),
                dest: (*dest).into(),
                sha256: (*sha).into(),
                enabled: true,
                load: None,
            })
            .collect(),
        env: Box::new([]),
        backups: Default::default(),
        generated_globs: Box::new([]),
        include: Box::new([]),
        harvested: Default::default(),
        provenance: Default::default(),
    }
}

pub(super) fn write_live_manifest(fx: &Fix, instance: &str, adapter: &str, enabled: bool) {
    crate::write_manifest(&fx.dir, &manifest(&fx.id, instance, adapter, enabled, &[])).unwrap();
}

/// Staged bytes plus the same bytes at the game-dir dest, claimed by an
/// enabled install manifest.
pub(super) fn plant_install(fx: &Fix, instance: &str, dest: &str, bytes: &[u8]) {
    let sha = crate::sha256_hex(bytes);
    let stage = crate::stage_dir(&fx.dir, &fx.id, instance).join(dest);
    std::fs::create_dir_all(stage.parent().unwrap()).unwrap();
    std::fs::write(&stage, bytes).unwrap();
    std::fs::write(fx.install.join(dest), bytes).unwrap();
    crate::write_manifest(
        &fx.dir,
        &manifest(
            &fx.id,
            instance,
            crate::ADAPTER_INSTALL,
            true,
            &[(dest, sha.as_str())],
        ),
    )
    .unwrap();
}

pub(super) fn plant_slot_install(fx: &Fix, id: &str, instance: &str, dest: &str, bytes: &[u8]) {
    let sha = crate::sha256_hex(bytes);
    let slot = fx.game_dir.join("profiles").join(id);
    let stage = slot.join("stage").join(instance).join(dest);
    std::fs::create_dir_all(stage.parent().unwrap()).unwrap();
    std::fs::write(&stage, bytes).unwrap();
    let manifest_dir = slot.join("manifests");
    std::fs::create_dir_all(&manifest_dir).unwrap();
    let body = toml::to_string(&manifest(
        &fx.id,
        instance,
        crate::ADAPTER_INSTALL,
        true,
        &[(dest, sha.as_str())],
    ))
    .unwrap();
    std::fs::write(manifest_dir.join(format!("{instance}.toml")), body).unwrap();
}

impl Fix {
    pub(super) async fn drop(self) {
        let _ = tokio::fs::remove_dir_all(&self.dir).await;
    }
}
