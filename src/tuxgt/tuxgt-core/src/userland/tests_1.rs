use super::testing::*;
use super::*;
use std::path::{Path, PathBuf};

#[test]
fn inventory_save_load_round_trip() {
    let base = inv_base("inv-toml");
    let prefix = base.join("pfx");
    let home = base.join("home");
    // Missing file → unknown extras, not an error.
    assert!(load_host_inventory(&prefix).unwrap().is_none());
    let intended = intended_host_manifest(&prefix, &home);
    let wrapped = vec![home.join(".config/protonfixes/localfixes")];
    let inv = host_inventory_from_intended(&prefix, &intended, &wrapped);
    save_host_inventory(&prefix, &inv).unwrap();
    assert_eq!(
        host_inventory_path(&prefix),
        prefix.join("config/host-install.toml")
    );
    let back = load_host_inventory(&prefix).unwrap().unwrap();
    assert_eq!(back, inv);
    assert_eq!(back.prefix, prefix);
    assert_eq!(back.paths.len(), intended.len());
    assert_eq!(&back.wrapped[..], &wrapped[..]);
    assert!(
        !back.paths.iter().any(|p| p.path.ends_with("__init__.py")),
        "never track localfixes/__init__.py"
    );
    let _ = std::fs::remove_dir_all(&base);
}
// E66 reconcile tests: explicit-home install, never the real `$HOME`
// and never the process `HOME` env.
fn e66_setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = inv_base(tag);
    let prefix = base.join("pfx");
    std::fs::create_dir_all(prefix.join("bin")).unwrap();
    std::fs::write(prefix.join("bin/tuxgt"), b"x").unwrap();
    std::fs::write(prefix.join("bin/tuxgt-launcher"), b"y").unwrap();
    for size in ICON_SIZES {
        let p = icon_path(&prefix, size);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, format!("png-{size}")).unwrap();
    }
    let prefix = std::fs::canonicalize(&prefix).unwrap();
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    (base, prefix, home)
}

fn e66_prev_with_extras(
    prefix: &Path,
    home: &Path,
    extras: Vec<HostInventoryEntry>,
) -> Vec<IntendedHostEntry> {
    let intended = intended_host_manifest(prefix, home);
    let mut inv = host_inventory_from_intended(prefix, &intended, &[]);
    inv.paths.extend(extras);
    save_host_inventory(prefix, &inv).unwrap();
    intended
}

