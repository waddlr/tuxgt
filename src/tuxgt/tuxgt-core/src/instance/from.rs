use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::{parse_mod_type, Error, Result};

use super::{
    check_requires_known, copy_payload_to_user, copy_payload_to_user_with_password, existing_keep,
    list_mods, local_abs, merge_rescan, mod_from_files, official_ids, scan_package, user_mods_dir,
    write_user_recipe, Mod, PackageFile, SourceRef,
};

/// Create a user mod from a local directory or archive (scan defaults
/// unless `files` is provided).
pub fn add_mod_from(
    config_dir: &Path,
    mod_type: &str,
    id: &str,
    path: &Path,
    label: Option<&str>,
    files: Option<&[PackageFile]>,
    data_dir: &Path,
    include: Vec<String>,
    requires: Vec<String>,
) -> Result<Mod> {
    add_mod_from_impl(
        config_dir,
        mod_type,
        id,
        path,
        label,
        files,
        data_dir,
        include,
        requires,
        None,
        false,
    )
}

pub fn add_mod_from_with_password(
    config_dir: &Path,
    mod_type: &str,
    id: &str,
    path: &Path,
    label: Option<&str>,
    files: Option<&[PackageFile]>,
    data_dir: &Path,
    include: Vec<String>,
    requires: Vec<String>,
    password: Option<&str>,
) -> Result<Mod> {
    add_mod_from_impl(
        config_dir,
        mod_type,
        id,
        path,
        label,
        files,
        data_dir,
        include,
        requires,
        password,
        true,
    )
}

fn add_mod_from_impl(
    config_dir: &Path,
    mod_type: &str,
    id: &str,
    path: &Path,
    label: Option<&str>,
    files: Option<&[PackageFile]>,
    data_dir: &Path,
    include: Vec<String>,
    requires: Vec<String>,
    password: Option<&str>,
    use_7z: bool,
) -> Result<Mod> {
    parse_mod_type(mod_type)?;
    if !valid_id(id) {
        return Err(Error::InvalidInstance(format!("bad id: {id}")));
    }
    if official_ids(data_dir)?.iter().any(|o| o == id) {
        return Err(Error::InvalidInstance(format!(
            "{id}: id shadows an official mod"
        )));
    }
    let dir = user_mods_dir(data_dir);
    let dest = dir.join(format!("{id}.toml"));
    if dest.exists() {
        return Err(Error::InvalidInstance(format!("{id}: mod already exists")));
    }
    let listed = list_mods(config_dir, data_dir)?;
    if listed.mods.iter().any(|i| i.id == id) {
        return Err(Error::InvalidInstance(format!("{id}: mod already exists")));
    }
    for inc in &include {
        if inc.is_empty() {
            return Err(Error::InvalidInstance(
                "include entries must not be empty".into(),
            ));
        }
    }
    for r in &requires {
        if !valid_id(r) {
            return Err(Error::InvalidInstance(format!("bad requires id: {r}")));
        }
        if r == id {
            return Err(Error::InvalidInstance(format!("{id}: requires itself")));
        }
    }
    let known: BTreeSet<String> = listed.mods.iter().map(|i| i.id.clone()).collect();
    check_requires_known(id, &requires, &known)?;
    let abs = local_abs(path)?;
    // Lock 11: the payload is copied into `$PREFIX/mods/user/<id>/` on Save
    // and the recipe points at the copy; the original is never retained.
    // The copy (not the original) is scanned so kept srcs always match the
    // stored tree. Any failure after the copy removes it again, so a failed
    // Save leaves no payload dir behind.
    let stored = if use_7z {
        copy_payload_to_user_with_password(data_dir, id, &abs, password)?
    } else {
        copy_payload_to_user(data_dir, id, &abs)?
    };
    let result: Result<Mod> = (|| {
        let scanned = match files {
            Some(f) => f.to_vec(),
            None => scan_package(&stored, mod_type, data_dir)?,
        };
        let label = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| package_stem(&abs));
        let inst = mod_from_files(
            id,
            mod_type,
            &label,
            &stored,
            &scanned,
            Vec::new(),
            include,
            requires,
        )?;
        fs::create_dir_all(&dir)?;
        write_user_recipe(&dest, &inst)?;
        Ok(inst)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(user_mods_dir(data_dir).join(id));
    }
    result
}

