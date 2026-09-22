use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::*;
use crate::{Error, Result};

/// `rename(2)` cross-device error (Linux value; no libc dep for one constant).
pub(crate) const EXDEV: i32 = 18;

/// Hicolor icon sizes shipped for `Icon=tuxgt` (9 dirs under
/// `$PREFIX/share/icons/` and `~/.local/share/icons/`).
pub const ICON_SIZES: [u32; 9] = [16, 24, 32, 48, 64, 128, 256, 512, 1024];

/// Where `tuxgt install` put things.
#[derive(Clone, Debug)]
pub struct InstallReport {
    pub prefix: PathBuf,
    /// A `rename(src, dest)` happened.
    pub moved: bool,
    /// `rename` hit `EXDEV`; links + conf point at the source tree.
    pub in_place: bool,
    pub conf: PathBuf,
    pub bin_links: Vec<(PathBuf, PathBuf)>,
    pub desktop_target: PathBuf,
    pub desktop_link: PathBuf,
    /// Previous-inventory paths no longer intended, removed because we
    /// still owned them (our symlink target, old-inventory sha256, or
    /// hook marker). Never includes PREFIX internals.
    pub stale_removed: Box<[PathBuf]>,
    /// Previous-inventory paths no longer intended but left alone: content
    /// no longer matches the old inventory (user replaced it), or the
    /// path sits under PREFIX (never delete the tree, `games/`, `config/`).
    pub stale_skipped: Box<[PathBuf]>,
    /// Protonfixes `localfixes` files written (native, plus Heroic Flatpak
    /// when that tree exists).
    pub hooks: Box<[PathBuf]>,
    /// Hicolor icons installed: `(present, total)` of the 9
    /// `~/.local/share/icons/hicolor/<size>/apps/tuxgt.png` files.
    pub icons: (usize, usize),
}

pub(crate) fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::Install("HOME is not set".into()))
}

/// The packaged prefix of the running binary: `current_exe` must be
/// `…/bin/tuxgt` (canonicalized, so a `~/.local/bin` symlink resolves to
/// the real prefix). Errors for dev builds (`target/debug/tuxgt`).
pub fn packaged_prefix() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(Error::Io)?;
    let canon = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dir = canon.parent().ok_or_else(|| {
        Error::Install("tuxgt install: run the packaged binary (…/bin/tuxgt)".into())
    })?;
    if dir.file_name().is_none_or(|n| n != "bin") {
        return Err(Error::Install(
            "tuxgt install: run the packaged binary (…/bin/tuxgt)".into(),
        ));
    }
    dir.parent().map(Path::to_path_buf).ok_or_else(|| {
        Error::Install("tuxgt install: run the packaged binary (…/bin/tuxgt)".into())
    })
}

/// Expand a leading `~` against `$HOME`.
pub fn expand_tilde(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if s == "~" || s.starts_with("~/") {
        if let Ok(h) = home() {
            return h.join(&s[1..].trim_start_matches('/'));
        }
    }
    p.to_path_buf()
}

pub(crate) fn symlink_force(link: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    #[cfg(not(unix))]
    std::fs::copy(target, link)?;
    Ok(())
}

pub fn icon_path(prefix: &Path, size: u32) -> PathBuf {
    prefix.join(format!("share/icons/hicolor/{size}x{size}/apps/tuxgt.png"))
}

pub(crate) fn packaged_icon_bytes(prefix: &Path, size: u32) -> Option<Vec<u8>> {
    std::fs::read(icon_path(prefix, size)).ok()
}

