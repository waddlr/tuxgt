use std::path::{Path, PathBuf};

use super::{DetectCtx, Detected, Detector};

pub struct Mo2;

impl Detector for Mo2 {
    fn id(&self) -> &'static str {
        "mo2"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(dir) = ctx.install_dir else {
            return;
        };
        let ini = dir.join("ModOrganizer.ini");
        if !ini.is_file() {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&ini) else {
            return;
        };
        let game_name = ini_key(&text, "gameName");
        let game_path = ini_key(&text, "gamePath").map(|s| wine_to_unix(&unquote_qt(&s)));
        if let Some(p) = game_path.filter(|p| p.is_dir() || p.is_file()) {
            let dir = if p.is_file() {
                p.parent().unwrap_or(&p).to_path_buf()
            } else {
                p
            };
            if out.exe_path.is_none() {
                if let Some(exe) = pick_binary(&text, &dir) {
                    out.set_exe(exe, self.id());
                }
            }
        }
        if let Some(name) = game_name {
            let n = name.to_ascii_lowercase();
            if n.contains("skyrim special")
                || n.contains("skyrim vr")
                || n.contains("skyrim se")
                || n.contains("fallout 4")
                || n.contains("fallout4")
            {
                out.set_engine("creation", self.id());
            }
        }
    }
}

fn pick_binary(text: &str, game_dir: &Path) -> Option<PathBuf> {
    let mut bins: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((_, rest)) = line.split_once("\\binary=") else {
            continue;
        };
        let p = wine_to_unix(&unquote_qt(rest));
        if p.is_file() {
            bins.push(p);
        }
    }
    let prefer = |p: &Path| {
        let n = p
            .file_name()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        matches!(
            n.as_str(),
            "skyrimse.exe" | "skyrimvr.exe" | "fallout4.exe" | "fallout4vr.exe" | "tesv.exe"
        )
    };
    if let Some(p) = bins.iter().find(|p| prefer(p)) {
        return Some(p.clone());
    }
    if let Some(p) = bins.iter().find(|p| !mo2_tool(p)) {
        return Some(p.clone());
    }
    for name in ["SkyrimSE.exe", "Fallout4.exe", "TESV.exe"] {
        let p = game_dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn mo2_tool(p: &Path) -> bool {
    let n = p
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    n.contains("modorganizer")
        || n.contains("skse")
        || n.contains("f4se")
        || n.contains("xedit")
        || n.contains("tesedit")
        || n.contains("bodyslide")
        || n.contains("outfitstudio")
        || n.contains("dyndolod")
        || n.contains("texgen")
        || n.contains("creationkit")
        || n.contains("explorer")
        || n.contains("synthesis")
        || n.contains("pandora")
        || n.contains("easynpc")
        || n.contains("xlodgen")
        || n.contains("cathedral")
}

fn ini_key(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn unquote_qt(s: &str) -> String {
    let s = s.trim();
    if let Some(inner) = s
        .strip_prefix("@ByteArray(")
        .and_then(|r| r.strip_suffix(')'))
    {
        inner.replace("\\\\", "\\")
    } else {
        s.trim_matches('"').to_string()
    }
}

fn wine_to_unix(s: &str) -> PathBuf {
    let s = s.trim();
    let s = s
        .strip_prefix("Z:")
        .or_else(|| s.strip_prefix("z:"))
        .unwrap_or(s);
    PathBuf::from(s.replace('\\', "/"))
}