/// Preview a rescan: user mods only. Official → error.
pub fn rescan_preview(
    config_dir: &Path,
    id: &str,
    data_dir: &Path,
) -> Result<(Mod, Vec<PackageFile>)> {
    let listed = list_mods(config_dir, data_dir)?;
    let inst = listed
        .mods
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| Error::UnknownInstance(id.into()))?;
    if inst.official {
        return Err(Error::InvalidInstance(format!(
            "{id}: cannot rescan an official mod"
        )));
    }
    let SourceRef::Local { path } = &inst.source else {
        return Err(Error::InvalidInstance(format!(
            "{id}: rescan requires a local source"
        )));
    };
    let abs = local_abs(Path::new(path))?;
    let scanned = scan_package(&abs, &inst.mod_type, data_dir)?;
    let keep = existing_keep(&inst);
    let files = merge_rescan(scanned, &keep, &inst.dests);
    Ok((inst, files))
}

/// Rewrite a user recipe from a package rescan. Does not touch FileManifests.
/// `include` is the caller's include dests (the GUI Rescan form): `Some`
/// replaces the previous list, `None` keeps it — the CLI `mods rescan` path.
pub fn rescan_mod(
    config_dir: &Path,
    id: &str,
    files: Option<&[PackageFile]>,
    include: Option<&[String]>,
    data_dir: &Path,
) -> Result<Mod> {
    let listed = list_mods(config_dir, data_dir)?;
    let prev = listed
        .mods
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| Error::UnknownInstance(id.into()))?;
    if prev.official {
        return Err(Error::InvalidInstance(format!(
            "{id}: cannot rescan an official mod"
        )));
    }
    let SourceRef::Local { path } = &prev.source else {
        return Err(Error::InvalidInstance(format!(
            "{id}: rescan requires a local source"
        )));
    };
    let abs = local_abs(Path::new(path))?;
    let owned;
    let files = match files {
        Some(f) => f,
        None => {
            let scanned = scan_package(&abs, &prev.mod_type, data_dir)?;
            owned = merge_rescan(scanned, &existing_keep(&prev), &prev.dests);
            &owned
        }
    };
    let include = match include {
        Some(list) => list.to_vec(),
        None => prev.include.to_vec(),
    };
    let mut inst = mod_from_files(
        &prev.id,
        &prev.mod_type,
        &prev.label,
        &abs,
        files,
        prev.games.to_vec(),
        include,
        prev.requires.to_vec(),
    )?;
    inst.env = prev.env.clone();
    inst.sha256 = prev.sha256.clone();
    inst.effect_files = prev.effect_files.clone();
    let dest = user_mods_dir(data_dir).join(format!("{id}.toml"));
    write_user_recipe(&dest, &inst)?;
    Ok(inst)
}

/// Index of the claiming Load dest among package scan dests (E91): the
/// first sibling DLL dest whose basename parses as a proxy slot, else the
/// first sibling DLL dest at all. Only top-level dests (no `/` or `\`,
/// no `pfx:` prefix) sit beside the game exe; subdir companions never
/// claim it. `None` when no sibling dest is a DLL (addons, shaders).
pub(crate) fn claiming_scan_index(dests: &[String]) -> Option<usize> {
    let sibling = |d: &String| {
        crate::prewire::is_dll(d)
            && !d.contains('/')
            && !d.contains('\\')
            && !crate::install::is_prefix_dest(d)
    };
    dests
        .iter()
        .position(|d| {
            sibling(d)
                && d.rsplit(['/', '\\'])
                    .next()
                    .is_some_and(|b| crate::modtype::parse_slot(b).is_ok())
        })
        .or_else(|| dests.iter().position(sibling))
}

/// Default `include` dests for an Add scan (E91): companion `.dll` dests
/// (every kept DLL except the claiming Load dest) default to Include.
/// Only `optiscaler` and `custom` claim Load dests; other types keep an
/// empty default. The claiming dest follows the Slot pick (default `dxgi`
/// for OptiScaler via the type dest rules).
pub fn scan_default_include(mod_type: &str, files: &[PackageFile]) -> Vec<String> {
    if mod_type != "optiscaler" && mod_type != "custom" {
        return Vec::new();
    }
    let dests: Vec<String> = files.iter().map(|f| f.dest.clone()).collect();
    let claiming = claiming_scan_index(&dests);
    files
        .iter()
        .enumerate()
        .filter(|(i, f)| f.keep && crate::prewire::is_dll(&f.dest) && Some(*i) != claiming)
        .map(|(_, f)| f.dest.clone())
        .collect()
}

/// Rewrite the claiming sibling dest to `<slot>.dll` in a package scan
/// (E91 Add-dialog Slot pick). Returns `true` when a claiming dest was
/// found; `false` (no sibling DLL at all) leaves `files` untouched.
/// Unknown slot stems error.
pub fn apply_package_slot(files: &mut [PackageFile], slot: &str) -> Result<bool> {
    let want = crate::modtype::slot_dll(slot)?;
    let dests: Vec<String> = files.iter().map(|f| f.dest.clone()).collect();
    let Some(idx) = claiming_scan_index(&dests) else {
        return Ok(false);
    };
    files[idx].dest = want;
    Ok(true)
}
