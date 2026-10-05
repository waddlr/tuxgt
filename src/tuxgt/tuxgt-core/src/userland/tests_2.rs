use super::testing::*;
use super::*;
use crate::Error;
use std::path::{Path, PathBuf};

#[test]
#[cfg(unix)]
fn uninstall_removes_owned_links_conf_and_ours_hooks() {
    let (base, prefix, home) = e66_setup("e68-owned");
    let first = install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(first.stale_removed.is_empty());
    let inv_path = prefix.join("config/host-install.toml");
    assert!(inv_path.is_file());
    let lf = home.join(".config/protonfixes/localfixes");
    assert!(lf.join("tuxgt.py").is_file());
    assert!(lf.join("default.py").is_file());
    let init = lf.join("__init__.py");
    assert!(init.is_file());

    let rep = uninstall_userland(&prefix).unwrap();
    assert_eq!(rep.prefix, prefix);
    // Owned host files are gone: PATH links, desktop link, boot conf,
    // and our hook files.
    assert!(std::fs::symlink_metadata(home.join(".local/bin/tuxgt")).is_err());
    assert!(std::fs::symlink_metadata(home.join(".local/bin/tuxgt-launcher")).is_err());
    assert!(
        std::fs::symlink_metadata(home.join(".local/share/applications/tuxgt.desktop")).is_err()
    );
    assert!(!home.join(".config/tuxgt.conf").exists());
    assert!(!lf.join("tuxgt.py").exists());
    assert!(!lf.join("default.py").exists());
    // ... and reported as removed, including the inventory file.
    for rel in [
        ".local/bin/tuxgt",
        ".local/bin/tuxgt-launcher",
        ".local/share/applications/tuxgt.desktop",
        ".config/tuxgt.conf",
        ".config/protonfixes/localfixes/tuxgt.py",
        ".config/protonfixes/localfixes/default.py",
    ] {
        assert!(rep.removed.contains(&home.join(rel)), "removed: {rel}");
    }
    assert!(rep.removed.contains(&inv_path));
    assert!(rep.skipped.is_empty(), "{:?}", rep.skipped);
    // PREFIX tree and untracked helpers survive.
    assert!(prefix.join("bin/tuxgt").is_file());
    assert!(prefix.join("bin/tuxgt-launcher").is_file());
    assert!(init.is_file(), "localfixes/__init__.py is never tracked");
    assert!(!inv_path.exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn uninstall_restores_foreign_wrap_native_and_heroic() {
    let (base, prefix, home) = e66_setup("e68-wrap");
    let lf = home.join(".config/protonfixes/localfixes");
    std::fs::create_dir_all(&lf).unwrap();
    std::fs::write(&lf.join("default.py"), "def main():\n    pass  # foreign\n").unwrap();
    let flatpak = home.join(".var/app/com.heroicgameslauncher.hgl/config/protonfixes/localfixes");
    std::fs::create_dir_all(&flatpak).unwrap();
    std::fs::write(&flatpak.join("default.py"), "FOREIGN_HEROIC = True\n").unwrap();
    install_userland_with_home(&prefix, &prefix, &home).unwrap();
    assert!(lf.join(WRAPPED_NAME).is_file());
    assert!(flatpak.join(WRAPPED_NAME).is_file());

    uninstall_userland(&prefix).unwrap();
    assert_eq!(
        std::fs::read_to_string(lf.join("default.py")).unwrap(),
        "def main():\n    pass  # foreign\n"
    );
    assert_eq!(
        std::fs::read_to_string(flatpak.join("default.py")).unwrap(),
        "FOREIGN_HEROIC = True\n"
    );
    assert!(!lf.join(WRAPPED_NAME).exists());
    assert!(!flatpak.join(WRAPPED_NAME).exists());
    assert!(!lf.join("tuxgt.py").exists());
    assert!(!flatpak.join("tuxgt.py").exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn uninstall_skips_modified_paths_and_keeps_prefix_tree() {
    let (base, prefix, home) = e66_setup("e68-skip");
    install_userland_with_home(&prefix, &prefix, &home).unwrap();
    // User replaces the boot conf and retargets a PATH link: no longer
    // ours, left in place.
    let conf = home.join(".config/tuxgt.conf");
    std::fs::write(&conf, "# user edit\nTUXGT_DATA=/nope\n").unwrap();
    let link = home.join(".local/bin/tuxgt");
    make_link(&link, Path::new("/elsewhere/tuxgt"));
    // PREFIX payloads and global env the inventory never covers.
    for rel in [
        "games/sentinel.txt",
        "mods/sentinel.txt",
        "downloads/sentinel.txt",
        "config/tuxgt.sqlite",
    ] {
        let p = prefix.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"prefix payload\n").unwrap();
    }
    let envd = home.join(".config/environment.d/90-tuxgt.conf");
    std::fs::create_dir_all(envd.parent().unwrap()).unwrap();
    std::fs::write(&envd, b"TUXGT_FOO=1\n").unwrap();

    let rep = uninstall_userland(&prefix).unwrap();
    assert_eq!(
        std::fs::read_to_string(&conf).unwrap(),
        "# user edit\nTUXGT_DATA=/nope\n"
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        PathBuf::from("/elsewhere/tuxgt")
    );
    assert!(rep.skipped.contains(&conf));
    assert!(rep.skipped.contains(&link));
    assert!(!rep.removed.contains(&conf));
    assert!(!rep.removed.contains(&link));
    for rel in [
        "bin/tuxgt",
        "games/sentinel.txt",
        "mods/sentinel.txt",
        "downloads/sentinel.txt",
        "config/tuxgt.sqlite",
    ] {
        assert!(prefix.join(rel).is_file(), "PREFIX survives: {rel}");
    }
    assert!(envd.is_file(), "environment.d survives");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn reinstall_over_existing_prefix_replaces_program_files() {
    // Issue #1: re-running `tuxgt install` from a fresh tree must replace
    // the prefix program files, never silently keep the stale tree.
    let base = inv_base("reinstall-replace");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let put = |root: &Path, rel: &str, bytes: &[u8]| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
    };
    // Old live prefix: stale program files, a dropped official, user data.
    let dest = base.join("dest");
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "lib/libtuxgt-launcher.so",
        "share/protonfixes/tuxgt_apply.py",
        "share/templates/add.toml",
        "share/icons/hicolor/16x16/apps/tuxgt.png",
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
        ("downloads/keep.part", b"user-download"),
        ("config/tuxgt.sqlite", b"user-db"),
        ("mods/user/mine.toml", b"user-recipe"),
        ("mods/user/mine/payload.bin", b"user-payload"),
        ("mods/contrib/other.toml", b"registry"),
    ] {
        put(&dest, rel, bytes);
    }
    // Fresh unpacked tree: new program files, tarball layout (no payloads,
    // no user dirs) plus junk a real tarball never ships.
    let src = base.join("src");
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "lib/libtuxgt-launcher.so",
        "share/protonfixes/tuxgt_apply.py",
        "share/templates/add.toml",
        "share/icons/hicolor/16x16/apps/tuxgt.png",
        "share/applications/tuxgt.desktop",
        "mods/official/kept.toml",
        "mods/official/fresh.toml",
    ] {
        put(&src, rel, b"NEW");
    }
    for rel in [
        "games/evil.txt",
        "config/evil.txt",
        "downloads/evil.txt",
        "mods/user/evil.toml",
    ] {
        put(&src, rel, b"evil");
    }
    std::os::unix::fs::symlink("/etc/hostname", src.join("bin/evil-link")).unwrap();

    let rep = install_userland_with_home(&src, &dest, &home).unwrap();
    assert!(!rep.moved, "existing prefix is overlaid, not moved");
    assert!(!rep.in_place);
    // 2 bin + 1 lib + 1 protonfixes + 1 template + 1 icon + 2 official
    // TOMLs (desktop skipped, install rewrites it; src junk + symlink skipped).
    assert_eq!(rep.overlaid, 8);
    for rel in [
        "bin/tuxgt",
        "bin/tuxgt-launcher",
        "lib/libtuxgt-launcher.so",
        "share/templates/add.toml",
        "share/icons/hicolor/16x16/apps/tuxgt.png",
        "mods/official/kept.toml",
        "mods/official/fresh.toml",
    ] {
        assert_eq!(
            std::fs::read(dest.join(rel)).unwrap(),
            b"NEW",
            "replaced: {rel}"
        );
    }
    // Hook scripts refresh from the baked copy (stale bytes gone).
    assert_eq!(
        std::fs::read(dest.join("share/protonfixes/tuxgt_apply.py")).unwrap(),
        TUXGT_APPLY_PY.as_bytes()
    );
    // Dropped official: toml + payload dir gone; kept payload intact.
    assert!(!dest.join("mods/official/gone.toml").exists());
    assert!(!dest.join("mods/official/gone").exists());
    assert_eq!(
        std::fs::read(dest.join("mods/official/kept/payload.bin")).unwrap(),
        b"kept-payload"
    );
    // User data untouched.
    for (rel, bytes) in [
        ("games/g0/sentinel.txt", b"user-game" as &[u8]),
        ("downloads/keep.part", b"user-download"),
        ("config/tuxgt.sqlite", b"user-db"),
        ("mods/user/mine.toml", b"user-recipe"),
        ("mods/user/mine/payload.bin", b"user-payload"),
        ("mods/contrib/other.toml", b"registry"),
    ] {
        assert_eq!(
            std::fs::read(dest.join(rel)).unwrap(),
            bytes,
            "untouched: {rel}"
        );
    }
    // Src junk never lands in the prefix.
    for rel in [
        "games/evil.txt",
        "config/evil.txt",
        "downloads/evil.txt",
        "mods/user/evil.toml",
        "bin/evil-link",
    ] {
        assert!(!dest.join(rel).exists(), "never copied: {rel}");
    }
    // Package modes (deploy parity): 755 executables, 644 the rest.
    use std::os::unix::fs::PermissionsExt;
    let mode = |rel: &str| {
        std::fs::metadata(dest.join(rel))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode("bin/tuxgt"), 0o755);
    assert_eq!(mode("lib/libtuxgt-launcher.so"), 0o755);
    assert_eq!(mode("share/templates/add.toml"), 0o644);
    // Desktop rewritten for this prefix; themed icons follow the new tree.
    let desk = std::fs::read_to_string(dest.join("share/applications/tuxgt.desktop")).unwrap();
    assert!(desk.contains(&format!("Exec={}/bin/tuxgt", rep.prefix.display())));
    assert_eq!(
        std::fs::read(home.join(".local/share/icons/hicolor/16x16/apps/tuxgt.png")).unwrap(),
        b"NEW"
    );
    // Conf + PATH links point at the live prefix.
    let conf = std::fs::read_to_string(home.join(".config/tuxgt.conf")).unwrap();
    assert!(conf.contains(&format!("TUXGT_DATA={}", rep.prefix.display())));
    assert_eq!(
        std::fs::read_link(home.join(".local/bin/tuxgt")).unwrap(),
        rep.prefix.join("bin/tuxgt")
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn uninstall_without_inventory_errors_install_once() {
    let base = inv_base("e68-noinv");
    let prefix = base.join("pfx");
    std::fs::create_dir_all(&prefix).unwrap();
    let err = uninstall_userland(&prefix).unwrap_err();
    assert!(matches!(err, Error::Install(_)));
    let msg = format!("{err}");
    assert!(msg.contains("tuxgt install"), "{msg}");
    let _ = std::fs::remove_dir_all(&base);
}