pub(crate) fn rewrite_desktop(path: &Path, prefix: &Path) -> Result<()> {
    let mut text = std::fs::read_to_string(path).unwrap_or_else(|_| {
        "[Desktop Entry]\nType=Application\nName=TuxGT\nComment=Linux-native manager for injector/runtime mods\nExec=tuxgt\nIcon=tuxgt\nCategories=Game;\nTerminal=false\n"
            .to_string()
    });
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let exec = format!("Exec={}/bin/tuxgt", prefix.display());
    let icon = "Icon=tuxgt".to_string();
    let name = "Name=TuxGT".to_string();
    let comment = "Comment=Linux-native manager for injector/runtime mods".to_string();
    let mut out = Vec::new();
    let (mut has_exec, mut has_icon, mut has_name, mut has_comment) = (false, false, false, false);
    for line in text.lines() {
        if line.starts_with("Exec=") {
            out.push(exec.clone());
            has_exec = true;
        } else if line.starts_with("Icon=") {
            out.push(icon.clone());
            has_icon = true;
        } else if line.starts_with("Name=") {
            out.push(name.clone());
            has_name = true;
        } else if line.starts_with("Comment=") {
            out.push(comment.clone());
            has_comment = true;
        } else {
            out.push(line.to_string());
        }
    }
    if !has_exec {
        out.push(exec);
    }
    if !has_icon {
        out.push(icon);
    }
    if !has_name {
        out.push(name);
    }
    if !has_comment {
        out.push(comment);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, out.join("\n") + "\n")?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Move (or keep) the unpacked tree at `src_prefix` to `dest`, then write
/// `~/.config/tuxgt.conf` plus the `~/.local` symlinks. Never sudo, never
/// `/usr`. On `EXDEV` the tree stays where it is and the links + conf
/// point at it.
pub fn install_userland(src_prefix: &Path, dest: &Path) -> Result<InstallReport> {
    let home = home()?;
    install_userland_with_home(src_prefix, dest, &home)
}

/// `install_userland` with an explicit home dir (tests pass a temp dir;
/// production passes `$HOME`). Reconciles: drops stale owned host paths
/// from the previous inventory, writes the intended set, saves inventory.
pub fn install_userland_with_home(
    src_prefix: &Path,
    dest: &Path,
    home: &Path,
) -> Result<InstallReport> {
    if dest.as_os_str().is_empty() {
        return Err(Error::Install("empty prefix".into()));
    }
    let dest = expand_tilde(dest);
    let dest = if dest.is_absolute() {
        dest
    } else {
        std::env::current_dir().map_err(Error::Io)?.join(&dest)
    };
    let src_canon = std::fs::canonicalize(src_prefix).unwrap_or_else(|_| src_prefix.to_path_buf());
    let dest_canon = std::fs::canonicalize(&dest).unwrap_or_else(|_| dest.clone());

    let (prefix, moved, in_place) = if src_canon == dest_canon {
        (dest_canon, false, false)
    } else if !dest.exists() {
        match std::fs::rename(&src_canon, &dest) {
            Ok(()) => (
                std::fs::canonicalize(&dest).unwrap_or(dest.clone()),
                true,
                false,
            ),
            Err(e) if e.raw_os_error() == Some(EXDEV) => {
                eprintln!(
                    "cannot move across filesystems; PREFIX stays at {}. To relocate: mv {} {} && {}/bin/tuxgt install",
                    src_canon.display(),
                    src_canon.display(),
                    dest.display(),
                    dest.display()
                );
                (src_canon.clone(), false, true)
            }
            Err(e) => return Err(Error::Io(e)),
        }
    } else if dest.join("bin/tuxgt").is_file() {
        (dest_canon, false, false)
    } else {
        return Err(Error::Install(format!(
            "refusing to clobber {}",
            dest.display()
        )));
    };

    // E66 reconcile: intended manifest first, then drop stale owned paths
    // from the previous inventory (no inventory = legacy install, skip).
    let intended = intended_host_manifest(&prefix, home);
    let previous = load_host_inventory(&prefix)?;
    let (stale_removed, stale_skipped) = match &previous {
        None => (Box::default(), Box::default()),
        Some(inv) => remove_stale_owned(&prefix, &intended, inv),
    };

    let conf = home.join(".config/tuxgt.conf");
    if let Some(parent) = conf.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = conf.with_extension("tmp");
    std::fs::write(
        &tmp,
        format!("# tuxgt boot\nTUXGT_DATA={}\n", prefix.display()),
    )?;
    std::fs::rename(&tmp, &conf)?;

    let local_bin = home.join(".local/bin");
    let local_apps = home.join(".local/share/applications");
    std::fs::create_dir_all(&local_bin)?;
    std::fs::create_dir_all(&local_apps)?;
    let mut bin_links = Vec::new();
    for name in ["tuxgt", "tuxgt-launcher"] {
        let target = prefix.join("bin").join(name);
        if target.is_file() {
            let link = local_bin.join(name);
            symlink_force(&link, &target)?;
            bin_links.push((link, target));
        }
    }

    let desktop_target = prefix.join("share/applications/tuxgt.desktop");
    rewrite_desktop(&desktop_target, &prefix)?;
    let desktop_link = local_apps.join("tuxgt.desktop");
    symlink_force(&desktop_link, &desktop_target)?;

    // Themed icons: copy all 9 packaged sizes into the XDG theme tree.
    let mut icon_ok = 0usize;
    for size in ICON_SIZES {
        let src = icon_path(&prefix, size);
        let dst = home.join(format!(
            ".local/share/icons/hicolor/{size}x{size}/apps/tuxgt.png"
        ));
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if src.is_file() && (std::fs::copy(&src, &dst).is_ok() || dst.is_file()) {
            icon_ok += 1;
        }
    }
    install_proton_hook(&prefix, &home.join(".config/protonfixes/localfixes"))?;
    let flatpak = flatpak_localfixes(home);
    if flatpak_tree_present(home) {
        install_proton_hook(&prefix, &flatpak)?;
    }

    // `wrapped` = localfixes dirs holding a renamed foreign default; the
    // marker file exists exactly when a wrap happened, so the list is
    // stable across re-installs.
    let mut wrapped = Vec::new();
    for lf in [
        home.join(".config/protonfixes/localfixes"),
        flatpak_localfixes(home),
    ] {
        if lf.join(WRAPPED_NAME).is_file() {
            wrapped.push(lf);
        }
    }
    save_host_inventory(
        &prefix,
        &host_inventory_from_intended(&prefix, &intended, &wrapped),
    )?;

    let hooks: Box<[PathBuf]> = intended
        .iter()
        .filter(|e| is_required_host_path(&e.path))
        .map(|e| e.path.clone())
        .collect::<Vec<_>>()
        .into_boxed_slice();

    Ok(InstallReport {
        prefix,
        moved,
        in_place,
        conf,
        bin_links,
        desktop_target,
        desktop_link,
        stale_removed,
        stale_skipped,
        hooks,
        icons: (icon_ok, ICON_SIZES.len()),
    })
}

/// Remove previous-inventory paths that are no longer intended, when we
/// still own them: our symlink target, old-inventory file sha256, or the
/// hook marker. Returns `(removed, skipped)`. Missing paths are already
/// gone (neither). Paths under PREFIX are never deleted (`games/`,
/// `config/`, the tree itself) — they land in `skipped`. Only files and
/// symlinks are ever removed; directories are left alone.
pub(crate) fn remove_stale_owned(
    prefix: &Path,
    intended: &[IntendedHostEntry],
    previous: &HostInstallInventory,
) -> (Box<[PathBuf]>, Box<[PathBuf]>) {
    let intended_set: HashSet<&Path> = intended.iter().map(|e| e.path.as_path()).collect();
    let (mut removed, mut skipped): (Vec<PathBuf>, Vec<PathBuf>) = (Vec::new(), Vec::new());
    for old in &previous.paths {
        if intended_set.contains(old.path.as_path()) {
            continue;
        }
        if old.path.starts_with(prefix) {
            skipped.push(old.path.clone());
            continue;
        }
        let owned = match old.kind {
            HostKind::Symlink => match std::fs::read_link(&old.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
                Ok(target) => Some(target == old.target.clone().unwrap_or_default()),
            },
            HostKind::File => match std::fs::read(&old.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
                Ok(bytes) => {
                    if Some(sha256_hex_bytes(&bytes)) == old.sha256 {
                        Some(true)
                    } else {
                        // Ours-marker hook file still owned even when the
                        // bytes drifted from the recorded hash.
                        Some(std::str::from_utf8(&bytes).map(is_ours).unwrap_or(false))
                    }
                }
            },
        };
        match owned {
            None => {}
            Some(true) => {
                if std::fs::remove_file(&old.path).is_ok() {
                    removed.push(old.path.clone());
                } else {
                    // Vanished or turned into a dir between check and
                    // remove: leave it, report as skipped.
                    skipped.push(old.path.clone());
                }
            }
            Some(false) => skipped.push(old.path.clone()),
        }
    }
    (removed.into_boxed_slice(), skipped.into_boxed_slice())
}

pub(crate) const HOOK_MARKER: &str = "tuxgt-proton-hook v1";
pub(crate) const WRAPPED_NAME: &str = "_tuxgt_wrapped_default.py";

pub const DEFAULT_PY: &str = include_str!("../../../../protonfixes-hook/default.py");
pub const DEFAULT_WRAP_PY: &str = include_str!("../../../../protonfixes-hook/default_wrap.py");
pub const TUXGT_PY: &str = include_str!("../../../../protonfixes-hook/tuxgt.py");
pub const TUXGT_APPLY_PY: &str = include_str!("../../../../protonfixes-hook/tuxgt_apply.py");
