use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static TEST_N: AtomicU64 = AtomicU64::new(0);

pub(crate) fn repo_mods_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/official")
}

/// Seed via the shared fixture helper: every prefix helper below seeds
/// automatically, so catalog tests always read the real shipped content.
pub(crate) fn seed_official_share(data: &Path) {
    seed_official_share_from_repo(data);
}

/// Copy the packaged official TOMLs into a fixture prefix. Test-only.
pub(crate) fn seed_official_share_from_repo(data_dir: &Path) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/official");
    let dest = official_mods_dir(data_dir);
    std::fs::create_dir_all(&dest).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.extension().is_some_and(|x| x == "toml") {
            std::fs::copy(&p, dest.join(e.file_name())).unwrap();
        }
    }
}

pub(crate) fn temp_config() -> PathBuf {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tuxgt-e14-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    seed_official_share(&dir);
    dir
}

pub(crate) fn n_official() -> usize {
    fs::read_dir(repo_mods_dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .ok()
                .is_some_and(|e| e.path().extension().is_some_and(|x| x == "toml"))
        })
        .count()
}

pub(crate) fn sample_recipe(id: &str, plans: &str) -> String {
    format!(
        r#"id = "{id}"
type = "optiscaler"
label = "fork"
plans_allowed = [{plans}]

[source]
type = "github"
owner = "someone"
repo = "OptiScaler-fork"
asset_glob = "OptiScaler_*.zip"
"#
    )
}

pub(crate) fn family_tpl() -> ModTemplate {
    ModTemplate {
        id: "family-x".into(),
        label: "Family X".into(),
        mod_type: "reshade_addon".into(),
        mode: String::new(),
        requires: Box::default(),
        family: Some(TemplateFamily {
            owner: "o".into(),
            repo: "r".into(),
            asset_glob: "x-*.addon64".into(),
            prerelease: true,
            drop: vec!["dxgi.dll".into()].into_boxed_slice(),
        }),
    }
}

pub(crate) fn ureq(id: &str, req: &str) -> String {
    format!(
        "id = \"{id}\"\ntype = \"custom\"\nlabel = \"{id}\"\n{req}[source]\ntype = \"local\"\npath = \"/tmp/x\"\n"
    )
}

pub(crate) fn scratch_pkg() -> (PathBuf, PathBuf) {
    let n = TEST_N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("tuxgt-e49-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&root);
    seed_official_share(&root);
    let pkg = root.join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("OptiScaler.dll"), b"dll").unwrap();
    fs::write(pkg.join("extra.ini"), b"extra").unwrap();
    fs::write(pkg.join("setup.bat"), b"bat").unwrap();
    fs::write(pkg.join("notes.md"), b"md").unwrap();
    (root, pkg)
}
