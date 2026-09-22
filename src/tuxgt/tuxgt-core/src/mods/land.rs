use std::fs;
use std::path::{Path, PathBuf};

use crate::{sha256_file, PlannedFile, Result};

pub(crate) fn depot_source(data_dir: &Path, f: &PlannedFile) -> Option<PathBuf> {
    if let Some(rest) = f.source.strip_prefix("mods/") {
        let path = data_dir.join("mods").join(rest);
        return path.is_file().then_some(path);
    }
    if let Some(rest) = f.source.strip_prefix("cache/") {
        let (key, tail) = rest.split_once('/')?;
        let rel = tail.split_once('#')?.1;
        let path = data_dir.join("unpack").join(key).join(rel);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = PathBuf::from(&f.source);
    path.is_file().then_some(path)
}

pub(crate) fn land_from_payload(
    payload: &Path,
    source_prefix: &str,
) -> Result<(Vec<PathBuf>, Vec<PlannedFile>)> {
    let mut files = Vec::new();
    collect_payload_files(payload, &mut files)?;
    let planned = files
        .iter()
        .map(|p| {
            let rel = p
                .strip_prefix(payload)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            let sha256 = sha256_file(p)?;
            Ok(PlannedFile {
                source: format!("{source_prefix}/{rel}"),
                dest: rel,
                sha256,
                enabled: true,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((files, planned))
}

pub(crate) fn collect_payload_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            collect_payload_files(&e, out)?;
        } else if e.file_name().is_some_and(|n| n != ".provenance.toml") {
            out.push(e);
        }
    }
    Ok(())
}
