use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use super::*;
use crate::{data_dir, Error, Result};

/// New session (setsid) for a spawned child: it leaves tuxgt's process
/// group, so terminal signals to tuxgt no longer reach it, and it survives
/// our exit via reparent instead of dying with the group. Tuxgt is still
/// the parent until then — plain fork/exec, no double-fork. Stdio policy
/// stays with the caller.
pub(crate) fn detach_command(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setsid();
            Ok(())
        });
    }
}

pub(crate) fn proton_runner(
    row: &Row,
    exe: &Path,
    prefix: Option<&Path>,
    proton: Option<&str>,
    paths: &LaunchPaths,
    env: &mut BTreeMap<String, String>,
) -> Result<(PathBuf, Vec<String>)> {
    let proton_script = proton.and_then(resolve_proton);
    let proton_dir = proton_script.as_ref().and_then(|p| p.parent());
    if let Some(pfx) = prefix {
        env.insert(
            "STEAM_COMPAT_DATA_PATH".into(),
            pfx.to_string_lossy().into_owned(),
        );
    }
    if let Some(dir) = row.install_dir.as_deref() {
        env.insert("STEAM_COMPAT_INSTALL_PATH".into(), dir.to_string());
    }
    let app = if row.manager == "steam" {
        row.game_id.as_str()
    } else {
        "0"
    };
    env.insert("STEAM_COMPAT_APP_ID".into(), app.to_string());
    let gameid = if row.manager == "steam" {
        row.game_id.clone()
    } else {
        "umu-default".into()
    };
    env.insert("GAMEID".into(), gameid);
    if let Some(root) = steam_client_root() {
        env.insert("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), root);
    }
    if let Some(dir) = proton_dir {
        let d = dir.to_string_lossy().into_owned();
        env.insert("PROTONPATH".into(), d.clone());
        env.insert("STEAM_COMPAT_TOOL_PATHS".into(), d);
    }

    if let Some(umu) = &paths.umu {
        let mut args = Vec::new();
        args.push(exe.to_string_lossy().into_owned());
        return Ok((umu.clone(), args));
    }
    if let Some(script) = proton_script {
        let args = vec![
            "waitforexitandrun".into(),
            exe.to_string_lossy().into_owned(),
        ];
        return Ok((script, args));
    }
    Err(Error::MissingRunner(row.id.clone()))
}

pub(crate) fn wine_runner(
    id: &str,
    exe: &Path,
    prefix: Option<&Path>,
    proton: Option<&str>,
    paths: &LaunchPaths,
    env: &mut BTreeMap<String, String>,
) -> Result<(PathBuf, Vec<String>)> {
    if let Some(pfx) = prefix {
        env.insert("WINEPREFIX".into(), pfx.to_string_lossy().into_owned());
    }
    let bin = proton
        .and_then(|p| {
            let path = PathBuf::from(p);
            path.is_file().then_some(path)
        })
        .or_else(|| paths.wine.clone())
        .ok_or_else(|| Error::MissingRunner(id.into()))?;
    Ok((bin, vec![exe.to_string_lossy().into_owned()]))
}

pub(crate) fn parse_launch_options(
    raw: Option<&str>,
) -> (Vec<String>, Vec<String>, Vec<(String, String)>) {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    let (left, right) = match s.split_once("%command%") {
        Some((l, r)) => (Some(l), r),
        None => (None, s),
    };
    let extra = tokenize(right);
    let mut wrap = Vec::new();
    let mut env = Vec::new();
    if let Some(l) = left {
        for tok in tokenize(l) {
            if let Some((k, v)) = env_assign(&tok) {
                env.push((k, v));
            } else {
                wrap.push(tok);
            }
        }
    }
    (wrap, extra, env)
}

pub(crate) fn parse_heroic_wrappers(raw: Option<&str>) -> Vec<Vec<String>> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    s.split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(tokenize)
        .filter(|v| !v.is_empty())
        .collect()
}

pub(crate) fn env_assign(tok: &str) -> Option<(String, String)> {
    let (k, v) = tok.split_once('=')?;
    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if k.chars().next()?.is_ascii_digit() {
        return None;
    }
    Some((k.to_string(), v.to_string()))
}

pub(crate) fn parse_env(raw: &str) -> Result<BTreeMap<String, String>> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(BTreeMap::new());
    }
    let v: serde_json::Value = serde_json::from_str(t).map_err(|e| Error::BadEnv(e.to_string()))?;
    let obj = v.as_object().ok_or_else(|| Error::BadEnv(t.into()))?;
    let mut m = BTreeMap::new();
    for (k, val) in obj {
        let s = match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        m.insert(k.clone(), s);
    }
    Ok(m)
}

pub(crate) fn pick<'a>(
    ovr: &'a Option<String>,
    det: &'a Option<String>,
    store: &'a Option<String>,
) -> &'a str {
    ovr.as_deref()
        .or(det.as_deref())
        .or(store.as_deref())
        .unwrap_or("")
}

/// True when `token`'s file name is `want` (wrapper program dedupe).
pub(crate) fn basename_eq(token: &str, want: &str) -> bool {
    Path::new(token)
        .file_name()
        .is_some_and(|n| n == std::ffi::OsStr::new(want))
}

