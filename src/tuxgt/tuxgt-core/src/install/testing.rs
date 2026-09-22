use crate::FileManifest;
use std::fs;
use std::path::PathBuf;

pub(crate) fn test_manifest(game: &str, instance: &str, dest: &str, sha: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: "reshade".into(),
        adapter: "install".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: vec![crate::download::PlannedFile {
            source: "cache/k/f#x".into(),
            dest: dest.into(),
            sha256: sha.into(),
            enabled: true,
        }]
        .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}

pub(crate) fn setup(game: &str) -> (PathBuf, PathBuf, PathBuf) {
    let data = std::env::temp_dir().join(format!(
        "tuxgt-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&data);
    let stage = crate::stage::stage_dir(&data, game, "reshade");
    fs::create_dir_all(&stage).unwrap();
    let root = data.join("gamedir");
    fs::create_dir_all(&root).unwrap();
    (data, stage, root)
}
