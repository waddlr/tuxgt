//! Harvested-file moves across adapter conversion.
//!
//! Runtime-generated files (`ReShade.ini`, `OptiScaler.log`, ...) live next
//! to the running mod: in the game dir on the install adapter, in
//! `<game>/runtime/` on preload. Conversion carries them from the old root
//! to the new one so user settings follow the mod; otherwise the old files
//! orphan and the next run creates fresh defaults. Disable parks them
//! outside the live root; uninstall deletes the ones no remaining instance
//! claims.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, FileManifest, Result};

/// One harvested file to carry from the conversion's source root to its
/// dest root. `identical` means the dest already held the same bytes, so
/// the apply drops the source without writing the dest.
#[derive(Clone, Debug)]
pub(crate) struct HarvestedMove {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub identical: bool,
}

/// Union of every moving manifest's generated globs, backfilling empty
/// ones from the mod type exactly like harvest does (pre-E18 manifests),
/// without persisting the backfill.
fn union_globs(moving: &[FileManifest]) -> BTreeSet<String> {
    let mut globs = BTreeSet::new();
    for m in moving {
        if m.generated_globs.is_empty() {
            let dests: Vec<&str> = m.files.iter().map(|f| f.dest.as_str()).collect();
            globs.extend(crate::generated_globs_for(&m.mod_type, &dests));
        } else {
            globs.extend(m.generated_globs.iter().cloned());
        }
    }
    globs
}

/// Plan the harvested-file moves from `from_root` to `to_root`: every file
/// under the source root whose name matches a moving manifest's generated
/// globs (same walk rules as harvest). Returns the moves plus the rels
/// present with differing bytes at both roots; the caller folds those into
/// its consent set, and the live, source-side settings win once consented.
/// Identical collisions dedupe silently. Pure read, so it runs in the
/// conversion prelude and the dry-run validator alike.
pub(crate) fn plan_harvested_moves(
    from_root: &Path,
    to_root: &Path,
    moving: &[FileManifest],
) -> Result<(Vec<HarvestedMove>, Vec<String>)> {
    let globs = union_globs(moving);
    if globs.is_empty() || from_root == to_root || !from_root.is_dir() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut moves = Vec::new();
    let mut clashes: Vec<String> = Vec::new();
    for rel in crate::download::walk_game_root(from_root) {
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        if !globs.iter().any(|g| crate::download::glob_match(g, name)) {
            continue;
        }
        let src = from_root.join(&rel);
        let dst = to_root.join(&rel);
        if dst.is_file() {
            if crate::sha256_file(&src)? == crate::sha256_file(&dst)? {
                moves.push(HarvestedMove {
                    src,
                    dst,
                    identical: true,
                });
            } else {
                clashes.push(rel);
            }
        } else if dst.exists() {
            return Err(Error::NotAFile(dst.display().to_string()));
        } else {
            moves.push(HarvestedMove {
                src,
                dst,
                identical: false,
            });
        }
    }
    Ok((moves, clashes))
}

/// Carry planned moves across: copy source bytes onto the dest (the live
/// side wins; identical moves skip the write), then drop the source. A
/// self-move or a source that vanished mid-flight is skipped — the
/// snapshot restore still puts the pre-conversion bytes back. Emptied
/// parent dirs under `from_root` are rmdir'd like `remove_runtime_dests`.
pub(crate) fn apply_harvested_moves(from_root: &Path, moves: &[HarvestedMove]) -> Result<()> {
    let mut parents: Vec<PathBuf> = Vec::new();
    for m in moves {
        if m.src == m.dst || !m.src.is_file() {
            continue;
        }
        if !m.identical {
            if let Some(parent) = m.dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&m.src, &m.dst)?;
        }
        fs::remove_file(&m.src)?;
        if let Some(p) = m.src.parent() {
            if p.starts_with(from_root) && p != from_root {
                parents.push(p.to_path_buf());
            }
        }
    }
    parents.sort_by_key(|p| std::cmp::Reverse(p.as_os_str().len()));
    parents.dedup();
    for p in parents {
        let mut cur = p;
        while cur.starts_with(from_root) && cur != from_root {
            if fs::remove_dir(&cur).is_err() {
                break;
            }
            match cur.parent() {
                Some(n) => cur = n.to_path_buf(),
                None => break,
            }
        }
    }
    Ok(())
}
