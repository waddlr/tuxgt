use sqlx::SqlitePool;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn data() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-prewire-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

pub(crate) async fn game_pool(dir: &Path) -> SqlitePool {
    crate::open_db(dir).await.unwrap()
}

pub(crate) fn preload_manifest(
    game: &str,
    instance: &str,
    adapter: &str,
    enabled: bool,
) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: vec![
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "ReShade64.dll".into(),
                sha256: "a".into(),
                enabled: true,
            },
            crate::download::PlannedFile {
                source: "s".into(),
                dest: "notes.txt".into(),
                sha256: "b".into(),
                enabled: true,
            },
        ]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}
