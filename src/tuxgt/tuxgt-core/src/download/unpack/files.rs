use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub(crate) fn copy_plain(asset: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    let name = asset
        .file_name()
        .ok_or_else(|| Error::Unpack("no file name".into()))?;
    let out = dest.join(name);
    fs::copy(asset, &out)?;
    Ok(vec![out])
}

pub(crate) fn strip_single_top_dir(dest: &Path) -> Result<()> {
    let entries: Vec<PathBuf> = fs::read_dir(dest)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    if entries.len() == 1 && entries[0].is_dir() {
        let tmp = dest.with_extension("strip");
        let _ = fs::remove_dir_all(&tmp);
        fs::rename(&entries[0], &tmp)?;
        for e in fs::read_dir(&tmp)? {
            let e = e?;
            fs::rename(e.path(), dest.join(e.file_name()))?;
        }
        fs::remove_dir_all(&tmp)?;
    }
    Ok(())
}

pub(crate) fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            collect_files(&e, out)?;
        } else if e.file_name().is_some_and(|n| n != ".provenance.toml") {
            out.push(e);
        }
    }
    Ok(())
}
