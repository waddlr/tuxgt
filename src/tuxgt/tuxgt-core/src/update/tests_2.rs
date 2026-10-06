//! Local-file apply (`update --from`): same overlay machinery as the
//! fetched path, minus fetch and version check. The package `bin/tuxgt`
//! is a shell script, so the host re-exec proves itself through a
//! sentinel file. No network is involved: there is no URL to serve.

use super::apply::apply_local_update;
use super::manifest::{load_prefix_manifest, save_prefix_manifest, PrefixFile, PrefixManifest};
use super::tests_0::{downloads_clean, pack_tarball, tmp_root, tree_join, write_tree};

/// Pack `tree/` and return the tarball path `pack_tarball` wrote.
fn pack_local(root: &std::path::Path) -> std::path::PathBuf {
    let asset = root.join("tuxgt.tar.gz");
    pack_tarball(&root.join("tree"));
    asset
}

#[test]
fn local_apply_overlays_tracks_and_refreshes_host() {
    let root = tmp_root("local-apply");
    let prefix = root.join("prefix");
    let sentinel = root.join("host-refresh.log");
    write_tree(
        &prefix,
        &[
            ("bin/tuxgt", "old-bin"),
            ("mods/official/reshade.toml", "old-recipe"),
            ("mods/official/reshade/payload.bin", "kept-payload"),
            ("mods/official/gone.toml", "old"),
            ("mods/official/gone/data.bin", "drop-me"),
            ("mods/user/mine.toml", "user"),
        ],
    );
    save_prefix_manifest(
        &prefix,
        &PrefixManifest {
            tag: "v0.0.1".into(),
            files: [
                "bin/tuxgt",
                "mods/official/reshade.toml",
                "mods/official/gone.toml",
            ]
            .iter()
            .map(|p| PrefixFile {
                path: (*p).into(),
                sha256: "prev".into(),
            })
            .collect(),
        },
    )
    .unwrap();

    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", sentinel.display());
    write_tree(
        &tree_join(&root),
        &[
            ("bin/tuxgt", &script),
            ("mods/official/reshade.toml", "new-recipe"),
        ],
    );
    let asset = pack_local(&root);
    let rep = apply_local_update(&prefix, "v9.9.9", &asset).expect("apply");
    assert_eq!(rep.tag, "v9.9.9");
    assert_eq!(rep.updated, 2);
    assert!(rep.removed.contains(&"mods/official/gone.toml".to_string()));
    assert!(rep.removed.contains(&"mods/official/gone/".to_string()));
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        script
    );
    assert!(!prefix.join("mods/official/gone").exists());
    assert!(prefix.join("mods/official/reshade/payload.bin").is_file());
    assert!(prefix.join("mods/user/mine.toml").is_file());
    assert_eq!(
        load_prefix_manifest(&prefix)
            .unwrap()
            .expect("manifest")
            .tag,
        "v9.9.9"
    );
    assert_eq!(
        std::fs::read_to_string(&sentinel).unwrap().trim(),
        format!("install --prefix {} --yes", prefix.display())
    );
    assert!(downloads_clean(&prefix));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_apply_on_legacy_prefix_drops_gone_officials_only() {
    let root = tmp_root("local-legacy");
    let prefix = root.join("prefix");
    let sentinel = root.join("host-refresh.log");
    write_tree(
        &prefix,
        &[
            ("bin/tuxgt", "old-bin"),
            ("bin/old-tool", "stale"),
            ("mods/official/reshade.toml", "old-recipe"),
            ("mods/official/reshade/payload.bin", "kept-payload"),
            ("mods/official/gone.toml", "old"),
            ("mods/official/gone/data.bin", "drop-me"),
        ],
    );
    // No manifest: the legacy fallback compares the official tree only.

    let script = format!("#!/bin/sh\necho \"$@\" >> {}\nexit 0\n", sentinel.display());
    write_tree(
        &tree_join(&root),
        &[
            ("bin/tuxgt", &script),
            ("mods/official/reshade.toml", "new-recipe"),
        ],
    );
    let asset = pack_local(&root);
    let rep = apply_local_update(&prefix, "v9.9.9", &asset).expect("apply");
    assert!(rep.removed.contains(&"mods/official/gone.toml".to_string()));
    assert!(rep.removed.contains(&"mods/official/gone/".to_string()));
    assert!(!prefix.join("mods/official/gone.toml").exists());
    assert!(!prefix.join("mods/official/gone").exists());
    // Non-official stale files are kept without a manifest to prove them.
    assert!(prefix.join("bin/old-tool").is_file());
    assert!(prefix.join("mods/official/reshade/payload.bin").is_file());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_apply_refuses_a_bare_dir_first() {
    let root = tmp_root("local-bare");
    let prefix = root.join("prefix");
    std::fs::create_dir_all(&prefix).unwrap();
    // Bad tag and missing tarball too: the prefix refusal comes first.
    let err = apply_local_update(&prefix, "not-a-version", &root.join("nope.tar.gz")).unwrap_err();
    assert!(err.to_string().contains("not an installed prefix"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_apply_refuses_a_bad_tag() {
    let root = tmp_root("local-badtag");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let err = apply_local_update(&prefix, "not-a-version", &root.join("nope.tar.gz")).unwrap_err();
    assert!(
        err.to_string().contains("cannot parse release tag"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_apply_refuses_a_missing_tarball() {
    let root = tmp_root("local-notar");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    let err = apply_local_update(&prefix, "v9.9.9", &root.join("nope.tar.gz")).unwrap_err();
    assert!(err.to_string().contains("no such tarball"), "{err}");
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "old"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn local_apply_refuses_a_tree_without_bin_tuxgt() {
    let root = tmp_root("local-nobin");
    let prefix = root.join("prefix");
    write_tree(&prefix, &[("bin/tuxgt", "old")]);
    write_tree(&tree_join(&root), &[("lib/only.so", "x")]);
    let asset = pack_local(&root);
    let err = apply_local_update(&prefix, "v9.9.9", &asset).unwrap_err();
    assert!(err.to_string().contains("no bin/tuxgt"), "{err}");
    assert_eq!(
        std::fs::read_to_string(prefix.join("bin/tuxgt")).unwrap(),
        "old"
    );
    assert!(downloads_clean(&prefix));
    let _ = std::fs::remove_dir_all(&root);
}
