use std::path::{Path, PathBuf};

use super::binary::{apis_from_libs, parse_binary, split_default_extra};
use super::{DetectCtx, Detected, Detector};

pub struct Unity;

impl Detector for Unity {
    fn id(&self) -> &'static str {
        "unity"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(dir) = ctx.install_dir else {
            return;
        };
        let data = find_data_dir(dir);
        if data.is_some()
            || dir.join("UnityPlayer.dll").is_file()
            || dir.join("UnityPlayer.so").is_file()
        {
            out.set_engine("unity", self.id());
        }
        if out.exe_path.is_none() {
            if let Some(exe) = exe_for_data(dir, data.as_deref()) {
                out.set_exe(exe, self.id());
            }
        }
        if out.build.is_none() {
            if let Some(data) = data {
                if let Some(b) = unity_version(&data) {
                    out.set_build(b, self.id());
                }
            }
        }
        if let Some(bin) = ctx.binary {
            if bin.libs.iter().any(|l| l.contains("unityplayer")) {
                out.set_engine("unity", self.id());
            }
        }
        if out.engine.as_deref() == Some("unity") {
            if let Some(player) = ["UnityPlayer.dll", "UnityPlayer.so"]
                .into_iter()
                .map(|n| dir.join(n))
                .find(|p| p.is_file())
            {
                if let Some(bin) = parse_binary(&player) {
                    let (api, extra) = split_default_extra(&apis_from_libs(&bin.libs));
                    if let Some(api) = api {
                        out.set_api(api, self.id());
                    }
                    if let Some(extra) = extra {
                        out.set_extra_apis(extra, self.id());
                    }
                }
            }
        }
    }
}

fn find_data_dir(dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            let name = p.file_name()?.to_string_lossy();
            if name.ends_with("_Data") {
                return Some(p);
            }
        }
    }
    None
}

fn exe_for_data(dir: &Path, data: Option<&Path>) -> Option<PathBuf> {
    if let Some(data) = data {
        let stem = data
            .file_name()?
            .to_string_lossy()
            .strip_suffix("_Data")?
            .to_string();
        for ext in [".exe", ".x86_64", ".x86", ""] {
            let p = dir.join(format!("{stem}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for name in ["UnityPlayer.dll", "GameAssembly.dll"] {
        if dir.join(name).is_file() {
            if let Some(exe) = first_exe(dir) {
                return Some(exe);
            }
        }
    }
    None
}

fn first_exe(dir: &Path) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for ent in rd.flatten() {
        let p = ent.path();
        if super::is_exe_file(&p) && !super::is_helper_exe(&p) {
            return Some(p);
        }
    }
    None
}

fn unity_version(data: &Path) -> Option<String> {
    let p = data.join("globalgamemanagers");
    let bytes = std::fs::read(p).ok()?;
    let n = bytes.len().min(64);
    let s = String::from_utf8_lossy(&bytes[..n]);
    let ver: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == 'f' || *c == 'p')
        .collect();
    if ver.chars().any(|c| c.is_ascii_digit()) && ver.contains('.') {
        Some(ver)
    } else {
        None
    }
}
