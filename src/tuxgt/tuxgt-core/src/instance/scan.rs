use super::*;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::download::unpack;
use crate::{parse_mod_type, Error, Result};

use super::{parse_recipe, Mod, SourceRef};

/// Source resolve stub: parse only, no I/O. Downloading is E15.
pub fn resolve_source(inst: &Mod) -> SourceRef {
    inst.source.clone()
}

/// One archive-relative file from a package scan, with keep/dest defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageFile {
    pub src: String,
    pub dest: String,
    pub keep: bool,
    pub is_new: bool,
}

pub(crate) fn unique_temp(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{tag}-{}-{n}", std::process::id()))
}

pub(crate) fn walk_rel_files(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            walk_rel_files(&e, root, out)?;
        } else {
            let rel = e.strip_prefix(root).unwrap_or(&e);
            let s = rel.to_string_lossy().replace('\\', "/");
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    Ok(())
}

pub(crate) fn strip_single_top_rel(root: &Path, files: &mut Vec<String>) -> Result<()> {
    let entries: Vec<PathBuf> = fs::read_dir(root)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    if entries.len() == 1 && entries[0].is_dir() {
        let prefix = entries[0]
            .file_name()
            .map(|n| n.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if prefix.is_empty() {
            return Ok(());
        }
        let pfx = format!("{prefix}/");
        for f in files.iter_mut() {
            if let Some(rest) = f.strip_prefix(&pfx) {
                *f = rest.to_string();
            }
        }
        files.retain(|f| f != &prefix && !f.is_empty());
    }
    Ok(())
}

/// ReShade-root wrapper prefix: first `reshade-shaders` component at index > 0.
/// `Shaders/`-direct packs (no marker) and already-root lists yield `None`.
pub(crate) fn reshade_wrapper_prefix(files: &[String]) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in files {
        let norm = f.replace('\\', "/");
        let comps: Vec<&str> = norm.split('/').filter(|c| !c.is_empty()).collect();
        for (k, c) in comps.iter().enumerate() {
            if c.eq_ignore_ascii_case("reshade-shaders") {
                if k > 0 {
                    let pfx = comps[..k].join("/") + "/";
                    *counts.entry(pfx).or_insert(0) += 1;
                }
                break;
            }
        }
    }
    if counts.is_empty() {
        return None;
    }
    let max = counts.values().copied().max().unwrap_or(0);
    counts
        .into_iter()
        .filter(|(_, n)| *n == max)
        .map(|(p, _)| p)
        .min_by_key(|p| p.len())
}

/// Strip the wrapper prefix from every entry; idempotent for root lists.
pub(crate) fn rebase_reshade_srcs(files: &mut Vec<String>) {
    let Some(pfx) = reshade_wrapper_prefix(files) else {
        return;
    };
    let ncomp = pfx.split('/').filter(|c| !c.is_empty()).count();
    for f in files.iter_mut() {
        let norm = f.replace('\\', "/");
        let comps: Vec<&str> = norm.split('/').collect();
        if comps.len() <= ncomp {
            continue;
        }
        let head = comps[..ncomp].join("/") + "/";
        if head.eq_ignore_ascii_case(&pfx) {
            *f = comps[ncomp..].join("/");
        }
    }
    files.retain(|f| !f.is_empty());
    files.sort();
    files.dedup();
}

