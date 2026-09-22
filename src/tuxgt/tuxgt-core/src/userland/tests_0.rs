use super::testing::*;
use super::*;
use crate::Error;
use std::path::{Path, PathBuf};

#[test]
fn packaged_prefix_needs_bin_parent() {
    // Dev binary is not …/bin/tuxgt; only assert the error path when the
    // test binary itself is not packaged (true in cargo test).
    if let Ok(p) = packaged_prefix() {
        assert!(p.join("bin/tuxgt").is_file(), "{p:?}");
    }
}

#[test]
fn reinstall_same_prefix_overwrites() {
    let base = std::env::temp_dir().join(format!("tuxgt-userland-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let prev = with_home(&base);
    let src = base.join("src/tuxgt");
    std::fs::create_dir_all(src.join("bin")).unwrap();
    std::fs::write(src.join("bin/tuxgt"), b"x").unwrap();
    std::fs::write(src.join("bin/tuxgt-launcher"), b"y").unwrap();
    let rep = install_userland(&src, &src).unwrap();
    assert!(!rep.moved);
    assert_eq!(rep.prefix, std::fs::canonicalize(&src).unwrap());
    let conf = std::fs::read_to_string(&rep.conf).unwrap();
    assert!(conf.contains(&format!("TUXGT_DATA={}", rep.prefix.display())));
    for (link, target) in &rep.bin_links {
        assert_eq!(&std::fs::read_link(link).unwrap(), target);
    }
    assert_eq!(
        &std::fs::read_link(&rep.desktop_link).unwrap(),
        &rep.desktop_target
    );
    let desk = std::fs::read_to_string(&rep.desktop_target).unwrap();
    assert!(desk.contains(&format!("Exec={}/bin/tuxgt", rep.prefix.display())));
    // Second run: same prefix is a clean overwrite.
    install_userland(&src, &src).unwrap();
    restore_home(prev);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn refuses_to_clobber_foreign_dir() {
    let base = std::env::temp_dir().join(format!("tuxgt-clobber-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let src = base.join("src/tuxgt");
    std::fs::create_dir_all(src.join("bin")).unwrap();
    std::fs::write(src.join("bin/tuxgt"), b"x").unwrap();
    let dest = base.join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("other.txt"), b"user").unwrap();
    let err = install_userland(&src, &dest).unwrap_err();
    assert!(matches!(err, Error::Install(_)));
    assert!(dest.join("other.txt").is_file());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn proton_hook_creates_default() {
    let base = std::env::temp_dir().join(format!("tuxgt-hook-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let prefix = base.join("pfx");
    let lf = base.join("localfixes");
    install_proton_hook(&prefix, &lf).unwrap();
    let d = std::fs::read_to_string(lf.join("default.py")).unwrap();
    assert!(d.contains(HOOK_MARKER));
    assert!(!d.contains("wrapped-user-default"));
    assert!(lf.join("tuxgt.py").is_file());
    assert!(prefix.join("share/protonfixes/tuxgt_apply.py").is_file());
    install_proton_hook(&prefix, &lf).unwrap();
    assert!(!lf.join(WRAPPED_NAME).exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn proton_hook_wraps_foreign_default_once() {
    let base = std::env::temp_dir().join(format!("tuxgt-hook-wrap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let prefix = base.join("pfx");
    let lf = base.join("localfixes");
    std::fs::create_dir_all(&lf).unwrap();
    std::fs::write(lf.join("default.py"), "def main():\n    pass\n").unwrap();
    std::fs::write(lf.join("300.py"), "# user game fix\n").unwrap();
    install_proton_hook(&prefix, &lf).unwrap();
    assert!(std::fs::read_to_string(lf.join(WRAPPED_NAME))
        .unwrap()
        .contains("def main"));
    let d = std::fs::read_to_string(lf.join("default.py")).unwrap();
    assert!(d.contains("wrapped-user-default"));
    assert!(lf.join("300.py").is_file());
    install_proton_hook(&prefix, &lf).unwrap();
    assert!(std::fs::read_to_string(lf.join(WRAPPED_NAME))
        .unwrap()
        .contains("def main"));
    std::fs::write(
        lf.join("default.py"),
        "# tuxgt-proton-hook v1\n# wrapped-user-default\nstale\n",
    )
    .unwrap();
    install_proton_hook(&prefix, &lf).unwrap();
    let again = std::fs::read_to_string(lf.join("default.py")).unwrap();
    assert!(again.contains("wrapped-user-default"));
    assert!(!again.contains("stale"));
    uninstall_proton_hook(&lf).unwrap();
    assert!(std::fs::read_to_string(lf.join("default.py"))
        .unwrap()
        .contains("def main"));
    assert!(!lf.join(WRAPPED_NAME).exists());
    assert!(!lf.join("tuxgt.py").exists());
    assert!(lf.join("300.py").is_file());
    let _ = std::fs::remove_dir_all(&base);
}

// E65 inventory tests: HOME-isolated via explicit `home` params, never
// the real `$HOME` and never the process `HOME` env.
fn inv_base(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("tuxgt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn status_of(results: &[HostVerifyEntry], path: &Path) -> HostStatus {
    results
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("no verify entry for {}", path.display()))
        .status
}

#[cfg(unix)]
fn make_link(link: &Path, target: &Path) {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link).unwrap();
}

/// Materialize every intended path on disk exactly as intended.
#[cfg(unix)]
fn materialize_intended(prefix: &Path, home: &Path) {
    for e in intended_host_manifest(prefix, home) {
        match e.kind {
            HostKind::Symlink => make_link(&e.path, e.symlink_target.as_deref().unwrap()),
            HostKind::File => {
                let bytes: Vec<u8> = if e.path.ends_with(".config/tuxgt.conf") {
                    intended_conf_text(prefix).into_bytes()
                } else if e.path.ends_with("tuxgt.py") {
                    TUXGT_PY.as_bytes().to_vec()
                } else if e.sha256.as_deref()
                    == Some(sha256_hex_bytes(DEFAULT_WRAP_PY.as_bytes()).as_str())
                {
                    DEFAULT_WRAP_PY.as_bytes().to_vec()
                } else {
                    DEFAULT_PY.as_bytes().to_vec()
                };
                if let Some(parent) = e.path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&e.path, bytes).unwrap();
            }
        }
    }
}

#[test]
fn intended_list_covers_host_set_only() {
    let base = inv_base("inv-list");
    let prefix = base.join("pfx");
    let home = base.join("home");
    let manifest = intended_host_manifest(&prefix, &home);
    let paths: Vec<PathBuf> = manifest.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            home.join(".local/bin/tuxgt"),
            home.join(".local/bin/tuxgt-launcher"),
            home.join(".local/share/applications/tuxgt.desktop"),
            home.join(".config/tuxgt.conf"),
            home.join(".config/protonfixes/localfixes/tuxgt.py"),
            home.join(".config/protonfixes/localfixes/default.py"),
        ]
    );
    assert_eq!(manifest[0].kind, HostKind::Symlink);
    assert_eq!(
        manifest[0].symlink_target.as_deref(),
        Some(prefix.join("bin/tuxgt").as_path())
    );
    let conf = manifest
        .iter()
        .find(|e| e.path.ends_with(".config/tuxgt.conf"))
        .unwrap();
    assert_eq!(conf.kind, HostKind::File);
    assert_eq!(
        conf.sha256.as_deref(),
        Some(sha256_hex_bytes(intended_conf_text(&prefix).as_bytes()).as_str())
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("__init__.py")),
        "never track localfixes/__init__.py"
    );
    assert!(
        !paths.iter().any(|p| p.starts_with(&prefix)),
        "never track PREFIX internals"
    );
    assert!(
        !paths
            .iter()
            .any(|p| p.to_string_lossy().contains("environment.d")),
        "never track environment.d"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn verify_reports_missing() {
    let base = inv_base("inv-missing");
    let prefix = base.join("pfx");
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let results = verify_host_install(&prefix, &home);
    assert!(!results.is_empty());
    assert!(results.iter().all(|e| e.status == HostStatus::Missing));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn verify_ok_and_file_hash_mismatch() {
    let base = inv_base("inv-hash");
    let prefix = base.join("pfx");
    let home = base.join("home");
    materialize_intended(&prefix, &home);
    let results = verify_host_install(&prefix, &home);
    assert!(
        results.iter().all(|e| e.status == HostStatus::Ok),
        "{results:?}"
    );
    // Hand-edited conf diverges from the intended sha256.
    let conf = home.join(".config/tuxgt.conf");
    std::fs::write(&conf, "# user edit\nTUXGT_DATA=/nope\n").unwrap();
    let results = verify_host_install(&prefix, &home);
    assert_eq!(status_of(&results, &conf), HostStatus::Modified);
    // Everything else still verifies.
    let hook = home.join(".config/protonfixes/localfixes/tuxgt.py");
    assert_eq!(status_of(&results, &hook), HostStatus::Ok);
    // Hand-edited hook file is modified too.
    std::fs::write(&hook, "# user edit\n").unwrap();
    let results = verify_host_install(&prefix, &home);
    assert_eq!(status_of(&results, &hook), HostStatus::Modified);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[cfg(unix)]
fn verify_symlink_target_mismatch_and_wrong_kind() {
    let base = inv_base("inv-link");
    let prefix = base.join("pfx");
    let home = base.join("home");
    materialize_intended(&prefix, &home);
    // Symlink points elsewhere (and dangling counts too).
    let bin_link = home.join(".local/bin/tuxgt");
    make_link(&bin_link, Path::new("/elsewhere/tuxgt"));
    let desktop = home.join(".local/share/applications/tuxgt.desktop");
    make_link(&desktop, Path::new("/elsewhere/tuxgt.desktop"));
    // Regular file where a symlink belongs; symlink where a file belongs.
    let launcher = home.join(".local/bin/tuxgt-launcher");
    let _ = std::fs::remove_file(&launcher);
    std::fs::write(&launcher, b"plain file, not a link").unwrap();
    let conf = home.join(".config/tuxgt.conf");
    let _ = std::fs::remove_file(&conf);
    make_link(&conf, Path::new("/elsewhere/tuxgt.conf"));
    let results = verify_host_install(&prefix, &home);
    assert_eq!(status_of(&results, &bin_link), HostStatus::WrongTarget);
    assert_eq!(status_of(&results, &desktop), HostStatus::WrongTarget);
    assert_eq!(status_of(&results, &launcher), HostStatus::WrongKind);
    assert_eq!(status_of(&results, &conf), HostStatus::WrongKind);
    let _ = std::fs::remove_dir_all(&base);
}

fn verify_entry(path: PathBuf, kind: HostKind, status: HostStatus) -> HostVerifyEntry {
    HostVerifyEntry { path, kind, status }
}

#[test]
fn required_paths_are_localfixes_only() {
    let home = PathBuf::from("/home/u");
    // PATH links, desktop entry, and boot conf never gate the op toast.
    for p in [
        home.join(".local/bin/tuxgt"),
        home.join(".local/bin/tuxgt-launcher"),
        home.join(".local/share/applications/tuxgt.desktop"),
        home.join(".config/tuxgt.conf"),
    ] {
        assert!(!is_required_host_path(&p), "not required: {}", p.display());
    }
    // Native + Heroic Flatpak localfixes files are required.
    for p in [
        home.join(".config/protonfixes/localfixes/tuxgt.py"),
        home.join(".config/protonfixes/localfixes/default.py"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/protonfixes/localfixes/tuxgt.py"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/protonfixes/localfixes/default.py"),
    ] {
        assert!(is_required_host_path(&p), "required: {}", p.display());
    }
}

#[test]
fn required_warning_none_when_healthy_or_unrequired() {
    let home = PathBuf::from("/home/u");
    let ok = |p: PathBuf, kind: HostKind| verify_entry(p, kind, HostStatus::Ok);
    // All ok → no toast.
    let healthy = vec![
        ok(home.join(".local/bin/tuxgt"), HostKind::Symlink),
        ok(
            home.join(".config/protonfixes/localfixes/tuxgt.py"),
            HostKind::File,
        ),
        ok(
            home.join(".config/protonfixes/localfixes/default.py"),
            HostKind::File,
        ),
    ];
    assert_eq!(required_host_warning(&healthy), None);
    assert!(required_host_failures(&healthy).is_empty());
    // PATH-only drift → still no toast.
    let path_only = vec![
        verify_entry(
            home.join(".local/bin/tuxgt"),
            HostKind::Symlink,
            HostStatus::WrongTarget,
        ),
        verify_entry(
            home.join(".local/share/applications/tuxgt.desktop"),
            HostKind::Symlink,
            HostStatus::Missing,
        ),
        verify_entry(
            home.join(".config/tuxgt.conf"),
            HostKind::File,
            HostStatus::Modified,
        ),
        ok(
            home.join(".config/protonfixes/localfixes/tuxgt.py"),
            HostKind::File,
        ),
        ok(
            home.join(".config/protonfixes/localfixes/default.py"),
            HostKind::File,
        ),
    ];
    assert_eq!(required_host_warning(&path_only), None);
    assert!(required_host_failures(&path_only).is_empty());
}

#[test]
fn required_warning_counts_and_prefers_missing_reason() {
    let home = PathBuf::from("/home/u");
    let hook = home.join(".config/protonfixes/localfixes/tuxgt.py");
    let default = home.join(".config/protonfixes/localfixes/default.py");
    // Modified-only → drifted reason.
    let modified = vec![
        verify_entry(hook.clone(), HostKind::File, HostStatus::Modified),
        verify_entry(default.clone(), HostKind::File, HostStatus::Ok),
    ];
    assert_eq!(
        required_host_warning(&modified),
        Some(RequiredHostWarning {
            count: 1,
            missing: false,
            example: hook.clone()
        })
    );
    // Missing anywhere → missing reason, full count, first bad path first.
    let mixed = vec![
        verify_entry(hook.clone(), HostKind::File, HostStatus::WrongKind),
        verify_entry(default.clone(), HostKind::File, HostStatus::Missing),
    ];
    assert_eq!(
        required_host_warning(&mixed),
        Some(RequiredHostWarning {
            count: 2,
            missing: true,
            example: hook.clone()
        })
    );
}
