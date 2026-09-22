use std::path::Path;

use super::{DetectCtx, Detected, Detector};

pub struct BlackSpace;

impl Detector for BlackSpace {
    fn id(&self) -> &'static str {
        "blackspace"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(dir) = ctx.install_dir else {
            return;
        };
        let bin64 = dir.join("bin64");
        let marks = bin64.join("cdt.dll").is_file() && bin64.join("cgraph.dll").is_file();
        if !marks {
            return;
        }
        out.set_engine("blackspace", self.id());
        if out.exe_path.is_none() {
            if let Some(exe) = first_exe(&bin64) {
                out.set_exe(exe, self.id());
            }
        }
    }
}

fn first_exe(dir: &Path) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(u64, std::path::PathBuf)> = None;
    for ent in rd.flatten() {
        let p = ent.path();
        if !super::is_exe_file(&p) || super::is_helper_exe(&p) {
            continue;
        }
        let sz = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        match best {
            Some((s, _)) if s >= sz => {}
            _ => best = Some((sz, p)),
        }
    }
    best.map(|(_, p)| p)
}
