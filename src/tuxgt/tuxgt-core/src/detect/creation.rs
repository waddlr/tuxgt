use super::{DetectCtx, Detected, Detector};

pub struct Creation;

impl Detector for Creation {
    fn id(&self) -> &'static str {
        "creation"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let dir = ctx.install_dir;
        let exe_name = out
            .exe_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_ascii_lowercase());
        let named = exe_name
            .as_deref()
            .map(|n| {
                matches!(
                    n,
                    "fallout4.exe"
                        | "fallout4vr.exe"
                        | "skyrimse.exe"
                        | "skyrimvr.exe"
                        | "tesv.exe"
                )
            })
            .unwrap_or(false);
        let data = dir
            .map(|d| {
                d.join("Data/Fallout4.esm").is_file()
                    || d.join("Data/Skyrim.esm").is_file()
                    || d.join("Data").join("Fallout4.esm").is_file()
            })
            .unwrap_or(false);
        if named || data {
            out.set_engine("creation", self.id());
        }
        if out.exe_path.is_none() {
            if let Some(dir) = dir {
                for name in ["Fallout4.exe", "SkyrimSE.exe", "SkyrimVR.exe", "TESV.exe"] {
                    let p = dir.join(name);
                    if p.is_file() {
                        out.set_exe(p, self.id());
                        break;
                    }
                }
            }
        }
    }
}
