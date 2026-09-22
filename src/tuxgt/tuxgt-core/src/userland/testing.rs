use super::*;
use std::path::{Path, PathBuf};

pub(crate) fn inv_base(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("tuxgt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

pub(crate) fn make_link(link: &Path, target: &Path) {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let _ = std::fs::remove_file(link);
    std::os::unix::fs::symlink(target, link).unwrap();
}

pub(crate) fn verify_entry(path: PathBuf, kind: HostKind, status: HostStatus) -> HostVerifyEntry {
    HostVerifyEntry { path, kind, status }
}

pub(crate) fn e66_setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
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

pub(crate) fn with_home(base: &Path) -> Option<std::ffi::OsString> {
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let prev = std::env::var_os("HOME");
    // SAFETY: only this test writes HOME; the suite's other HOME
    // reader (clobber test) only needs it to exist.
    unsafe { std::env::set_var("HOME", &home) };
    prev
}

pub(crate) fn restore_home(prev: Option<std::ffi::OsString>) {
    // SAFETY: test-only; pairs with with_home.
    unsafe {
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}
