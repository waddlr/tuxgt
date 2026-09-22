use std::fmt;
use std::path::PathBuf;

use crate::{Error, Result};

/// Preload proxy slot. Closed set; stock-named and ASI-install loads claim none.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ProxySlot {
    Dxgi,
    D3d9,
    D3d10,
    D3d11,
    D3d12,
    Winmm,
    Version,
}

impl ProxySlot {
    pub fn as_str(self) -> &'static str {
        match self {
            ProxySlot::Dxgi => "dxgi",
            ProxySlot::D3d9 => "d3d9",
            ProxySlot::D3d10 => "d3d10",
            ProxySlot::D3d11 => "d3d11",
            ProxySlot::D3d12 => "d3d12",
            ProxySlot::Winmm => "winmm",
            ProxySlot::Version => "version",
        }
    }
}

impl fmt::Display for ProxySlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn parse_slot(s: &str) -> Result<ProxySlot> {
    let lower = s.to_ascii_lowercase();
    let stem = lower.strip_suffix(".dll").unwrap_or(&lower);
    match stem {
        "dxgi" => Ok(ProxySlot::Dxgi),
        "d3d9" => Ok(ProxySlot::D3d9),
        "d3d10" => Ok(ProxySlot::D3d10),
        "d3d11" => Ok(ProxySlot::D3d11),
        "d3d12" => Ok(ProxySlot::D3d12),
        "winmm" => Ok(ProxySlot::Winmm),
        "version" => Ok(ProxySlot::Version),
        _ => Err(Error::InvalidSlot(s.into())),
    }
}

pub trait ModType: Send + Sync {
    fn mod_type(&self) -> &'static str;
    fn default_slot(&self) -> Option<ProxySlot>;
    fn requires(&self) -> Option<&'static [&'static str]>;
    /// Shared ReShade dir this type installs into, relative to the game root.
    /// `None` = dests keep their archive-relative path.
    fn dest_root(&self) -> Option<&'static str> {
        None
    }
    /// True when this type may rewrite `rel` while staging (R21 gate).
    /// Checked before any byte is read so binary payloads never pay for
    /// parsing; only an exact approved file returns true.
    fn staging_rewrite_applies(&self, _rel: &str) -> bool {
        false
    }
    /// Rewrite verified depot `src` bytes for the staged copy of `rel`
    /// (R21). `Ok(None)` stages bytes identical; `Ok(Some(out))` stages
    /// `out` with `staged_sha` over `out` and `tuxgt_modified` set.
    /// Deterministic and side-effect free; errors fail the sync with
    /// prior staged bytes and bookkeeping intact.
    fn staging_rewrite(
        &self,
        _ctx: &StagingRewriteContext,
        _rel: &str,
        _src: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

/// Per-game context for a type-owned staging rewrite (R21). Synchronous
/// only: game roots and prefixes live behind the sqlite pool and are
/// unavailable on the staging path, so rewrites build absolute values
/// from the runtime dir.
#[derive(Clone, Debug)]
pub struct StagingRewriteContext {
    pub game_id: String,
    pub instance_id: String,
    /// `<game>/runtime/` — the loader's `TUXGT_GAME_DIR` and home of
    /// generated files; per-game absolute values point here.
    pub runtime_dir: PathBuf,
}

struct ReshadeType;
struct OptiscalerType;
struct ReshadeAddonType;
struct CustomType;
struct EffectType;
struct TextureType;

static RESHADE_REQUIRES: &[&str] = &["reshade"];

impl ModType for ReshadeType {
    fn mod_type(&self) -> &'static str {
        "reshade"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        None
    }
}

impl ModType for OptiscalerType {
    fn mod_type(&self) -> &'static str {
        "optiscaler"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        None
    }
    fn staging_rewrite_applies(&self, rel: &str) -> bool {
        is_optiscaler_ini(rel)
    }
    fn staging_rewrite(
        &self,
        ctx: &StagingRewriteContext,
        rel: &str,
        src: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        rewrite_optiscaler_ini(ctx, rel, src)
    }
}

impl ModType for ReshadeAddonType {
    fn mod_type(&self) -> &'static str {
        "reshade_addon"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        Some(RESHADE_REQUIRES)
    }
}

