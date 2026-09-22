use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryKind {
    Pe,
    Elf,
}

#[derive(Clone, Debug)]
pub struct BinaryInfo {
    pub path: PathBuf,
    pub kind: BinaryKind,
    pub bitness: u8,
    pub libs: BTreeSet<String>,
    pub file_version: Option<String>,
}

pub fn parse_binary(path: &Path) -> Option<BinaryInfo> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    if meta.len() > MAX_BYTES {
        tracing::warn!(path = %path.display(), size = meta.len(), "exe too large to parse");
        return None;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "exe unreadable");
            return None;
        }
    };
    match goblin::Object::parse(&bytes) {
        Ok(goblin::Object::PE(pe)) => {
            let libs = pe
                .libraries
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect();
            let file_version = pe.resource_data.and_then(|r| r.version_info).and_then(|v| {
                v.string_info
                    .file_version()
                    .or_else(|| v.string_info.product_version())
            });
            Some(BinaryInfo {
                path: path.to_path_buf(),
                kind: BinaryKind::Pe,
                bitness: if pe.is_64 { 64 } else { 32 },
                libs,
                file_version,
            })
        }
        Ok(goblin::Object::Elf(elf)) => {
            let libs = elf
                .libraries
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect();
            Some(BinaryInfo {
                path: path.to_path_buf(),
                kind: BinaryKind::Elf,
                bitness: if elf.is_64 { 64 } else { 32 },
                libs,
                file_version: None,
            })
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "binary parse failed");
            None
        }
    }
}

pub fn apis_from_libs(libs: &BTreeSet<String>) -> Vec<&'static str> {
    let has = |n: &str| libs.iter().any(|l| l == n || l.starts_with(n));
    let mut v = Vec::new();
    if has("d3d12.dll") {
        v.push("dx12");
    }
    if has("d3d11.dll") {
        v.push("dx11");
    }
    if has("d3d10.dll") {
        v.push("dx10");
    }
    if has("d3d9.dll") {
        v.push("dx9");
    }
    if has("vulkan-1.dll") || has("libvulkan.so") {
        v.push("vulkan");
    }
    let has_d3d = v.iter().any(|a| a.starts_with("dx"));
    if !has_d3d && (has("opengl32.dll") || has("libgl.so") || has("libopengl.so")) {
        v.push("opengl");
    }
    v
}

pub fn split_default_extra(apis: &[&str]) -> (Option<String>, Option<String>) {
    let mut it = apis.iter();
    let default = it.next().map(|s| (*s).to_string());
    let extra: Vec<&str> = it.copied().collect();
    let extra = if extra.is_empty() {
        None
    } else {
        Some(extra.join(","))
    };
    (default, extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn dx10_from_d3d10() {
        let mut libs = BTreeSet::new();
        libs.insert("d3d10.dll".into());
        assert_eq!(apis_from_libs(&libs), vec!["dx10"]);
    }
}
