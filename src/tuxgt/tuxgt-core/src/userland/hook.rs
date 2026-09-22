use std::path::Path;

use super::*;
use crate::{atomic_write_str, Result};

pub(crate) fn is_ours(text: &str) -> bool {
    text.contains(HOOK_MARKER)
}

/// File-level compose into `localfixes`. Never symlink the directory.
/// Foreign `default.py` is renamed once to `_tuxgt_wrapped_default.py`.
pub fn install_proton_hook(prefix: &Path, localfixes: &Path) -> Result<()> {
    let hooks = prefix.join("share/protonfixes");
    std::fs::create_dir_all(&hooks)?;
    atomic_write_str(&hooks.join("tuxgt_apply.py"), TUXGT_APPLY_PY)?;
    atomic_write_str(&hooks.join("default.py"), DEFAULT_PY)?;
    atomic_write_str(&hooks.join("default_wrap.py"), DEFAULT_WRAP_PY)?;
    atomic_write_str(&hooks.join("tuxgt.py"), TUXGT_PY)?;

    std::fs::create_dir_all(localfixes)?;
    let init = localfixes.join("__init__.py");
    if !init.exists() {
        atomic_write_str(&init, "")?;
    }
    atomic_write_str(&localfixes.join("tuxgt.py"), TUXGT_PY)?;

    let default = localfixes.join("default.py");
    let wrapped = localfixes.join(WRAPPED_NAME);
    if default.is_file() {
        let text = std::fs::read_to_string(&default)?;
        if is_ours(&text) {
            if text.contains("wrapped-user-default") {
                atomic_write_str(&default, DEFAULT_WRAP_PY)?;
            } else {
                atomic_write_str(&default, DEFAULT_PY)?;
            }
        } else if wrapped.exists() {
            atomic_write_str(&default, DEFAULT_WRAP_PY)?;
        } else {
            std::fs::rename(&default, &wrapped)?;
            atomic_write_str(&default, DEFAULT_WRAP_PY)?;
        }
    } else {
        atomic_write_str(&default, DEFAULT_PY)?;
    }
    Ok(())
}

/// Restore a wrapped foreign default.py. Ours-only default is removed.
pub fn uninstall_proton_hook(localfixes: &Path) -> Result<()> {
    let default = localfixes.join("default.py");
    let wrapped = localfixes.join(WRAPPED_NAME);
    if wrapped.is_file() {
        let _ = std::fs::remove_file(&default);
        std::fs::rename(&wrapped, &default)?;
    } else if default.is_file() {
        let text = std::fs::read_to_string(&default).unwrap_or_default();
        if is_ours(&text) {
            std::fs::remove_file(&default)?;
        }
    }
    let tuxgt = localfixes.join("tuxgt.py");
    if tuxgt.is_file() {
        let text = std::fs::read_to_string(&tuxgt).unwrap_or_default();
        if is_ours(&text) {
            std::fs::remove_file(&tuxgt)?;
        }
    }
    Ok(())
}