impl ModType for CustomType {
    fn mod_type(&self) -> &'static str {
        "custom"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        None
    }
}

impl ModType for EffectType {
    fn mod_type(&self) -> &'static str {
        "effect"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        Some(RESHADE_REQUIRES)
    }
    fn dest_root(&self) -> Option<&'static str> {
        Some("reshade-shaders/Shaders")
    }
}

impl ModType for TextureType {
    fn mod_type(&self) -> &'static str {
        "texture"
    }
    fn default_slot(&self) -> Option<ProxySlot> {
        None
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        Some(RESHADE_REQUIRES)
    }
    fn dest_root(&self) -> Option<&'static str> {
        Some("reshade-shaders/Textures")
    }
}

pub static MOD_TYPES: &[&dyn ModType] = &[
    &ReshadeType,
    &OptiscalerType,
    &ReshadeAddonType,
    &CustomType,
    &EffectType,
    &TextureType,
];

/// Marker substituted with the absolute per-game runtime dir by the R21
/// OptiScaler rewrite. Exact case; any other text passes through untouched.
pub const TUXGT_RUNTIME_TOKEN: &str = "@TUXGT_RUNTIME@";

/// Basename gate for the one approved R21 rewrite file (ASCII
/// case-insensitive). Only the final component is compared, so `pfx:` and
/// subdir rels never match by accident.
fn is_optiscaler_ini(rel: &str) -> bool {
    rel.rsplit(['/', '\\'])
        .next()
        .is_some_and(|b| b.eq_ignore_ascii_case("OptiScaler.ini"))
}