pub(crate) fn has_launcher(wrappers: &[Vec<String>], launcher: Option<&Path>) -> bool {
    let want = launcher.and_then(|p| p.file_name());
    wrappers.iter().flatten().any(|t| {
        let n = Path::new(t).file_name();
        n.is_some_and(|n| n == "tuxgt-launcher") || (want.is_some() && n == want)
    })
}

pub(crate) fn find_tuxgt_launcher() -> Result<PathBuf> {
    let data = data_dir();
    if data.join("bin/tuxgt-launcher").is_file() {
        return Ok(data.join("bin/tuxgt-launcher"));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("tuxgt-launcher");
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    find_in_path("tuxgt-launcher").ok_or(Error::MissingLauncher)
}

pub(crate) fn find_steam() -> Option<PathBuf> {
    if let Some(p) = find_in_path("steam") {
        return Some(p);
    }
    for root in steam_roots() {
        let sh = root.join("steam.sh");
        if sh.is_file() {
            return Some(sh);
        }
    }
    None
}

/// Native `heroic` binary only. Flatpak (`flatpak run com.heroicgameslauncher.hgl
/// <url>`) is a multi-arg program and does not fit `program + args` dispatch;
/// add it when someone needs it.
pub(crate) fn find_heroic() -> Option<PathBuf> {
    find_in_path("heroic")
}

pub(crate) fn find_in_path(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() && p.is_file() {
        return Some(p.to_path_buf());
    }
    for dir in env::split_paths(&env::var_os("PATH")?) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub(crate) fn resolve_proton(name: &str) -> Option<PathBuf> {
    resolve_proton_in(name, &steam_roots())
}

pub(crate) fn resolve_proton_in(name: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let p = PathBuf::from(name);
    if p.is_file() {
        return Some(p);
    }
    let nested = p.join("proton");
    if nested.is_file() {
        return Some(nested);
    }
    let want = name.to_ascii_lowercase();
    for root in roots {
        let ctd = root.join("compatibilitytools.d");
        let direct = ctd.join(name).join("proton");
        if direct.is_file() {
            return Some(direct);
        }
        if let Ok(rd) = std::fs::read_dir(&ctd) {
            for ent in rd.flatten() {
                let n = ent.file_name().to_string_lossy().to_ascii_lowercase();
                if n == want || n.replace(' ', "_") == want {
                    let cand = ent.path().join("proton");
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
        }
        let common = root.join("steamapps").join("common");
        if let Ok(rd) = std::fs::read_dir(&common) {
            for ent in rd.flatten() {
                let n = ent.file_name().to_string_lossy().to_ascii_lowercase();
                if proton_dir_matches(&n, &want) {
                    let cand = ent.path().join("proton");
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn proton_dir_matches(dir: &str, want: &str) -> bool {
    if dir == want || dir.replace(' ', "_") == want {
        return true;
    }
    if want == "proton_experimental" {
        return dir.contains("proton") && dir.contains("experimental");
    }
    if want == "proton_hotfix" {
        return dir.contains("proton") && dir.contains("hotfix");
    }
    if let Some(rest) = want.strip_prefix("proton_") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            let needle = official_proton_ver(rest);
            return dir.starts_with("proton") && dir.contains(&needle);
        }
        return dir.starts_with("proton") && dir.contains(rest);
    }
    if let Some(ver) = version_file_ver(want) {
        return dir.starts_with("proton") && dir.contains(&ver);
    }
    false
}

/// CompatToolMapping `proton_90` → folder `Proton 9.0`; `proton_10` → `Proton 10`.
pub(crate) fn official_proton_ver(digits: &str) -> String {
    match digits.len() {
        2 if digits.starts_with('1') => format!("{digits}."),
        2 => format!("{}.{}", &digits[0..1], &digits[1..2]),
        _ => format!("{digits}."),
    }
}

pub(crate) fn version_file_ver(s: &str) -> Option<String> {
    let head = s.split('-').next()?;
    let mut parts = head.split('.');
    let a = parts.next()?;
    let b = parts.next()?;
    if !a.is_empty()
        && !b.is_empty()
        && a.chars().all(|c| c.is_ascii_digit())
        && b.chars().all(|c| c.is_ascii_digit())
    {
        Some(format!("{a}.{b}"))
    } else {
        None
    }
}

pub(crate) fn steam_roots() -> Vec<PathBuf> {
    match steamlocate::locate_all() {
        Ok(dirs) => dirs.iter().map(|d| d.path().to_path_buf()).collect(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn steam_client_root() -> Option<String> {
    steam_roots()
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().into_owned())
}

pub(crate) fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), ch) if ch == q => quote = None,
            (Some(_), ch) => cur.push(ch),
            (None, '"' | '\'') => quote = Some(c),
            (None, ch) if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, ch) => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(crate) fn quote_arg(s: &str) -> String {
    if s.is_empty()
        || s.chars().any(|c| {
            !(c.is_ascii_alphanumeric()
                || matches!(c, '/' | '_' | '.' | ',' | ':' | '=' | '+' | '-'))
        })
    {
        let mut out = String::from("'");
        for c in s.chars() {
            if c == '\'' {
                out.push_str("'\\''");
            } else {
                out.push(c);
            }
        }
        out.push('\'');
        out
    } else {
        s.to_string()
    }
}
