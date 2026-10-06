use std::path::Path;

use super::testing::*;
use super::*;
use crate::update::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
use crate::Error;

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
}

fn prev_manifest(paths: &[&str]) -> PrefixManifest {
    PrefixManifest {
        tag: "v0.0.1".into(),
        files: paths
            .iter()
            .map(|p| PrefixFile {
                path: (*p).into(),
                sha256: "prev".into(),
            })
            .collect(),
    }
}

#[test]
#[cfg(unix)]
fn reinstall_drops_upstream_removed_prefix_files() {
    let base = inv_base("reinstall-stale-drop");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    // Old live prefix: current program files plus upstream-removed ones.
    let dest = base.join("dest");
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "bin/old-tool",
        "lib/libtuxgt-launcher.so",
        "lib/retired.so",
        "share/protonfixes/tuxgt_apply.py",
        "share/templates/old.toml",
        "share/applications/tuxgt.desktop",
        "mods/official/kept.toml",
        "mods/official/gone.toml",
    ] {
        put(&dest, rel, b"OLD");
    }
    put(&dest, "mods/official/kept/payload.bin", b"kept-payload");
    put(&dest, "mods/official/gone/payload.bin", b"gone-payload");
    for (rel, bytes) in [
        ("games/g0/sentinel.txt", b"user-game" as &[u8]),
        ("config/tuxgt.sqlite", b"user-db"),
        ("mods/user/mine.toml", b"user-recipe"),
    ] {
        put(&dest, rel, bytes);
    }
    save_prefix_manifest(
        &dest,
        &prev_manifest(&[
            "bin/tuxgt",
            "bin/tuxgt-launcher",
            "bin/old-tool",
            "lib/libtuxgt-launcher.so",
            "lib/retired.so",
            "share/protonfixes/tuxgt_apply.py",
            "share/templates/old.toml",
            "share/applications/tuxgt.desktop",
            "mods/official/kept.toml",
            "mods/official/gone.toml",
        ]),
    )
    .unwrap();
    // Fresh unpacked tree: the removed files are gone upstream.
    let src = base.join("src");
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "lib/libtuxgt-launcher.so",
        "share/protonfixes/tuxgt_apply.py",
        "share/templates/add.toml",
        "share/applications/tuxgt.desktop",
        "mods/official/kept.toml",
        "mods/official/fresh.toml",
    ] {
        put(&src, rel, b"NEW");
    }

    let rep = install_userland_with_home(&src, &dest, &home).unwrap();
    // Upstream-removed files are gone from the live prefix ...
    for rel in ["bin/old-tool", "lib/retired.so", "share/templates/old.toml"] {
        assert!(!dest.join(rel).exists(), "dropped upstream: {rel}");
    }
    // ... and reported (the gone official was already dropped by the overlay
    // pass, so it is silently skipped here, never double-reported).
    for rel in ["bin/old-tool", "lib/retired.so", "share/templates/old.toml"] {
        assert!(
            rep.prefix_removed.contains(&rel.to_string()),
            "reported: {rel}"
        );
    }
    assert!(
        !rep.prefix_removed.iter().any(|r| r.contains("gone")),
        "{:?}",
        rep.prefix_removed
    );
    // ... and from the saved manifest, which tracks landed bytes.
    let manifest = load_prefix_manifest(&dest)
        .unwrap()
        .expect("manifest saved");
    assert_eq!(manifest.tag, format!("v{}", crate::app_version()));
    for rel in ["bin/old-tool", "lib/retired.so", "share/templates/old.toml"] {
        assert!(
            !manifest.files.iter().any(|f| f.path == rel),
            "untracked: {rel}"
        );
    }
    for f in &manifest.files {
        let disk = crate::sha256_file(&dest.join(&f.path)).unwrap();
        assert_eq!(disk, f.sha256, "{}", f.path);
    }
    // Kept set intact: overlaid files, kept payload, user data.
    assert_eq!(std::fs::read(dest.join("bin/tuxgt")).unwrap(), b"NEW");
    assert_eq!(
        std::fs::read(dest.join("mods/official/kept/payload.bin")).unwrap(),
        b"kept-payload"
    );
    assert_eq!(
        std::fs::read(dest.join("games/g0/sentinel.txt")).unwrap(),
        b"user-game"
    );
    assert_eq!(
        std::fs::read(dest.join("config/tuxgt.sqlite")).unwrap(),
        b"user-db"
    );
    assert_eq!(
        std::fs::read(dest.join("mods/user/mine.toml")).unwrap(),
        b"user-recipe"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn fresh_install_seeds_prefix_manifest() {
    let (base, prefix, home) = e66_setup("seed-manifest");
    let first = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(
        first.prefix_removed.is_empty(),
        "{:?}",
        first.prefix_removed
    );
    let manifest = load_prefix_manifest(&prefix)
        .unwrap()
        .expect("manifest seeded");
    assert_eq!(manifest.tag, format!("v{}", crate::app_version()));
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "share/applications/tuxgt.desktop",
    ] {
        assert!(
            manifest.files.iter().any(|f| f.path == rel),
            "tracked: {rel}"
        );
    }
    assert!(
        manifest
            .files
            .iter()
            .any(|f| f.path.starts_with("share/icons/")),
        "icons tracked"
    );
    // Same-tree re-run after a tracked file vanished: no error, the row drops.
    std::fs::remove_file(prefix.join("bin/tuxgt-launcher")).unwrap();
    let second = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(
        second.prefix_removed.is_empty(),
        "{:?}",
        second.prefix_removed
    );
    let healed = load_prefix_manifest(&prefix)
        .unwrap()
        .expect("manifest resaved");
    assert!(!healed.files.iter().any(|f| f.path == "bin/tuxgt-launcher"));
    assert!(healed.files.iter().any(|f| f.path == "bin/tuxgt"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn reinstall_without_previous_manifest_skips_prefix_stale_pass() {
    let base = inv_base("reinstall-legacy-skip");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let dest = base.join("dest");
    for rel in [
        "bin/tuxgt",
        "bin/legacy-tool",
        "mods/official/kept.toml",
        "mods/official/gone.toml",
    ] {
        put(&dest, rel, b"OLD");
    }
    put(&dest, "mods/official/gone/payload.bin", b"gone-payload");
    assert!(load_prefix_manifest(&dest).unwrap().is_none());
    let src = base.join("src");
    for rel in ["bin/tuxgt", "mods/official/kept.toml"] {
        put(&src, rel, b"NEW");
    }

    let rep = install_userland_with_home(&src, &dest, &home).unwrap();
    assert!(rep.prefix_removed.is_empty(), "{:?}", rep.prefix_removed);
    // No previous manifest: the unknown stray survives (legacy skip).
    assert!(
        dest.join("bin/legacy-tool").is_file(),
        "legacy stray survives"
    );
    // ... but the manifest seeds for next time, without adopting the stray.
    let manifest = load_prefix_manifest(&dest)
        .unwrap()
        .expect("manifest seeded");
    assert!(!manifest.files.iter().any(|f| f.path == "bin/legacy-tool"));
    assert!(manifest.files.iter().any(|f| f.path == "bin/tuxgt"));
    // The old drop-gone-officials pass still runs.
    assert!(!dest.join("mods/official/gone.toml").exists());
    assert!(!dest.join("mods/official/gone").exists());
    assert_eq!(std::fs::read(dest.join("bin/tuxgt")).unwrap(), b"NEW");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn partial_tree_without_bin_skips_prefix_pass() {
    let base = inv_base("reinstall-partial-skip");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let dest = base.join("dest");
    for rel in ["bin/tuxgt", "lib/libtuxgt-launcher.so"] {
        put(&dest, rel, b"OLD");
    }
    let before = prev_manifest(&["bin/tuxgt", "lib/libtuxgt-launcher.so"]);
    save_prefix_manifest(&dest, &before).unwrap();
    // Partial source tree: one template, no bin/ — intent unknown.
    let src = base.join("src");
    put(&src, "share/templates/extra.toml", b"NEW");

    let rep = install_userland_with_home(&src, &dest, &home).unwrap();
    assert!(rep.prefix_removed.is_empty(), "{:?}", rep.prefix_removed);
    // The overlay still lands what the tree has ...
    assert_eq!(
        std::fs::read(dest.join("share/templates/extra.toml")).unwrap(),
        b"NEW"
    );
    // ... but nothing is deleted and the manifest is untouched.
    assert!(dest.join("lib/libtuxgt-launcher.so").is_file());
    assert_eq!(
        load_prefix_manifest(&dest).unwrap().expect("manifest kept"),
        before
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn corrupt_prefix_manifest_fails_as_install() {
    let (base, prefix, home) = e66_setup("corrupt-manifest");
    put(&prefix, "config/prefix-manifest.toml", b"not = [toml");
    let err = install_userland_with_home(&prefix, &prefix, &home).unwrap_err();
    assert!(matches!(err, Error::Install(_)), "{err:?}");
    assert!(format!("{err}").contains("prefix manifest parse"), "{err}");
    let _ = std::fs::remove_dir_all(&base);
}
