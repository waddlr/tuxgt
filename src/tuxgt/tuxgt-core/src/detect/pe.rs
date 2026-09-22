use super::binary::{apis_from_libs, split_default_extra, BinaryKind};
use super::{DetectCtx, Detected, Detector};

pub struct Pe;

impl Detector for Pe {
    fn id(&self) -> &'static str {
        "pe"
    }

    fn detect(&self, ctx: &DetectCtx, out: &mut Detected) {
        let Some(bin) = ctx.binary else {
            return;
        };
        out.set_bitness(bin.bitness.to_string(), self.id());
        let mut apis = apis_from_libs(&bin.libs);
        if out.engine.as_deref() == Some("unreal") && apis.iter().any(|a| *a != "opengl") {
            apis.retain(|a| *a != "opengl");
        }
        let (api, extra) = split_default_extra(&apis);
        if let Some(api) = api {
            out.set_api(api, self.id());
        }
        if let Some(extra) = extra {
            out.set_extra_apis(extra, self.id());
        }
        if let Some(v) = &bin.file_version {
            out.set_exe_version(v.clone(), self.id());
        }
        match bin.kind {
            BinaryKind::Elf => {
                out.set_platform("native", self.id());
                if out.api.is_none() {
                    if let Some(dir) = bin.path.parent() {
                        if dir.join("vulkaninfo").is_file()
                            || dir.join("libvulkan.so.1").is_file()
                            || dir.join("libvulkan.so").is_file()
                        {
                            out.set_api("vulkan", self.id());
                        }
                    }
                }
            }
            BinaryKind::Pe => {}
        }
    }
}
