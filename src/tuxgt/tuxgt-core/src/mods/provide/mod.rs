use std::fs;
use std::path::{Path, PathBuf};

use crate::instance::{payload_dir, Mod, SourceRef};
use crate::{Error, Result};

mod matching;
mod scan;

use matching::{choose, concrete_name, pattern_hits};
use scan::{classify, load_candidates, narrow_candidates, walk};

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
