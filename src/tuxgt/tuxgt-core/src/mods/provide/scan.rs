use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::matching::{choose, pattern_hits};
use super::{Cand, Pick, ProvidedFiles};

pub(crate) struct Rm(PathBuf);

impl Drop for Rm {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Directory prefixes of `cands`, including ancestors (`a/b` yields `a/b` and `a`).
fn dir_prefixes(cands: &[Cand]) -> Vec<String> {
    let mut out = Vec::new();
    for cand in cands {
        let mut rest = cand.rel.as_str();
        while let Some((parent, _)) = rest.rsplit_once('/') {
            out.push(parent.to_string());
            rest = parent;
        }
    }
    out.sort();
    out.dedup();
    out
}

fn under_prefix(cands: &[Cand], prefix: &str) -> Vec<Cand> {
    cands
        .iter()
        .filter(|cand| {
            cand.rel.len() > prefix.len()
                && cand.rel.as_bytes().get(prefix.len()) == Some(&b'/')
                && cand.rel.starts_with(prefix)
        })
        .cloned()
        .collect()
}

fn prefixes_nested(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

/// Keep the original candidates when the whole tree is already usable, or
/// when two different folders each satisfy the recipe. One winning folder
/// is returned on its own.
pub(crate) fn narrow_candidates(files: &[String], cands: &[Cand]) -> Vec<Cand> {
    if files.is_empty() || classify(files, cands).ready {
        return cands.to_vec();
    }
    let mut ready: Vec<(String, Vec<Cand>)> = Vec::new();
    for prefix in dir_prefixes(cands) {
        let subset = under_prefix(cands, &prefix);
        if classify(files, &subset).ready {
            ready.push((prefix, subset));
        }
    }
    if ready.is_empty() {
        return cands.to_vec();
    }
    let disjoint = ready
        .iter()
        .any(|(a, _)| ready.iter().any(|(b, _)| a != b && !prefixes_nested(a, b)));
    if disjoint {
        return cands.to_vec();
    }
    ready
        .into_iter()
        .min_by_key(|(_, subset)| subset.len())
        .map(|(_, subset)| subset)
        .unwrap_or_else(|| cands.to_vec())
}

pub(crate) fn classify(files: &[String], cands: &[Cand]) -> ProvidedFiles {
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut ambiguous = Vec::new();
    for pat in files {
        let hits = pattern_hits(files, pat, cands);
        match choose(pat, &hits) {
            Pick::One(_) => present.push(pat.clone()),
            Pick::Missing => missing.push(pat.clone()),
            Pick::Ambiguous => ambiguous.push(pat.clone()),
        }
    }
    let ready = missing.is_empty() && ambiguous.is_empty();
    ProvidedFiles {
        present,
        missing,
        ambiguous,
        ready,
    }
}

/// One directory per call. Overlapping `provide_files` on the same archive
/// must not `remove_dir_all` each other's extract.
fn unpack_key(path: &Path) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::download::url_key(&format!(
        "provide:{}:{n}:{seq}:{}",
        std::process::id(),
        path.display()
    ))
}

pub(crate) fn load_candidates(
    path: &Path,
    password: Option<&str>,
) -> Result<(Vec<Cand>, Option<Rm>)> {
    if path.is_dir() {
        let mut cands = Vec::new();
        walk(path, path, &mut cands)?;
        return Ok((cands, None));
    }
    if path.is_file() && crate::instance::is_archive(path) {
        let key = unpack_key(path);
        let tmp = crate::download::tmp_unpack_dir(&key);
        let _ = fs::remove_dir_all(&tmp);
        let guard = Rm(tmp.clone());
        crate::download::unpack_with_password(path, &tmp, password)?;
        crate::download::strip_single_top_dir(&tmp)?;
        let mut cands = Vec::new();
        walk(&tmp, &tmp, &mut cands)?;
        return Ok((cands, Some(guard)));
    }
    let rel = path
        .file_name()
        .ok_or_else(|| Error::InvalidInstance(format!("no file name: {}", path.display())))?
        .to_string_lossy()
        .into_owned();
    if rel.is_empty() || super::super::is_junk_dest(&rel) {
        return Ok((Vec::new(), None));
    }
    Ok((
        vec![Cand {
            rel,
            path: path.to_path_buf(),
        }],
        None,
    ))
}

pub(crate) fn walk(root: &Path, dir: &Path, out: &mut Vec<Cand>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        let rel = rel_of(root, &path);
        if path.is_dir() {
            if rel
                .split('/')
                .any(|seg| seg.len() > 1 && seg.starts_with('.'))
            {
                continue;
            }
            walk(root, &path, out)?;
            continue;
        }
        if path.file_name().is_some_and(|n| n == ".provenance.toml") {
            continue;
        }
        if rel.is_empty() || super::super::is_junk_dest(&rel) {
            continue;
        }
        out.push(Cand { rel, path });
    }
    Ok(())
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::unpack_key;
    use std::path::Path;

    #[test]
    fn unpack_keys_differ_for_the_same_archive() {
        let path = Path::new("/tmp/same.zip");
        assert_ne!(unpack_key(path), unpack_key(path));
    }
}
