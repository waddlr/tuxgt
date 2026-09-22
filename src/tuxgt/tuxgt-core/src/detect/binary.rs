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
        Ok(goblin::Object::PE(pe)) => Some(info_from_pe(path, &bytes, pe)),
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
            parse_pe_lenient(path, &bytes).or_else(|| pe_header_fallback(path, &bytes))
        }
    }
}

fn info_from_pe(path: &Path, bytes: &[u8], pe: goblin::pe::PE<'_>) -> BinaryInfo {
    let mut libs: BTreeSet<String> = pe
        .libraries
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    libs.extend(super::delay::delay_libs(bytes, &pe));
    let file_version = pe.resource_data.and_then(|r| r.version_info).and_then(|v| {
        v.string_info
            .file_version()
            .or_else(|| v.string_info.product_version())
    });
    BinaryInfo {
        path: path.to_path_buf(),
        kind: BinaryKind::Pe,
        bitness: if pe.is_64 { 64 } else { 32 },
        libs,
        file_version,
    }
}

/// Skip the cert table and TLS under Permissive so a cert-past-EOF parse
/// can still return imports and bitness.
fn parse_pe_lenient(path: &Path, bytes: &[u8]) -> Option<BinaryInfo> {
    let mut opts = goblin::pe::options::ParseOptions::default();
    opts.parse_attribute_certificates = false;
    opts.parse_tls_data = false;
    opts.parse_mode = goblin::pe::options::ParseMode::Permissive;
    let pe = goblin::pe::PE::parse_with_opts(bytes, &opts).ok()?;
    Some(info_from_pe(path, bytes, pe))
}

/// COFF Machine when even a lenient parse fails (bad version resources).
pub(crate) fn pe_header_bitness(bytes: &[u8]) -> Option<u8> {
    if bytes.len() < 0x40 || bytes[0] != b'M' || bytes[1] != b'Z' {
        return None;
    }
    let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().ok()?) as usize;
    if bytes.len() < pe.saturating_add(6) || &bytes[pe..pe + 4] != b"PE\0\0" {
        return None;
    }
    let machine = u16::from_le_bytes(bytes[pe + 4..pe + 6].try_into().ok()?);
    match machine {
        0x14c => Some(32),
        0x8664 | 0xaa64 => Some(64),
        _ => None,
    }
}

fn pe_header_fallback(path: &Path, bytes: &[u8]) -> Option<BinaryInfo> {
    let bitness = pe_header_bitness(bytes)?;
    Some(BinaryInfo {
        path: path.to_path_buf(),
        kind: BinaryKind::Pe,
        bitness,
        libs: BTreeSet::new(),
        file_version: None,
    })
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
