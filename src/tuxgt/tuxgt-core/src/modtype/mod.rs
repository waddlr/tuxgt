use std::fmt;

use crate::{Error, Result};

/// Preload proxy slot. Closed set; stock-named and ASI-install loads claim none.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ProxySlot {
    Dxgi,
    D3d11,
    D3d12,
    Winmm,
    Version,
}

impl ProxySlot {
    pub fn as_str(self) -> &'static str {
        match self {
            ProxySlot::Dxgi => "dxgi",
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
        Some(ProxySlot::Dxgi)
    }
    fn requires(&self) -> Option<&'static [&'static str]> {
        None
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

/// Per-file type dest default (no zero/two check). OptiScaler.dll → dxgi.dll.
pub fn type_dest_for(mod_type: &str, src: &str) -> String {
    if mod_type == "optiscaler" && is_optiscaler_dll(src) {
        "dxgi.dll".into()
    } else {
        src.to_string()
    }
}

/// OptiScaler: basename `OptiScaler.dll` → `dxgi.dll`. Other types leave dests unchanged.
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
        1 => {
            if !skip.get(opti[0]).copied().unwrap_or(false) {
                dests[opti[0]] = "dxgi.dll".into();
            }
            Ok(())
        }
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
