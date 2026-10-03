//! Apply tests: fake prefixes updated from release-shaped tarballs served
//! on localhost. The package `bin/tuxgt` is a shell script, so the host
//! re-exec proves itself through a sentinel file.

use std::path::{Path, PathBuf};

use super::apply::apply_app_update;
use super::manifest::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
use crate::download::testing::serve_http;

pub(super) fn tmp_root(case: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("tuxgt-update-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

pub(super) fn write_tree(root: &Path, files: &[(&str, &str)]) {
    for (rel, body) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body.as_bytes()).unwrap();
    }
}

/// Pack `tree/` (holding the `tuxgt/` top dir) into release-shaped bytes.
/// `tar` must exist (EXT_TOOLS).
pub(super) fn pack_tarball(tree: &Path) -> Vec<u8> {
    let out = tree.with_file_name("tuxgt.tar.gz");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(tree)
        .arg("tuxgt")
        .status()
        .expect("tar runs");
    assert!(status.success());
    std::fs::read(&out).unwrap()
}

/// Serve tarball bytes once; returns the asset URL.
pub(super) fn serve_once(bytes: Vec<u8>) -> String {
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let len = leaked.len();
    let port = serve_http(leaked, len, len, 1);
    format!("http://127.0.0.1:{port}/tuxgt.tar.gz")
}

pub(super) fn tree_join(root: &Path) -> PathBuf {
    root.join("tree").join("tuxgt")
}

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// No fetch entry or scratch dir left: `downloads.log` (the fetch audit
/// log) is the only entry that may remain.
pub(super) fn downloads_clean(prefix: &Path) -> bool {
    match std::fs::read_dir(prefix.join("downloads")) {
        Err(_) => true,
        Ok(rd) => rd
            .flatten()
            .all(|e| e.path().is_file() && e.file_name().to_string_lossy() == "downloads.log"),
    }
}