#[test]
#[cfg(unix)]
fn reconcile_removes_unmodified_stale_owned_paths() {
    let (base, prefix, home) = e66_setup("e66-stale-rm");
    // Stale file whose bytes still match the old inventory hash.
    let legacy = home.join(".local/bin/tuxgt-legacy");
    let legacy_bytes = b"# legacy shim\n";
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, legacy_bytes).unwrap();
    // Stale symlink still pointing at our old target.
    let old_link = home.join(".local/bin/tuxgt-old-link");
    let old_target = prefix.join("bin/tuxgt-old");
    make_link(&old_link, &old_target);
    // Stale ours-marker hook file whose bytes drifted from the
    // recorded hash: still ours, still removed.
    let legacy_hook = home.join(".config/protonfixes/localfixes/legacy_hook.py");
    std::fs::create_dir_all(legacy_hook.parent().unwrap()).unwrap();
    std::fs::write(&legacy_hook, format!("{HOOK_MARKER}\n# drifted\n")).unwrap();
    let intended = e66_prev_with_extras(
        &prefix,
        &home,
        vec![
            HostInventoryEntry {
                path: legacy.clone(),
                kind: HostKind::File,
                target: None,
                sha256: Some(sha256_hex_bytes(legacy_bytes)),
            },
            HostInventoryEntry {
                path: old_link.clone(),
                kind: HostKind::Symlink,
                target: Some(old_target),
                sha256: None,
            },
            HostInventoryEntry {
                path: legacy_hook.clone(),
                kind: HostKind::File,
                target: None,
                sha256: Some(sha256_hex_bytes(b"older bytes")),
            },
        ],
    );
    let rep = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(!legacy.exists(), "unmodified stale file removed");
    assert!(
        std::fs::symlink_metadata(&old_link).is_err(),
        "stale owned symlink removed"
    );
    assert!(!legacy_hook.exists(), "drifted ours-marker file removed");
    for p in [&legacy, &old_link, &legacy_hook] {
        assert!(rep.stale_removed.contains(p), "removed: {}", p.display());
    }
    assert!(rep.stale_skipped.is_empty(), "{:?}", rep.stale_skipped);
    // New inventory matches the intended set exactly.
    let back = load_host_inventory(&prefix).unwrap().unwrap();
    assert_eq!(back, host_inventory_from_intended(&prefix, &intended, &[]));
    // Intended set itself was written.
    let conf = std::fs::read_to_string(home.join(".config/tuxgt.conf")).unwrap();
    assert!(conf.contains(&format!("TUXGT_DATA={}", rep.prefix.display())));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn reconcile_leaves_hand_edited_and_prefix_paths() {
    let (base, prefix, home) = e66_setup("e66-stale-skip");
    // Stale file the user replaced: recorded hash no longer matches,
    // no ours marker → left in place.
    let legacy = home.join(".local/bin/tuxgt-legacy");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, "# user replacement\n").unwrap();
    // Stale path under PREFIX: never deleted, even when the hash
    // matches (the tree, `games/`, `config/` are not ours to prune).
    let in_tree = prefix.join("config/legacy.conf");
    let tree_bytes = b"old tree file\n";
    std::fs::create_dir_all(in_tree.parent().unwrap()).unwrap();
    std::fs::write(&in_tree, tree_bytes).unwrap();
    e66_prev_with_extras(
        &prefix,
        &home,
        vec![
            HostInventoryEntry {
                path: legacy.clone(),
                kind: HostKind::File,
                target: None,
                sha256: Some(sha256_hex_bytes(b"original bytes")),
            },
            HostInventoryEntry {
                path: in_tree.clone(),
                kind: HostKind::File,
                target: None,
                sha256: Some(sha256_hex_bytes(tree_bytes)),
            },
        ],
    );
    let rep = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert_eq!(
        std::fs::read_to_string(&legacy).unwrap(),
        "# user replacement\n"
    );
    assert!(in_tree.is_file(), "PREFIX tree never pruned");
    assert!(rep.stale_removed.is_empty(), "{:?}", rep.stale_removed);
    assert!(rep.stale_skipped.contains(&legacy));
    assert!(rep.stale_skipped.contains(&in_tree));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn reinstall_same_set_is_noop_delete() {
    let (base, prefix, home) = e66_setup("e66-noop");
    // Legacy first install: no previous inventory, nothing stale.
    let first = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(first.stale_removed.is_empty());
    assert!(first.stale_skipped.is_empty());
    let intended = intended_host_manifest(&first.prefix, &home);
    let expected = host_inventory_from_intended(&first.prefix, &intended, &[]);
    assert_eq!(
        load_host_inventory(&first.prefix).unwrap().unwrap(),
        expected
    );
    // Second install with the same set: clean overwrite, no deletes.
    let second = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(
        second.stale_removed.is_empty(),
        "{:?}",
        second.stale_removed
    );
    assert!(
        second.stale_skipped.is_empty(),
        "{:?}",
        second.stale_skipped
    );
    assert_eq!(
        load_host_inventory(&first.prefix).unwrap().unwrap(),
        expected
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn reinstall_wraps_foreign_default_once() {
    let (base, prefix, home) = e66_setup("e66-wrap");
    let lf = home.join(".config/protonfixes/localfixes");
    std::fs::create_dir_all(&lf).unwrap();
    std::fs::write(lf.join("default.py"), "def main():\n    pass\n").unwrap();
    let rep = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(std::fs::read_to_string(lf.join(WRAPPED_NAME))
        .unwrap()
        .contains("def main"));
    let d = std::fs::read_to_string(lf.join("default.py")).unwrap();
    assert!(d.contains("wrapped-user-default"));
    assert!(rep.stale_removed.is_empty());
    let back = load_host_inventory(&rep.prefix).unwrap().unwrap();
    assert_eq!(&back.wrapped[..], [lf.clone()]);
    // Second install: wrap stays exactly-once, nothing stale.
    let again = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(std::fs::read_to_string(lf.join(WRAPPED_NAME))
        .unwrap()
        .contains("def main"));
    assert!(std::fs::read_to_string(lf.join("default.py"))
        .unwrap()
        .contains("wrapped-user-default"));
    assert!(again.stale_removed.is_empty(), "{:?}", again.stale_removed);
    assert!(again.stale_skipped.is_empty(), "{:?}", again.stale_skipped);
    assert_eq!(load_host_inventory(&rep.prefix).unwrap().unwrap(), back);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn icons_collapse_to_single_line() {
    let home = PathBuf::from("/home/u");
    let icon = |size: u32| {
        verify_entry(
            home.join(format!(
                ".local/share/icons/hicolor/{size}x{size}/apps/tuxgt.png"
            )),
            HostKind::File,
            HostStatus::Missing,
        )
    };
    let other = verify_entry(
        home.join(".config/tuxgt.conf"),
        HostKind::File,
        HostStatus::Modified,
    );
    let report = vec![other.clone(), icon(16), icon(32)];
    let lines = install_check_lines(&report);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], format!("{}\tmodified", other.path.display()));
    assert_eq!(lines[1], "icons\t0/2\tmissing");
    let (rest, collapsed) =
        collapse_icon_paths(&report.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    assert_eq!(rest, vec![other.path.clone()]);
    assert_eq!(collapsed, Some("icons\t2/9".to_string()));
    assert!(icons_check_line(&[other]).is_none());
}

// E67 --check shape: explicit-home fixtures, never the real `$HOME`
// and never the process `HOME` env.
#[test]
#[cfg(unix)]
fn check_clean_install_is_all_ok_no_lines() {
    let (base, prefix, home) = e66_setup("e67-check-ok");
    install_userland_with_home(&prefix, &prefix, &home).unwrap();
    let report = verify_host_install(&prefix, &home);
    assert!(install_check_ok(&report));
    assert!(install_check_lines(&report).is_empty());
    assert_eq!(
        install_check_summary(&report),
        format!("ok\t{}/{}", report.len(), report.len())
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn check_missing_paths_are_lines_not_ok() {
    let base = inv_base("e67-check-missing");
    let prefix = base.join("pfx");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    // No inventory file and nothing on disk: every intended path is
    // still verified, extras unknown, not a failure by itself.
    assert!(load_host_inventory(&prefix).unwrap().is_none());
    let report = verify_host_install(&prefix, &home);
    assert!(!report.is_empty());
    assert!(!install_check_ok(&report));
    let lines = install_check_lines(&report);
    assert_eq!(lines.len(), report.len());
    for (entry, line) in report.iter().zip(lines.iter()) {
        assert_eq!(line, &format!("{}\tmissing", entry.path.display()));
    }
    assert_eq!(
        install_check_summary(&report),
        format!("ok\t0/{}", report.len())
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn check_modified_conf_is_single_non_ok_line() {
    let (base, prefix, home) = e66_setup("e67-check-mod");
    install_userland_with_home(&prefix, &prefix, &home).unwrap();
    let conf = home.join(".config/tuxgt.conf");
    std::fs::write(&conf, "# user edit\nTUXGT_DATA=/nope\n").unwrap();
    let report = verify_host_install(&prefix, &home);
    assert!(!install_check_ok(&report));
    let lines = install_check_lines(&report);
    assert_eq!(lines, vec![format!("{}\tmodified", conf.display())]);
    assert_eq!(
        install_check_summary(&report),
        format!("ok\t{}/{}", report.len() - 1, report.len())
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn install_report_lists_localfixes_hooks() {
    let (base, prefix, home) = e66_setup("e67-hooks");
    let rep = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    let lf = home.join(".config/protonfixes/localfixes");
    assert_eq!(&rep.hooks[..], [lf.join("tuxgt.py"), lf.join("default.py")]);
    let _ = std::fs::remove_dir_all(&base);
}