/// R21 approved rewrite: in `OptiScaler.ini`, substitute
/// [`TUXGT_RUNTIME_TOKEN`] with the absolute per-game runtime dir inside
/// `[Log] LogFileName` values. Section and key match ASCII
/// case-insensitively; the token matches exact case. A missing section,
/// key, or token is not applicable (`Ok(None)`). Line endings, key
/// whitespace/casing, and trailing comments are preserved; every matching
/// key line in every `[Log]` section is substituted. A UTF-8 BOM is
/// preserved; non-UTF-8 bytes are a typed manifest error (fail closed).
fn rewrite_optiscaler_ini(
    ctx: &StagingRewriteContext,
    rel: &str,
    src: &[u8],
) -> Result<Option<Vec<u8>>> {
    const BOM: &[u8] = b"\xef\xbb\xbf";
    let body = src.strip_prefix(BOM).unwrap_or(src);
    let text = std::str::from_utf8(body).map_err(|_| {
        Error::Manifest(format!(
            "{} {}: {rel} is not valid UTF-8",
            ctx.game_id, ctx.instance_id
        ))
    })?;
    // Wine-absolute: OptiScaler accepts forward slashes, so `Z:/...` needs
    // no backslash escaping inside the ini value.
    let abs = format!("Z:{}", ctx.runtime_dir.to_string_lossy().replace('\\', "/"));
    let mut in_log = false;
    let mut changed = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (core, term) = match line.strip_suffix("\r\n") {
            Some(core) => (core, "\r\n"),
            None => match line.strip_suffix('\n') {
                Some(core) => (core, "\n"),
                None => (line, ""),
            },
        };
        if let Some(rest) = core.trim().strip_prefix('[') {
            if let Some((name, _)) = rest.split_once(']') {
                in_log = name.trim().eq_ignore_ascii_case("log");
            }
        }
        if in_log {
            if let Some((key, value)) = core.split_once('=') {
                if key.trim().eq_ignore_ascii_case("logfilename")
                    && value.contains(TUXGT_RUNTIME_TOKEN)
                {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(&value.replace(TUXGT_RUNTIME_TOKEN, &abs));
                    out.push_str(term);
                    changed = true;
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    if !changed {
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(src.len() + 64);
    if body.len() != src.len() {
        bytes.extend_from_slice(BOM);
    }
    bytes.extend_from_slice(out.as_bytes());
    Ok(Some(bytes))
}

pub fn parse_mod_type(name: &str) -> Result<&'static dyn ModType> {
    for t in MOD_TYPES {
        if t.mod_type() == name {
            return Ok(*t);
        }
    }
    Err(Error::InvalidModType(name.into()))
}

/// `Some(rest)` when `name` is `rel`'s first path component (ASCII
/// case-insensitive), so only a whole component is ever stripped.
fn strip_component<'a>(rel: &'a str, name: &str) -> Option<&'a str> {
    let (head, rest) = rel.split_once('/')?;
    head.eq_ignore_ascii_case(name).then_some(rest)
}

/// Final dest for an archive-relative path under a type's `dest_root`.
/// `None` root returns `rel` unchanged. Otherwise a leading `reshade-shaders/`
/// is stripped once, then a leading `Shaders/` or `Textures/` maps to the
/// matching shared ReShade dir (packs that ship both roots keep them apart);
/// anything else is prefixed with `dest_root`. Recipe `shader_dir` /
/// `texture_dir` (E96) insert one extra component after that shared dir.
pub fn dest_for(
    dest_root: Option<&str>,
    rel: &str,
    shader_dir: Option<&str>,
    texture_dir: Option<&str>,
) -> String {
    let Some(root) = dest_root else {
        return rel.to_string();
    };
    if rel
        .rsplit('/')
        .next()
        .unwrap_or(rel)
        .to_ascii_lowercase()
        .ends_with(".ini")
    {
        return rel.rsplit('/').next().unwrap_or(rel).to_string();
    }
    let rel = strip_component(rel, "reshade-shaders").unwrap_or(rel);
    let mut path = None;
    for known in ["Shaders", "Textures"] {
        if let Some(rest) = strip_component(rel, known) {
            path = Some(format!("reshade-shaders/{known}/{rest}"));
            break;
        }
    }
    let path = path.unwrap_or_else(|| format!("{root}/{rel}"));
    for (known, extra) in [("Shaders", shader_dir), ("Textures", texture_dir)] {
        let Some(dir) = extra.filter(|s| !s.is_empty()) else {
            continue;
        };
        let prefix = format!("reshade-shaders/{known}/");
        if let Some(rest) = path.strip_prefix(&prefix) {
            return format!("reshade-shaders/{known}/{dir}/{rest}");
        }
    }
    path
}

pub fn is_optiscaler_dll(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .eq_ignore_ascii_case("OptiScaler.dll")
}
/// ReShade Addon payload extension, case-insensitive. `.addon32` is the
/// 32-bit ReShade addon, `.addon64` the 64-bit one, bare `.addon` the
/// architecture-neutral form ReShade itself accepts; all three are
/// injectable payloads and all three stage into a 32-bit game unchanged.
/// One predicate so the Add form, classification, the required-dest rule,
/// and harvest globs cannot drift apart.
pub fn is_addon(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".addon") || lower.ends_with(".addon32") || lower.ends_with(".addon64")
}

/// OptiScaler: exactly one `OptiScaler.dll` is required. The dest stays the
/// source name. Other types leave dests unchanged.
pub fn apply_type_dests(mod_type: &str, dests: &mut [String]) -> Result<()> {
    apply_type_dests_except(mod_type, dests, &[], &[])
}

/// Like [`apply_type_dests`], but skip indices already remapped by recipe `[dests]`.
/// `srcs` are archive-relative paths before remap (same order as `dests`).
/// Skipped OptiScaler.dll srcs still count toward the two-file error unless
/// their dests differ.
pub fn apply_type_dests_except(
    mod_type: &str,
    dests: &mut [String],
    skip: &[bool],
    srcs: &[String],
) -> Result<()> {
    if mod_type != "optiscaler" {
        return Ok(());
    }
    let opti: Vec<usize> = (0..dests.len())
        .filter(|&i| {
            let skipped = skip.get(i).copied().unwrap_or(false);
            if skipped {
                srcs.get(i).is_some_and(|s| is_optiscaler_dll(s))
            } else {
                is_optiscaler_dll(&dests[i])
            }
        })
        .collect();
    match opti.len() {
        1 => Ok(()),
        0 => Err(Error::InvalidInstance(
            "optiscaler: no OptiScaler.dll in payload".into(),
        )),
        _ => {
            let all_skipped = opti.iter().all(|&i| skip.get(i).copied().unwrap_or(false));
            if all_skipped {
                for (ai, &i) in opti.iter().enumerate() {
                    for &j in &opti[ai + 1..] {
                        if dests[i] == dests[j] {
                            return Err(Error::InvalidInstance(
                                "optiscaler: multiple OptiScaler.dll files in payload".into(),
                            ));
                        }
                    }
                }
                Ok(())
            } else {
                Err(Error::InvalidInstance(
                    "optiscaler: multiple OptiScaler.dll files in payload".into(),
                ))
            }
        }
    }
}