#[tokio::test]
async fn apply_overlays_tracks_and_refreshes_host() {
    let root = tmp_root("apply");
    let prefix = root.join("prefix");
    let sentinel = root.join("host-refresh.log");
    write_tree(
        &prefix,
        &[
            ("bin/tuxgt", "old-bin"),
            ("bin/tuxgt-launcher", "old-launcher"),
            ("bin/old-tool", "stale"),
            ("lib/libtuxgt-launcher.so", "old-so"),
            ("share/protonfixes/tuxgt_apply.py", "old-apply"),
            ("share/applications/tuxgt.desktop", "Exec=/old\n"),
            ("mods/official/reshade.toml", "old-recipe"),
            ("mods/official/reshade/payload.bin", "kept-payload"),
            ("mods/official/gone.toml", "old"),
            ("mods/official/gone/data.bin", "drop-me"),
            ("mods/user/mine.toml", "user"),
            ("mods/user/mine/data.bin", "user-payload"),
            ("games/g1/file", "game"),
            ("config/ui.toml", "ui"),
            ("config/evil.toml", "not-ours"),
        ],
    );
    let prev = |paths: &[&str]| PrefixManifest {
        tag: "v0.0.1".into(),
        files: paths
            .iter()
            .map(|p| PrefixFile {
                path: (*p).into(),
                sha256: "prev".into(),
            })
            .collect(),
    };
    save_prefix_manifest(
        &prefix,
        &prev(&[
            "bin/tuxgt",
            "bin/tuxgt-launcher",
            "bin/old-tool",
            "lib/libtuxgt-launcher.so",
            "share/protonfixes/tuxgt_apply.py",
            "share/applications/tuxgt.desktop",
            "mods/official/reshade.toml",
            "mods/official/gone.toml",
            "mods/official/stale.toml",
            "config/evil.toml",
            "games/g1/file",
        ]),
    )
    .unwrap();

    let tree = root.join("tree");
    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", sentinel.display());
    write_tree(
        &tree.join("tuxgt"),
        &[
            ("bin/tuxgt", &script),
            ("bin/tuxgt-launcher", "new-launcher"),
            ("lib/libtuxgt-launcher.so", "new-so"),
            ("lib/libtuxgt-launcher32.so", "new-so32"),
            ("share/protonfixes/tuxgt_apply.py", "new-apply"),
            ("share/applications/tuxgt.desktop", "generic\n"),
            ("mods/official/reshade.toml", "new-recipe"),
            ("mods/official/fresh.toml", "brand-new"),
        ],
    );
    let url = serve_once(pack_tarball(&tree));
    let progress_calls = std::sync::atomic::AtomicU64::new(0);
    let sink = |_: crate::download::FetchProgress| {
        progress_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    };
    let rep = apply_app_update(&prefix, "v999.0.0", &url, Some(&sink), None)
        .await
        .expect("apply");
    assert_eq!(rep.tag, "v999.0.0");
    // 8 package files; the present desktop entry is kept, not overlaid.
    assert_eq!(rep.updated, 7);
    assert!(rep.removed.contains(&"bin/old-tool".to_string()));
    assert!(rep.removed.contains(&"mods/official/gone.toml".to_string()));
    assert!(rep.removed.contains(&"mods/official/gone/".to_string()));
    assert!(!rep.removed.iter().any(|r| r.contains("evil")));
    assert!(!rep.removed.iter().any(|r| r.contains("stale")));
    assert!(progress_calls.load(std::sync::atomic::Ordering::SeqCst) > 0);

    // Overlaid content + deploy-parity modes.
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        script
    );
    assert_eq!(
        std::fs::read_to_string(prefix.join("lib/libtuxgt-launcher32.so")).unwrap(),
        "new-so32"
    );
    assert_eq!(mode_of(&prefix.join("bin/tuxgt")), 0o755);
    assert_eq!(mode_of(&prefix.join("lib/libtuxgt-launcher.so")), 0o755);
    assert_eq!(mode_of(&prefix.join("mods/official/fresh.toml")), 0o644);
    // Deploy parity: the rewritten desktop entry survives.
    assert_eq!(
        std::fs::read_to_string(prefix.join("share/applications/tuxgt.desktop")).unwrap(),
        "Exec=/old\n"
    );
    // Stale + gone-official payload dropped; user/game/config data kept.
    assert!(!prefix.join("bin/old-tool").exists());
    assert!(!prefix.join("mods/official/gone.toml").exists());
    assert!(!prefix.join("mods/official/gone").exists());
    assert!(prefix.join("mods/official/reshade/payload.bin").is_file());
    assert!(prefix.join("mods/user/mine/data.bin").is_file());
    assert!(prefix.join("games/g1/file").is_file());
    assert!(prefix.join("config/evil.toml").is_file());
    assert!(prefix.join("config/ui.toml").is_file());
    // Manifest tracks reality, including the kept desktop entry.
    let manifest = load_prefix_manifest(&prefix).unwrap().expect("manifest");
    assert_eq!(manifest.tag, "v999.0.0");
    assert_eq!(manifest.files.len(), 8);
    let desktop = manifest
        .files
        .iter()
        .find(|f| f.path == "share/applications/tuxgt.desktop")
        .expect("desktop tracked");
    assert_eq!(
        desktop.sha256,
        crate::download::sha256_file(&prefix.join("share/applications/tuxgt.desktop")).unwrap()
    );
    for f in &manifest.files {
        let disk = crate::download::sha256_file(&prefix.join(&f.path)).unwrap();
        assert_eq!(disk, f.sha256, "{}", f.path);
    }
    // Host refresh ran as the new binary, pinned to the live prefix.
    assert_eq!(
        std::fs::read_to_string(&sentinel).unwrap().trim(),
        format!("install --prefix {} --yes", prefix.display())
    );
    // downloads/ is empty at rest: spent entry + scratch gone.
    assert!(downloads_clean(&prefix));
    let _ = std::fs::remove_dir_all(&root);
}
