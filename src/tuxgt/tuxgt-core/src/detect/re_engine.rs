use std::path::{Path, PathBuf};

use super::{DetectCtx, Detected, Detector};

pub struct ReEngine;

impl Detector for ReEngine {
    fn id(&self) -> &'static str {
        "re-engine"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(dir) = ctx.install_dir else {
            return;
        };
        if !looks_re(dir) {
            return;
        }
        out.set_engine("re_engine", self.id());
        if out.exe_path.is_none() {
            if let Some(exe) = first_exe(dir) {
                out.set_exe(exe, self.id());
            }
        }
    }
}

fn looks_re(dir: &Path) -> bool {
    if dir.join("re_chunk_000.pak").is_file() {
        return true;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            let n = name.to_string_lossy();
            if n.starts_with("re_chunk_000.pak") {
                return true;
            }
        }
    }
    let natives = dir.join("natives");
    natives.is_dir() && has_pak(dir)
}

fn has_pak(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        e.path()
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("pak"))
            .unwrap_or(false)
    })
}

fn first_exe(dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut named = None;
    for ent in rd.flatten() {
        let p = ent.path();
        let name = p.file_name()?.to_string_lossy().to_ascii_lowercase();
        if !p.is_file() || !name.ends_with(".exe") {
            continue;
        }
        if name.starts_with("unins") || name.contains("uninstall") || name.contains("crash") {
            continue;
        }
        if name.contains("re") || name.contains("dd2") || name.contains("mhr") {
            return Some(p);
        }
        if named.is_none() {
            named = Some(p);
        }
    }
    named
}