/// FS-level rebase: move `dir/<prefix>/` children up into `dir`.
pub(crate) fn strip_to_reshade_root_fs(dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    walk_rel_files(dir, dir, &mut files).unwrap_or(());
    let Some(pfx) = reshade_wrapper_prefix(&files) else {
        return Ok(());
    };
    let src_root = dir.join(&pfx);
    let mut moved: Vec<String> = Vec::new();
    walk_rel_files(&src_root, &src_root, &mut moved).unwrap_or(());
    for rel in moved {
        let from = src_root.join(&rel);
        let to = dir.join(&rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::rename(&from, &to).is_err() {
            fs::copy(&from, &to)?;
            fs::remove_file(&from)?;
        }
    }
    let _ = fs::remove_dir_all(&src_root);
    // Remove now-empty ancestor chain up to `dir` (single top-level wrapper).
    let mut cur = src_root.parent();
    while let Some(p) = cur {
        if p == dir {
            break;
        }
        let empty = fs::read_dir(p)
            .map(|mut e| e.next().is_none())
            .unwrap_or(false);
        if !empty {
            break;
        }
        let _ = fs::remove_dir(p);
        cur = p.parent();
    }
    Ok(())
}

pub(crate) fn list_package_srcs(path: &Path) -> Result<Vec<String>> {
    if path.is_dir() {
        let mut files = Vec::new();
        walk_rel_files(path, path, &mut files)?;
        strip_single_top_rel(path, &mut files)?;
        rebase_reshade_srcs(&mut files);
        files.sort();
        files.dedup();
        Ok(files)
    } else if path.is_file() {
        let tmp = unique_temp("tuxgt-e49-scan");
        let _ = fs::remove_dir_all(&tmp);
        let listed = (|| {
            let unpacked = unpack(path, &tmp)?;
            let mut files: Vec<String> = unpacked
                .iter()
                .map(|p| {
                    p.strip_prefix(&tmp)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .filter(|s| !s.is_empty())
                .collect();
            rebase_reshade_srcs(&mut files);
            files.sort();
            files.dedup();
            Ok(files)
        })();
        let _ = fs::remove_dir_all(&tmp);
        listed
    } else {
        Err(Error::InvalidInstance(format!(
            "package path not found: {}",
            path.display()
        )))
    }
}

pub(crate) fn type_drop_globs(mod_type: &str, share_dir: &Path) -> Vec<String> {
    let Ok(list) = official_mods(share_dir) else {
        return Vec::new();
    };
    let mut drops = BTreeSet::new();
    for i in list {
        if i.mod_type == mod_type {
            for r in i.payload {
                drops.extend(r.drop);
            }
        }
    }
    drops.into_iter().collect()
}

pub(crate) fn default_dest(mod_type: &str, src: &str) -> String {
    crate::modtype::type_dest_for(mod_type, src)
}

pub(crate) fn package_stem(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "package".into())
}
/// Package-support files that are common noise in local Add/Rescan forms.
/// This only controls the initial checkbox state; users can re-select every
/// row and Save still copies the file into the user payload.
fn is_package_support_file(src: &str) -> bool {
    let base = src.rsplit('/').next().unwrap_or(src);
    let lower = base.to_ascii_lowercase();
    lower.ends_with(".bat")
        || lower.ends_with(".reg")
        || lower.ends_with(".md")
        || lower.ends_with(".txt")
        || lower.ends_with(".sh")
        || lower.starts_with("readme")
        || lower.starts_with("license")
}

/// Scan a directory or archive. Package-support files and type-specific
/// recipe drops start unchecked; every row remains selectable in Add/Rescan.
pub fn scan_package(path: &Path, mod_type: &str, data_dir: &Path) -> Result<Vec<PackageFile>> {
    parse_mod_type(mod_type)?;
    let drops = type_drop_globs(mod_type, &official_mods_dir(data_dir));
    let srcs = list_package_srcs(path)?;
    if srcs.is_empty() {
        return Err(Error::InvalidInstance("package contains no files".into()));
    }
    Ok(srcs
        .into_iter()
        .map(|src| {
            let keep = !is_package_support_file(&src)
                && !drops.iter().any(|g| crate::download::glob_match(g, &src));
            let dest = default_dest(mod_type, &src);
            PackageFile {
                src,
                dest,
                keep,
                is_new: false,
            }
        })
        .collect())
}
/// One Add-pick classification (E88): what the Settings Add form does next.
/// `temps` holds archive-unpack dirs the caller deletes on cancel (lock 11:
/// Save copies payload into `$PREFIX/mods/user/<id>/`; cancel leaves nothing).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClassifyKind {
    /// A recipe TOML to import (a standalone file, or the single valid
    /// recipe inside a picked folder).
    Recipe { file: PathBuf },
    /// A single payload file; `injectable` = `.dll`/`.addon`/`.addon64`.
    Single { file: PathBuf, injectable: bool },
    /// A multi-file folder payload.
    Folder { dir: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Classification {
    pub kind: ClassifyKind,
    pub temps: Vec<PathBuf>,
}

pub(crate) fn is_injectable(path: &Path) -> bool {
    let n = path.to_string_lossy().to_ascii_lowercase();
    n.ends_with(".dll") || n.ends_with(".addon") || n.ends_with(".addon64")
}

pub(crate) fn is_archive(path: &Path) -> bool {
    let n = path.to_string_lossy().to_ascii_lowercase();
    n.ends_with(".zip")
        || n.ends_with(".rar")
        || n.ends_with(".7z")
        || n.ends_with(".cab")
        || n.ends_with(".tar.gz")
        || n.ends_with(".tgz")
        || n.ends_with(".tar.bz2")
        || n.ends_with(".exe")
}

/// Classify one Add pick per the E86 tree: a parseable recipe TOML imports
/// as a recipe (an unparseable TOML falls through to a single regular
/// file); an archive unpacks to a temp dir and follows the folder rules;
/// `.dll`/`.addon`/`.addon64` is a single injectable; anything else is a
/// single regular file. A directory with exactly one file entry classifies
/// as that entry; with exactly one parseable recipe TOML it imports the
/// recipe; several valid recipes error; otherwise it is a multi-file folder.
pub fn classify_package(path: &Path) -> Result<Classification> {
    classify_package_with_password(path, None)
}

pub fn classify_package_with_password(
    path: &Path,
    password: Option<&str>,
) -> Result<Classification> {
    if path.is_file() {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        if lower.ends_with(".toml") {
            if fs::read_to_string(path)
                .ok()
                .is_some_and(|t| parse_recipe(&t, false).is_ok())
            {
                return Ok(Classification {
                    kind: ClassifyKind::Recipe { file: path.into() },
                    temps: Vec::new(),
                });
            }
        }
        if is_archive(path) {
            let tmp = unique_temp("tuxgt-e88-classify");
            let _ = fs::remove_dir_all(&tmp);
            if let Err(e) = crate::download::unpack_with_password(path, &tmp, password) {
                let _ = fs::remove_dir_all(&tmp);
                return Err(e);
            }
            let mut inner = match classify_folder_with_password(&tmp, password) {
                Ok(inner) => inner,
                Err(e) => {
                    let _ = fs::remove_dir_all(&tmp);
                    return Err(e);
                }
            };
            inner.temps.push(tmp);
            return Ok(inner);
        }
        return Ok(Classification {
            kind: ClassifyKind::Single {
                file: path.into(),
                injectable: is_injectable(path),
            },
            temps: Vec::new(),
        });
    }
    if path.is_dir() {
        return classify_folder_with_password(path, password);
    }
    Err(Error::InvalidInstance(format!(
        "package path not found: {}",
        path.display()
    )))
}


fn classify_folder_with_password(
    dir: &Path,
    password: Option<&str>,
) -> Result<Classification> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    if entries.len() == 1 && entries[0].is_file() {
        return classify_package_with_password(&entries[0], password);
    }
    let mut recipes = Vec::new();
    for e in &entries {
        if e.is_file()
            && e.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
            && fs::read_to_string(e)
                .ok()
                .is_some_and(|t| parse_recipe(&t, false).is_ok())
        {
            recipes.push(e.clone());
        }
    }
    if recipes.len() == 1 {
        return Ok(Classification {
            kind: ClassifyKind::Recipe {
                file: recipes.pop().unwrap(),
            },
            temps: Vec::new(),
        });
    }
    if recipes.len() > 1 {
        return Err(Error::InvalidInstance(format!(
            "{}: several valid recipe TOMLs; keep exactly one",
            dir.display()
        )));
    }
    Ok(Classification {
        kind: ClassifyKind::Folder { dir: dir.into() },
        temps: Vec::new(),
    })
}
