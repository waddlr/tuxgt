use std::path::{Path, PathBuf};

use super::*;

/// Kind of an intended host path: symlink (compare target string) or file
/// (compare sha256 of bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    Symlink,
    File,
}

/// Per-path verify result for one intended host path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostStatus {
    Ok,
    Missing,
    Modified,
    WrongTarget,
    WrongKind,
}

impl std::fmt::Display for HostStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            HostStatus::Ok => "ok",
            HostStatus::Missing => "missing",
            HostStatus::Modified => "modified",
            HostStatus::WrongTarget => "wrong-target",
            HostStatus::WrongKind => "wrong-kind",
        };
        f.write_str(s)
    }
}

/// One intended host path, computed from this binary every time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntendedHostEntry {
    pub path: PathBuf,
    pub kind: HostKind,
    /// Expected symlink target (canonical string) for `HostKind::Symlink`.
    pub symlink_target: Option<PathBuf>,
    /// Expected sha256 hex of file bytes for `HostKind::File`.
    pub sha256: Option<String>,
}

/// Verify outcome for one intended host path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostVerifyEntry {
    pub path: PathBuf,
    pub kind: HostKind,
    pub status: HostStatus,
}

/// One recorded host path in `$PREFIX/config/host-install.toml`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostInventoryEntry {
    pub path: PathBuf,
    pub kind: HostKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Last successful install, stored at `$PREFIX/config/host-install.toml`.
/// `wrapped` lists the `localfixes` dirs where a foreign `default.py` was
/// renamed to `_tuxgt_wrapped_default.py`, so uninstall can restore them.
/// Never tracks `localfixes/__init__.py`, PREFIX internals, or
/// `environment.d`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostInstallInventory {
    pub prefix: PathBuf,
    pub paths: Vec<HostInventoryEntry>,
    #[serde(default)]
    pub wrapped: Box<[PathBuf]>,
}

pub(crate) fn sha256_hex_bytes(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// True for the 9 themed-icon host paths
/// (`~/.local/share/icons/hicolor/<size>/apps/tuxgt.png`).
pub fn is_icon_host_path(path: &Path) -> bool {
    let mut comps = path.components().rev();
    if !matches!(comps.next().map(|c| c.as_os_str()), Some(n) if n == "tuxgt.png") {
        return false;
    }
    if !matches!(comps.next().map(|c| c.as_os_str()), Some(n) if n == "apps") {
        return false;
    }
    match comps
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
    {
        Some(dir) => {
            let mut parts = dir.splitn(2, 'x');
            match (parts.next(), parts.next()) {
                (Some(w), Some(h)) => {
                    w == h && w.parse::<u32>().is_ok_and(|s| ICON_SIZES.contains(&s))
                }
                _ => false,
            }
        }
        None => false,
    }
}

/// Intended `~/.config/tuxgt.conf` bytes for `prefix`.
pub(crate) fn intended_conf_text(prefix: &Path) -> String {
    format!("# tuxgt boot\nTUXGT_DATA={}\n", prefix.display())
}

/// Intended `default.py` bytes for a `localfixes` dir: the wrap shim when a
/// foreign default is (or was) present, else the plain hook copy.
pub(crate) fn intended_default_text(localfixes: &Path) -> &'static str {
    let wrapped = localfixes.join(WRAPPED_NAME);
    if wrapped.exists() {
        return DEFAULT_WRAP_PY;
    }
    let default = localfixes.join("default.py");
    if let Ok(text) = std::fs::read_to_string(&default) {
        if !is_ours(&text) {
            return DEFAULT_WRAP_PY;
        }
    }
    DEFAULT_PY
}

pub(crate) fn flatpak_localfixes(home: &Path) -> PathBuf {
    home.join(".var/app/com.heroicgameslauncher.hgl/config/protonfixes/localfixes")
}

pub(crate) fn flatpak_tree_present(home: &Path) -> bool {
    let lf = flatpak_localfixes(home);
    lf.exists() || lf.parent().is_some_and(|p| p.exists())
}
