//! Proxy-slot choice: `<self>` keeps the DLL's own name, and a proxy stem
//! already held by another enabled mod in the game is refused.

use std::collections::BTreeMap;
use std::path::Path;

use crate::{parse_slot, Error, FileManifest, PlannedFile, Result};

use super::claiming_slot_index;

/// Stock name. Not a [`crate::ProxySlot`]; the claiming dest keeps the
/// source basename.
pub const SELF_SLOT: &str = "<self>";

pub fn is_self_slot(slot: &str) -> bool {
    slot == SELF_SLOT
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

pub(crate) fn stock_basename(source: &str) -> String {
    basename(source).to_string()
}

/// The one DLL a slot renames. Companions (`amd_fidelityfx_dx12.dll`, the
/// rest of an OptiScaler pack) stay under their own names.
pub(crate) fn is_named_injector(dest: &str) -> bool {
    let base = basename(dest);
    crate::modtype::is_optiscaler_dll(base)
        || base.eq_ignore_ascii_case("ReShade64.dll")
        || base.eq_ignore_ascii_case("ReShade32.dll")
}

/// ReShade and OptiScaler always have one injector DLL. A custom pack is
/// configurable only when its recipe names a slot. NVIDIA Streamline is
/// custom with no slot: every DLL stays its own name.
pub fn recipe_slot_configurable(mod_type: &str, recipe_slot: Option<&str>) -> bool {
    match mod_type {
        "reshade" | "optiscaler" => true,
        "custom" => recipe_slot.is_some(),
        _ => false,
    }
}

pub(crate) fn slot_configurable(
    mod_type: &str,
    recipe_slot: Option<&str>,
    files: &[PlannedFile],
    include: &[String],
) -> bool {
    claiming_slot_index(files, include).is_some() && recipe_slot_configurable(mod_type, recipe_slot)
}

/// `slot` is `<self>` or a proxy stem. `<self>` becomes `stock`.
pub(crate) fn resolve_slot_dest(slot: &str, stock: &str) -> Result<String> {
    if is_self_slot(slot) {
        if stock.is_empty() || !stock.to_ascii_lowercase().ends_with(".dll") {
            return Err(Error::InvalidSlot(slot.into()));
        }
        return Ok(stock.to_string());
    }
    crate::modtype::slot_dll(slot)
}

/// Conversion asks only slot-configurable mods. Install asks all of them.
/// Preload asks those currently sitting on a proxy name, so a rename back
/// to `<self>` can default to yes and Streamline is never in the list.
pub(crate) fn conversion_needs_prompt(
    target_install: bool,
    mod_type: &str,
    recipe_slot: Option<&str>,
    files: &[PlannedFile],
    include: &[String],
) -> bool {
    if !slot_configurable(mod_type, recipe_slot, files, include) {
        return false;
    }
    if target_install {
        return true;
    }
    let Some(idx) = claiming_slot_index(files, include) else {
        return false;
    };
    parse_slot(basename(&files[idx].dest)).is_ok()
}

fn slot_holder<'a>(manifests: &'a [FileManifest], stem: &str, except: &str) -> Option<&'a str> {
    let want = parse_slot(stem).ok()?;
    for m in manifests {
        if !m.enabled || m.instance == except {
            continue;
        }
        let Some(idx) = claiming_slot_index(&m.files, &m.include) else {
            continue;
        };
        if parse_slot(basename(&m.files[idx].dest)).ok() == Some(want) {
            return Some(m.instance.as_str());
        }
    }
    None
}

/// Refuse `slot` when another enabled mod in the game already claims it.
/// `<self>` is not a shared slot.
pub(crate) fn ensure_slot_free(
    manifests: &[FileManifest],
    instance: &str,
    slot: &str,
) -> Result<()> {
    if is_self_slot(slot) {
        return Ok(());
    }
    let stem = parse_slot(slot)?.as_str();
    if let Some(holder) = slot_holder(manifests, stem, instance) {
        return Err(Error::SlotInUse {
            instance: instance.to_string(),
            slot: stem.to_string(),
            holder: holder.to_string(),
        });
    }
    Ok(())
}

/// Same check for a resolved dest (`ReShade64.dll` is free, `dxgi.dll` is not).
pub(crate) fn ensure_dest_free(
    manifests: &[FileManifest],
    instance: &str,
    dest: &str,
) -> Result<()> {
    let Ok(stem) = parse_slot(basename(dest)) else {
        return Ok(());
    };
    ensure_slot_free(manifests, instance, stem.as_str())
}

/// Prospective slots for a re-pick. `picks` is `(instance, slot)`.
/// Refuses when two enabled mods would share a proxy stem. Mods absent
/// from `picks` keep the stem they already claim. `<self>` is not shared.
pub fn ensure_repick_free(data_dir: &Path, game: &str, picks: &[(&str, &str)]) -> Result<()> {
    let manifests = crate::game_manifests(data_dir, game)?;
    let mut chosen: BTreeMap<&str, &str> = BTreeMap::new();
    for (inst, slot) in picks {
        chosen.insert(*inst, *slot);
    }
    let mut held: BTreeMap<String, String> = BTreeMap::new();
    for m in manifests.iter().filter(|m| m.enabled) {
        let stem = if let Some(slot) = chosen.get(m.instance.as_str()) {
            if is_self_slot(slot) {
                continue;
            }
            parse_slot(slot)?.as_str().to_string()
        } else {
            let Some(idx) = claiming_slot_index(&m.files, &m.include) else {
                continue;
            };
            let Ok(slot) = parse_slot(basename(&m.files[idx].dest)) else {
                continue;
            };
            slot.as_str().to_string()
        };
        if let Some(holder) = held.get(&stem) {
            return Err(Error::SlotInUse {
                instance: m.instance.clone(),
                slot: stem,
                holder: holder.clone(),
            });
        }
        held.insert(stem, m.instance.clone());
    }
    Ok(())
}

