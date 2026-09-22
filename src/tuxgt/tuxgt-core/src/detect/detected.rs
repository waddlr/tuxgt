use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::game::GameId;

pub trait Detector: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, ctx: &DetectCtx, out: &mut Detected);
}

pub struct DetectCtx<'a> {
    pub id: &'a GameId,
    pub install_dir: Option<&'a Path>,
    pub binary: Option<&'a BinaryInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct Detected {
    pub exe_path: Option<PathBuf>,
    pub platform: Option<String>,
    pub bitness: Option<String>,
    pub api: Option<String>,
    pub extra_apis: Option<String>,
    pub engine: Option<String>,
    pub prefix_path: Option<PathBuf>,
    pub proton: Option<String>,
    pub build: Option<String>,
    pub exe_version: Option<String>,
    pub(crate) sources: BTreeMap<&'static str, &'static str>,
}

impl Detected {
    pub(crate) fn owner(field: &str) -> &'static [&'static str] {
        match field {
            "bitness" | "exe_version" => &["pe"],
            "api" | "extra_apis" => &["unreal", "unity", "pe"],
            "platform" => &["pe", "runtime"],
            "engine" => &[
                "unreal",
                "unity",
                "re-engine",
                "creation",
                "blackspace",
                "mo2",
            ],
            "exe_path" => &[
                "mo2",
                "unreal",
                "unity",
                "re-engine",
                "creation",
                "blackspace",
                "runtime",
            ],
            "prefix_path" | "proton" => &["runtime"],
            "build" => &["unreal", "unity", "pe", "runtime"],
            _ => &[],
        }
    }

    pub(crate) fn can_overwrite(field: &str, current_src: Option<&str>, src: &str) -> bool {
        let owners = Self::owner(field);
        let cur_i = current_src.and_then(|c| owners.iter().position(|o| *o == c));
        let new_i = owners.iter().position(|o| *o == src);
        match (cur_i, new_i) {
            (None, Some(_)) => true,
            (Some(c), Some(n)) => n <= c,
            _ => false,
        }
    }

    pub(crate) fn set_str(
        &mut self,
        field: &'static str,
        value: String,
        src: &'static str,
    ) -> Option<String> {
        if value.is_empty() {
            return None;
        }
        let current_src = self.sources.get(field).copied();
        let accept = match self.slot(field) {
            None => true,
            Some(old) if old == value => false,
            Some(old) => {
                if Self::can_overwrite(field, current_src, src) {
                    true
                } else {
                    tracing::warn!(
                        field,
                        current = %old,
                        incoming = %value,
                        from = src,
                        "detector field conflict"
                    );
                    false
                }
            }
        };
        if accept {
            self.sources.insert(field, src);
            Some(value)
        } else {
            None
        }
    }

    pub(crate) fn slot(&self, field: &str) -> Option<String> {
        match field {
            "engine" => self.engine.clone(),
            "api" => self.api.clone(),
            "extra_apis" => self.extra_apis.clone(),
            "bitness" => self.bitness.clone(),
            "platform" => self.platform.clone(),
            "proton" => self.proton.clone(),
            "build" => self.build.clone(),
            "exe_version" => self.exe_version.clone(),
            "exe_path" => self
                .exe_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            "prefix_path" => self
                .prefix_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    pub(crate) fn set_engine(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("engine", v.into(), src) {
            self.engine = Some(v);
        }
    }
    pub(crate) fn set_api(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("api", v.into(), src) {
            self.api = Some(v);
        }
    }
    pub(crate) fn set_extra_apis(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("extra_apis", v.into(), src) {
            self.extra_apis = Some(v);
        }
    }
    pub(crate) fn set_bitness(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("bitness", v.into(), src) {
            self.bitness = Some(v);
        }
    }
    pub(crate) fn set_platform(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("platform", v.into(), src) {
            self.platform = Some(v);
        }
    }
    pub(crate) fn set_build(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("build", v.into(), src) {
            self.build = Some(v);
        }
    }
    pub(crate) fn set_exe_version(&mut self, v: impl Into<String>, src: &'static str) {
        if let Some(v) = self.set_str("exe_version", v.into(), src) {
            self.exe_version = Some(v);
        }
    }
    pub(crate) fn set_exe(&mut self, p: PathBuf, src: &'static str) {
        if self
            .set_str("exe_path", p.to_string_lossy().into_owned(), src)
            .is_some()
        {
            self.exe_path = Some(p);
        }
    }
}
