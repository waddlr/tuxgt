use std::fs;
use std::path::{Path, PathBuf};

use crate::instance::{payload_dir, Mod, SourceRef};
use crate::{Error, Result};

/// How a `provided` recipe's patterns sit in its payload dir.
#[derive(Debug)]
pub struct ProvidedFiles {
    /// Patterns with exactly one payload file.
    pub present: Vec<String>,
    /// Patterns with zero matches.
    pub missing: Vec<String>,
    /// Patterns still matching more than one.
    pub ambiguous: Vec<String>,
    /// `missing` and `ambiguous` are both empty.
    pub ready: bool,
}

/// Result of one [`provide_files`] call, after the copy.
#[derive(Debug)]
pub struct ProvideReport {
    pub present: Vec<String>,
    pub missing: Vec<String>,
    pub ambiguous: Vec<String>,
    /// Catalog offered-flag after this call.
    pub enabled: bool,
}

#[derive(Clone)]
struct Cand {
    rel: String,
    path: PathBuf,
}

struct Rm(PathBuf);

impl Drop for Rm {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

enum Pick {
    One(PathBuf),
    Missing,
    Ambiguous,
}

/// `None` when `inst` is not a `provided` source. Does not create directories.
pub fn provided_files(data_dir: &Path, inst: &Mod) -> Result<Option<ProvidedFiles>> {
    let SourceRef::Provided { files, .. } = &inst.source else {
        return Ok(None);
    };
    let payload = payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    let cands = if payload.is_dir() {
        let mut c = Vec::new();
        walk(&payload, &payload, &mut c)?;
        c
    } else {
        Vec::new()
    };
    Ok(Some(classify(files, &cands)))
}

/// Copy user-supplied files into a `provided` mod's payload.
///
/// Unique matches are saved even when other patterns are still missing.
/// A failed unpack does not touch the payload. An encrypted archive returns
/// [`Error::ArchivePasswordRequired`] until [`provide_files_with_password`]
/// is given the password.
pub fn provide_files(
    config_dir: &Path,
    data_dir: &Path,
    id: &str,
    path: &Path,
) -> Result<ProvideReport> {
    provide_files_with_password(config_dir, data_dir, id, path, None)
}

/// [`provide_files`] with the archive password. `None` still detects an
/// encrypted archive instead of prompting.
pub fn provide_files_with_password(
    config_dir: &Path,
    data_dir: &Path,
    id: &str,
    path: &Path,
    password: Option<&str>,
) -> Result<ProvideReport> {
    let inst = crate::find_mod(config_dir, data_dir, id)?;
    let SourceRef::Provided { files, .. } = &inst.source else {
        return Err(Error::InvalidInstance(format!(
            "{id}: not a provided source"
        )));
    };
    let files: Vec<String> = files.to_vec();
    if !path.exists() || (!path.is_file() && !path.is_dir()) {
        return Err(Error::InvalidInstance(format!(
            "package path not found: {}",
            path.display()
        )));
    }
    let was_ready = provided_files(data_dir, &inst)?
        .map(|p| p.ready)
        .unwrap_or(false);
    let (cands, _tmp) = load_candidates(path, password)?;
    // A multi-arch drop names the same file twice. When exactly one
    // subdirectory satisfies every pattern, that folder is the drop.
    let cands = narrow_candidates(&files, &cands);
    let mut accepted: Vec<(PathBuf, String)> = Vec::new();
    let mut input_ambiguous: Vec<String> = Vec::new();
    for pat in &files {
        let hits = pattern_hits(&files, pat, &cands);
        match choose(pat, &hits) {
            Pick::One(p) => accepted.push((p, concrete_name(pat))),
            Pick::Ambiguous => input_ambiguous.push(pat.clone()),
            Pick::Missing => {}
        }
    }
    // One required file and one supplied file: the source name does not matter.
    if files.len() == 1 && accepted.is_empty() && input_ambiguous.is_empty() && cands.len() == 1 {
        accepted.push((cands[0].path.clone(), concrete_name(&files[0])));
    }
    if files.len() == 1 {
        if let Some(pin) = inst.sha256.as_deref() {
            if let Some((src, _)) = accepted.first() {
                let got = crate::sha256_file(src)?;
                if got != pin {
                    return Err(Error::HashMismatch {
                        want: pin.to_string(),
                        got,
                    });
                }
            }
        }
    }
    let payload = payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    if !accepted.is_empty() {
        fs::create_dir_all(&payload)?;
        for (src, name) in &accepted {
            if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
                return Err(Error::InvalidInstance(format!(
                    "bad payload file name: {name}"
                )));
            }
            fs::copy(src, payload.join(name))?;
        }
    }
    let scanned = provided_files(data_dir, &inst)?.unwrap_or(ProvidedFiles {
        present: Vec::new(),
        missing: files.clone(),
        ambiguous: Vec::new(),
        ready: false,
    });
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut ambiguous = Vec::new();
    for pat in &files {
        if scanned.present.iter().any(|p| p == pat) {
            present.push(pat.clone());
        } else if scanned.ambiguous.iter().any(|p| p == pat)
            || input_ambiguous.iter().any(|p| p == pat)
        {
            ambiguous.push(pat.clone());
        } else {
            missing.push(pat.clone());
        }
    }
    let ready = missing.is_empty() && ambiguous.is_empty();
    // A partial copy is not a version. Publishing its digest makes an
    // installed game look updated while files are still missing.
    if ready && payload.is_dir() {
        let (sha, bytes) = crate::download::digest_path(&payload)?;
        crate::instance::write_payload_provenance(
            &payload,
            &crate::ModProvenance {
                source: "provided".into(),
                asset_sha256: sha,
                asset_bytes: bytes,
                fetched_at: crate::download::now_unix(),
            },
        );
        crate::db::mark_cache_dirty();
    } else if payload.is_dir() {
        let prov = payload.join(".provenance.toml");
        if prov.exists() {
            fs::remove_file(prov)?;
        }
        crate::db::mark_cache_dirty();
    }
    if !was_ready && ready {
        crate::enable_mod(config_dir, id, data_dir)?;
    }
    let enabled = crate::find_mod(config_dir, data_dir, id)?.enabled;
    Ok(ProvideReport {
        present,
        missing,
        ambiguous,
        enabled,
    })
}