/// Normalized LoadDLL dest basename for a proxy slot pick (E91):
/// `winmm` or `winmm.dll` (any case) → `winmm.dll`. Unknown stems error.
pub fn slot_dll(slot: &str) -> Result<String> {
    Ok(format!("{}.dll", parse_slot(slot)?.as_str()))
}

/// One package in a graph. `slot` is explicit per package; never defaulted silently.
pub struct ModPackage<'a> {
    pub name: &'a str,
    pub type_: &'a str,
    pub slot: Option<ProxySlot>,
    pub requires: &'a [&'a str],
    /// Other Mod ids (names) this package needs installed (E85). Unlike
    /// `requires`, these match package names, not types.
    pub requires_mods: &'a [&'a str],
}

#[derive(Debug, Default)]
pub struct Diagnosis {
    pub missing_requires: Vec<String>,
    pub slot_conflicts: Vec<String>,
}

/// Report-only diagnostics. Never enables a dep, never picks a slot.
/// Required types are the union of the package's own `requires` and its
/// type-level `requires()` (e.g. any `reshade_addon` package needs `reshade`
/// even when its own list is empty). Required Mods (`requires_mods`, E85)
/// match package names instead.
pub fn diagnose(packages: &[ModPackage<'_>]) -> Result<Diagnosis> {
    for p in packages {
        parse_mod_type(p.type_)?;
        for r in p.requires {
            parse_mod_type(r)?;
        }
    }
    let mut out = Diagnosis::default();
    for p in packages {
        let type_reqs = parse_mod_type(p.type_)?.requires().unwrap_or(&[]);
        for r in p.requires.iter().chain(type_reqs.iter()) {
            if !packages.iter().any(|q| q.type_ == *r) {
                let line = format!("{} requires missing {r}", p.name);
                if !out.missing_requires.contains(&line) {
                    out.missing_requires.push(line);
                }
            }
        }
        for r in p.requires_mods {
            if !packages.iter().any(|q| q.name == *r) {
                let line = format!("{} requires missing {r}", p.name);
                if !out.missing_requires.contains(&line) {
                    out.missing_requires.push(line);
                }
            }
        }
    }
    for (i, a) in packages.iter().enumerate() {
        let Some(slot) = a.slot else { continue };
        for b in &packages[i + 1..] {
            if b.slot == Some(slot) {
                out.slot_conflicts
                    .push(format!("slot {slot}: {} vs {}", a.name, b.name));
            }
        }
    }
    Ok(out)
}

/// Fixture graph: reshade + optiscaler(dxgi) + example addon + custom dll(dxgi).
/// Requires satisfied; one dxgi conflict. Model only.
pub fn fixture_packages() -> Vec<ModPackage<'static>> {
    vec![
        ModPackage {
            name: "reshade",
            type_: "reshade",
            slot: None,
            requires: &[],
            requires_mods: &[],
        },
        ModPackage {
            name: "optiscaler",
            type_: "optiscaler",
            slot: Some(ProxySlot::Dxgi),
            requires: &[],
            requires_mods: &[],
        },
        ModPackage {
            name: "example-addon",
            type_: "reshade_addon",
            slot: None,
            requires: RESHADE_REQUIRES,
            requires_mods: &[],
        },
        ModPackage {
            name: "custom-dll",
            type_: "custom",
            slot: Some(ProxySlot::Dxgi),
            requires: &[],
            requires_mods: &[],
        },
    ]
}

#[cfg(test)]
mod tests_0;
#[cfg(test)]
mod tests_1;