/// Instance ids to re-pick when two enabled mods share a proxy dest.
/// Empty when nothing is clobbered. Disabled mods do not hold a slot.
pub fn resync_repick_instances(
    data_dir: &Path,
    config_dir: &Path,
    game: &str,
) -> Result<Vec<String>> {
    let manifests = crate::game_manifests(data_dir, game)?;
    let mut held: BTreeMap<String, usize> = BTreeMap::new();
    for m in manifests.iter().filter(|m| m.enabled) {
        let Some(idx) = claiming_slot_index(&m.files, &m.include) else {
            continue;
        };
        let Ok(slot) = parse_slot(basename(&m.files[idx].dest)) else {
            continue;
        };
        *held.entry(slot.as_str().to_string()).or_default() += 1;
    }
    if !held.values().any(|n| *n > 1) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for m in &manifests {
        if !m.enabled {
            continue;
        }
        let recipe_slot = crate::find_mod(config_dir, data_dir, &m.instance)
            .ok()
            .and_then(|inst| inst.slot);
        if slot_configurable(&m.mod_type, recipe_slot.as_deref(), &m.files, &m.include) {
            out.push(m.instance.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str, dest: &str) -> PlannedFile {
        PlannedFile {
            source: source.into(),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
            load: None,
        }
    }

    #[test]
    fn self_resolves_to_the_source_basename() {
        assert_eq!(
            resolve_slot_dest(SELF_SLOT, "ReShade64.dll").unwrap(),
            "ReShade64.dll"
        );
        assert_eq!(
            resolve_slot_dest("dxgi", "ReShade64.dll").unwrap(),
            "dxgi.dll"
        );
        assert!(resolve_slot_dest("nope", "ReShade64.dll").is_err());
    }

    #[test]
    fn streamline_is_not_slot_configurable() {
        let include = ["sl.interposer.dll", "sl.common.dll", "nvngx_dlss.dll"].map(str::to_string);
        let files = vec![
            file("sl.interposer.dll", "sl.interposer.dll"),
            file("sl.common.dll", "sl.common.dll"),
            file("nvngx_dlss.dll", "nvngx_dlss.dll"),
        ];
        // include covers every DLL, so none of them claim a proxy slot.
        assert!(!recipe_slot_configurable("custom", None));
        assert!(!conversion_needs_prompt(
            true, "custom", None, &files, &include,
        ));
        assert!(conversion_needs_prompt(
            true,
            "reshade",
            Some("dxgi"),
            &[file("ReShade64.dll", "ReShade64.dll")],
            &[],
        ));
        assert!(!conversion_needs_prompt(
            false,
            "reshade",
            Some("dxgi"),
            &[file("ReShade64.dll", "ReShade64.dll")],
            &[],
        ));
        assert!(conversion_needs_prompt(
            false,
            "reshade",
            Some("dxgi"),
            &[file("ReShade64.dll", "dxgi.dll")],
            &[],
        ));
    }

    #[test]
    fn disabled_mod_does_not_hold_a_slot() {
        let mut holder = FileManifest {
            game: "g".into(),
            instance: "optiscaler".into(),
            mod_type: "optiscaler".into(),
            adapter: "install".into(),
            enabled: false,
            load_order: 0,
            files: vec![file("OptiScaler.dll", "dxgi.dll")].into_boxed_slice(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            include: Box::default(),
            harvested: Default::default(),
            provenance: Default::default(),
        };
        assert!(ensure_slot_free(&[holder.clone()], "reshade", "dxgi").is_ok());
        holder.enabled = true;
        let err = ensure_slot_free(&[holder], "reshade", "dxgi").unwrap_err();
        assert!(
            matches!(err, Error::SlotInUse { ref holder, .. } if holder == "optiscaler"),
            "{err}"
        );
        assert!(ensure_slot_free(&[], "reshade", SELF_SLOT).is_ok());
    }

    #[test]
    fn injector_claims_among_companion_dlls() {
        let files = vec![
            file("amd_fidelityfx_dx12.dll", "amd_fidelityfx_dx12.dll"),
            file("OptiScaler.dll", "OptiScaler.dll"),
        ];
        let idx = super::super::claiming_slot_index(&files, &[]).unwrap();
        assert_eq!(files[idx].dest, "OptiScaler.dll");
        assert!(conversion_needs_prompt(
            true,
            "optiscaler",
            Some("dxgi"),
            &files,
            &[],
        ));
        let both = vec![
            file("ReShade32.dll", "ReShade32.dll"),
            file("ReShade64.dll", "ReShade64.dll"),
        ];
        assert!(super::super::claiming_slot_index(&both, &[]).is_none());
    }
}
