use super::testing::nanos;
use super::*;
use crate::FileManifest;
use std::fs;
use std::path::{Path, PathBuf};

mod matching;
mod ops;
mod parse;
mod replace;

const STREAM: &[&str] = &[
    "sl.interposer.dll",
    "sl.common.dll",
    "sl.pcl.dll",
    "sl.dlss.dll",
    "sl.dlss_g.dll",
    "sl.reflex.dll",
    "nvngx_dlss.dll",
    "nvngx_dlssd.dll",
    "nvngx_dlssg.dll",
];

fn official_text(id: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../mods/official")
            .join(format!("{id}.toml")),
    )
    .unwrap()
}

fn scratch(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-provide-{tag}-{}-{}",
        std::process::id(),
        nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let data = dir.join("data");
    let cfg = dir.join("config");
    fs::create_dir_all(&cfg).unwrap();
    crate::instance::seed_official_share_from_repo(&data);
    (dir, data, cfg)
}

fn payload(data: &Path, id: &str) -> PathBuf {
    data.join("mods").join("official").join(id)
}

fn write_tree(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn reshade_manifest(game: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: "reshade".into(),
        mod_type: "reshade".into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        env: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
    }
}

fn dep_manifest(game: &str, instance: &str, mod_type: &str) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: mod_type.into(),
        ..reshade_manifest(game)
    }
}

trait Ready {
    fn ready(&self) -> bool;
}

impl Ready for ProvideReport {
    fn ready(&self) -> bool {
        self.missing.is_empty() && self.ambiguous.is_empty()
    }
}
