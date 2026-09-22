use std::path::PathBuf;

pub fn boot_conf() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let text = std::fs::read_to_string(home.join(".config/tuxgt.conf")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("TUXGT_DATA=") {
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            if !v.is_empty() {
                return Some(PathBuf::from(v));
            }
        }
    }
    None
}

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TUXGT_DATA") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Some(p) = boot_conf() {
        return p;
    }
    if let Ok(exe) = std::env::current_exe() {
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        if let Some(dir) = exe.parent() {
            if dir.file_name().is_some_and(|n| n == "bin") {
                if let Some(prefix) = dir.parent() {
                    return prefix.to_path_buf();
                }
            }
        }
    }
    dirs_home().join("tuxgt")
}

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TUXGT_CONFIG") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    data_dir().join("config")
}

pub(crate) fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Shared debug-log gate: `TUXGT_DEBUG=1` in env OR `config_dir()/ui.toml`
/// parses with `debug_log = true`. Used by the app init and the launch env.
pub fn debug_log_enabled() -> bool {
    if std::env::var_os("TUXGT_DEBUG").is_some_and(|v| v == "1") {
        return true;
    }
    std::fs::read_to_string(config_dir().join("ui.toml"))
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|v| v.get("debug_log").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}