/// Drop a `provided` mod's payload and turn it off. The recipe stays.
pub fn clear_provided_files(config_dir: &Path, data_dir: &Path, id: &str) -> Result<()> {
    let inst = crate::find_mod(config_dir, data_dir, id)?;
    if !matches!(inst.source, SourceRef::Provided { .. }) {
        return Err(Error::InvalidInstance(format!(
            "{id}: not a provided source"
        )));
    }
    let payload = payload_dir(data_dir, inst.official, inst.registry.as_deref(), &inst.id);
    if payload.exists() {
        fs::remove_dir_all(&payload)?;
    }
    crate::db::mark_cache_dirty();
    crate::disable_mod(config_dir, id, data_dir)?;
    Ok(())
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
fn narrow_candidates(files: &[String], cands: &[Cand]) -> Vec<Cand> {
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

fn classify(files: &[String], cands: &[Cand]) -> ProvidedFiles {
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

fn load_candidates(path: &Path, password: Option<&str>) -> Result<(Vec<Cand>, Option<Rm>)> {
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
    if rel.is_empty() || super::is_junk_dest(&rel) {
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

fn walk(root: &Path, dir: &Path, out: &mut Vec<Cand>) -> Result<()> {
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
        if rel.is_empty() || super::is_junk_dest(&rel) {
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

/// Payload basename for a pattern. Glob marks are stripped so
/// `my-mod-*.dll` is stored as `my-mod-.dll`.
fn concrete_name(pat: &str) -> String {
    let base = pat.rsplit(['/', '\\']).next().unwrap_or(pat);
    base.chars().filter(|c| *c != '*' && *c != '?').collect()
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

/// Same extension, and the source stem is the dest stem plus a `-` or `.`
/// version tail (`renodx-dlss-v1` for `renodx-dlss`). `_` is not a separator:
/// `nvngx_dlssd` must not fill `nvngx_dlss`, and `sl.dlss_g` must not fill `sl.dlss`.
fn prefix_match(pat: &str, base: &str) -> bool {
    let dest = concrete_name(pat);
    let (dst_stem, dst_ext) = split_ext(&dest);
    let (src_stem, src_ext) = split_ext(base);
    if dst_stem.is_empty() || !dst_ext.eq_ignore_ascii_case(src_ext) {
        return false;
    }
    let src_l = src_stem.to_ascii_lowercase();
    let dst_l = dst_stem.to_ascii_lowercase();
    match src_l.strip_prefix(&dst_l) {
        Some(rest) => rest.starts_with('-') || rest.starts_with('.'),
        None => false,
    }
}

fn pattern_hit(pat: &str, rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let glob_ok = if pat.contains('/') {
        crate::download::glob_match(pat, rel)
    } else {
        crate::download::glob_match(pat, base)
    };
    glob_ok || prefix_match(pat, base)
}

/// Hits for one pattern. A candidate that exactly fulfills a *sibling*
/// pattern is not a hit here, not even as a `-`/`.` version tail:
/// `nvngx_dlssnr.real.dll` must not fill `nvngx_dlssnr.dll`.
fn pattern_hits<'a>(files: &[String], pat: &str, cands: &'a [Cand]) -> Vec<&'a Cand> {
    let mine = concrete_name(pat);
    cands
        .iter()
        .filter(|c| {
            if !pattern_hit(pat, &c.rel) {
                return false;
            }
            let base = c.rel.rsplit('/').next().unwrap_or(&c.rel);
            base == mine
                || !files
                    .iter()
                    .any(|f| concrete_name(f).eq_ignore_ascii_case(base))
        })
        .collect()
}

fn has_segment(rel: &str, name: &str) -> bool {
    rel.split('/').any(|seg| seg.eq_ignore_ascii_case(name))
}

fn choose(pat: &str, hits: &[&Cand]) -> Pick {
    if hits.is_empty() {
        return Pick::Missing;
    }
    let dest = concrete_name(pat);
    let exact: Vec<&Cand> = hits
        .iter()
        .copied()
        .filter(|c| c.rel.rsplit('/').next().unwrap_or(&c.rel) == dest)
        .collect();
    let pool: Vec<&Cand> = if exact.is_empty() {
        hits.to_vec()
    } else {
        exact
    };
    if pool.len() == 1 {
        return Pick::One(pool[0].path.clone());
    }
    let hits = pool.as_slice();
    let mut kept: Vec<&Cand> = hits.to_vec();
    let x64: Vec<&Cand> = kept
        .iter()
        .copied()
        .filter(|c| has_segment(&c.rel, "x64"))
        .collect();
    if !x64.is_empty() {
        kept = x64;
    }
    kept.retain(|c| {
        !has_segment(&c.rel, "development")
            && !has_segment(&c.rel, "x86")
            && !has_segment(&c.rel, "win32")
    });
    if kept.len() == 1 {
        Pick::One(kept[0].path.clone())
    } else {
        Pick::Ambiguous
    }
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
