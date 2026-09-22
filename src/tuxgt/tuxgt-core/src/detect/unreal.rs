use std::path::{Path, PathBuf};

use super::{DetectCtx, Detected, Detector};

pub struct Unreal;

impl Detector for Unreal {
    fn id(&self) -> &'static str {
        "unreal"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(dir) = ctx.install_dir else {
            return;
        };
        if looks_unreal(dir) {
            out.set_engine("unreal", self.id());
        }
        if out.exe_path.is_none() {
            if let Some(exe) = find_shipping(dir) {
                out.set_exe(exe, self.id());
            }
        }
        if out.build.is_none() {
            if let Some(b) = read_build_version(dir) {
                out.set_build(b, self.id());
            }
        }
        if looks_unreal(dir) || out.engine.as_deref() == Some("unreal") {
            if let Some(exe) = out.exe_path.as_deref() {
                if has_d3d12_agility(exe) {
                    out.set_api("dx12", self.id());
                    out.set_extra_apis("dx11", self.id());
                }
            }
        }
    }
}

fn has_d3d12_agility(exe: &Path) -> bool {
    let Some(parent) = exe.parent() else {
        return false;
    };
    parent.join("D3D12").join("D3D12Core.dll").is_file() || parent.join("D3D12Core.dll").is_file()
}

pub fn looks_unreal(dir: &Path) -> bool {
    dir.join("Engine/Build/Build.version").is_file()
        || dir
            .join("Engine")
            .join("Build")
            .join("Build.version")
            .is_file()
        || find_shipping(dir).is_some()
        || dir.join("Binaries").join("Win64").is_dir()
        || dir.join("Binaries").join("Win32").is_dir()
}

fn find_shipping(dir: &Path) -> Option<PathBuf> {
    let mut dirs = vec![
        dir.join("Binaries/Win64"),
        dir.join("Binaries/Win32"),
        dir.join("Binaries/WinGDK"),
    ];
    for depth0 in read_dirs(dir) {
        let binaries = depth0.join("Binaries");
        for win in ["Win64", "Win32", "WinGDK"] {
            dirs.push(binaries.join(win));
        }
    }
    let mut fallback = None;
    for bin in dirs {
        match shipping_in(&bin) {
            (Some(exe), _) => return Some(exe),
            (None, Some(other)) if fallback.is_none() => fallback = Some(other),
            _ => {}
        }
    }
    fallback
}

fn shipping_in(dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (None, None);
    };
    let mut fallback = None;
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name() else {
            continue;
        };
        let lower = name.to_string_lossy().to_ascii_lowercase();
        if !lower.ends_with(".exe") {
            continue;
        }
        if lower.contains("shipping") {
            return (Some(p), None);
        }
        if lower.contains("crash") || lower.contains("cef") || lower.contains("editor") {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(p);
        }
    }
    (None, fallback)
}

fn read_build_version(dir: &Path) -> Option<String> {
    let p = dir.join("Engine/Build/Build.version");
    let p = if p.is_file() {
        p
    } else {
        dir.join("Engine").join("Build").join("Build.version")
    };
    let text = std::fs::read_to_string(p).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let major = v.get("MajorVersion")?.as_u64()?;
    let minor = v.get("MinorVersion")?.as_u64()?;
    let patch = v.get("PatchVersion").and_then(|x| x.as_u64()).unwrap_or(0);
    let ch = v.get("Changelist").and_then(|x| x.as_u64());
    Some(match ch {
        Some(c) => format!("{major}.{minor}.{patch}-{c}"),
        None => format!("{major}.{minor}.{patch}"),
    })
}

fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}
