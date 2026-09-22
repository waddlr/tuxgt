use std::fs;
use std::path::Path;

use crate::Result;

/// Write `bytes` to `path` via a `.tmp` sibling + rename, creating parents.
/// Single owner (T01); previously copied in `download`, `stage`, `apply`.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// `str` wrapper over [`atomic_write`]; previously `userland::hook::atomic_write_str`.
pub(crate) fn atomic_write_str(path: &Path, text: &str) -> Result<()> {
    atomic_write(path, text.as_bytes())
}
