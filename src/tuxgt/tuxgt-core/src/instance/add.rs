use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::{
    check_requires_known, list_mods, local_abs, official_ids, parse_recipe, recipe_path,
    user_mods_dir, write_user_recipe, Mod, SourceRef,
};

/// Unpack archive `src` into `dest` via a temp dir that is deleted on
/// every path. The single unpack-then-copy seam: `copy_payload_to_user`
/// and the install land path share it instead of each owning tmp cleanup.
/// When `clear_dest`, `dest` is removed after a successful unpack and
/// before the copy (redownload semantics: a failed unpack leaves the old
/// payload alone).
fn unpack_then_copy_impl(
    src: &Path,
    dest: &Path,
    clear_dest: bool,
    password: Option<&str>,
    use_7z: bool,
) -> Result<()> {
    let key = crate::download::url_key(&src.to_string_lossy());
    let tmp = crate::download::tmp_unpack_dir(&key);
    let _ = fs::remove_dir_all(&tmp);
    let result = (|| -> Result<()> {
        if use_7z {
            crate::download::unpack_with_password(src, &tmp, password)?;
        } else {
            crate::download::unpack(src, &tmp)?;
        }
        if clear_dest {
            let _ = fs::remove_dir_all(dest);
        }
        crate::download::copy_tree(&tmp, dest)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}

pub(crate) fn unpack_then_copy(src: &Path, dest: &Path, clear_dest: bool) -> Result<()> {
    unpack_then_copy_impl(src, dest, clear_dest, None, false)
}

pub(crate) fn unpack_then_copy_with_password(
    src: &Path,
    dest: &Path,
    clear_dest: bool,
    password: Option<&str>,
) -> Result<()> {
    unpack_then_copy_impl(src, dest, clear_dest, password, true)
}

fn copy_payload_to_user_impl(
    data_dir: &Path,
    id: &str,
    src: &Path,
    password: Option<&str>,
    use_7z: bool,
) -> Result<PathBuf> {
    let dest = user_mods_dir(data_dir).join(id);
    if dest.exists() {
        return Err(Error::InvalidInstance(format!(
            "{id}: payload dir already exists"
        )));
    }
    if src.is_dir() {
        crate::download::copy_tree(src, &dest)?;
        crate::download::strip_single_top_dir(&dest)?;
        strip_to_reshade_root_fs(&dest)?;
        return Ok(dest);
    }
    if src.is_file() {
        if is_archive(src) {
            if use_7z {
                unpack_then_copy_with_password(src, &dest, false, password)?;
            } else {
                unpack_then_copy(src, &dest, false)?;
            }
            strip_to_reshade_root_fs(&dest)?;
            return Ok(dest);
        }
        fs::create_dir_all(&dest)?;
        let name = src
            .file_name()
            .ok_or_else(|| Error::InvalidInstance(format!("no file name: {}", src.display())))?;
        let out = dest.join(name);
        fs::copy(src, &out)?;
        return Ok(out);
    }
    Err(Error::InvalidInstance(format!(
        "package path not found: {}",
        src.display()
    )))
}

/// Copy one payload into `mods/user/<id>/` using the existing archive tools.
pub(crate) fn copy_payload_to_user(data_dir: &Path, id: &str, src: &Path) -> Result<PathBuf> {
    copy_payload_to_user_impl(data_dir, id, src, None, false)
}

pub(crate) fn copy_payload_to_user_with_password(
    data_dir: &Path,
    id: &str,
    src: &Path,
    password: Option<&str>,
) -> Result<PathBuf> {
    copy_payload_to_user_impl(data_dir, id, src, password, true)
}
pub fn add_mod(config_dir: &Path, recipe_file: &Path, data_dir: &Path) -> Result<Mod> {
    let text = fs::read_to_string(recipe_file)?;
    let mut inst = parse_recipe(&text, false)?;
    if official_ids(data_dir)?.iter().any(|id| id == &inst.id) {
        return Err(Error::InvalidInstance(format!(
            "{}: id shadows an official mod",
            inst.id
        )));
    }
    let known: BTreeSet<String> = list_mods(config_dir, data_dir)?
        .mods
        .iter()
        .map(|i| i.id.clone())
        .collect();
    check_requires_known(&inst.id, &inst.requires, &known)?;
    let dir = user_mods_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let dest = dir.join(format!("{}.toml", inst.id));
    if dest.exists() {
        return Err(Error::InvalidInstance(format!(
            "{}: mod already exists",
            inst.id
        )));
    }
    // Local payload is copied into `$PREFIX/mods/user/<id>/` and the
    // stored recipe points at the copy. Remote recipes import as-is
    // (their payload is acquired at install).
    if let SourceRef::Local { path } = inst.source.clone() {
        let raw = Path::new(&path);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else if let Some(parent) = recipe_file.parent() {
            parent.join(raw)
        } else {
            raw.to_path_buf()
        };
        let abs = local_abs(&joined)?;
        let stored = copy_payload_to_user(data_dir, &inst.id, &abs)?;
        inst.source = SourceRef::Local {
            path: stored.to_string_lossy().into_owned(),
        };
        let result = write_user_recipe(&dest, &inst);
        if result.is_err() {
            let _ = fs::remove_dir_all(user_mods_dir(data_dir).join(&inst.id));
        }
        result?;
    } else {
        fs::write(&dest, text)?;
    }
    Ok(inst)
}
pub fn remove_mod(config_dir: &Path, data_dir: &Path, id: &str) -> Result<()> {
    let listed = list_mods(config_dir, data_dir)?;
    let inst = listed
        .mods
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| Error::UnknownInstance(id.into()))?;
    if inst.official {
        return Err(Error::InvalidInstance(format!(
            "{id}: cannot remove an official mod"
        )));
    }
    let dest = recipe_path(data_dir, false, inst.registry.as_deref(), id);
    if dest.exists() {
        fs::remove_file(&dest)?;
    }
    let _ = fs::remove_dir_all(payload_dir(data_dir, false, inst.registry.as_deref(), id));
    crate::db::mark_cache_dirty();
    Ok(())
}
