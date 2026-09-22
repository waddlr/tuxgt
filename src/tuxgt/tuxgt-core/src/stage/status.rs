use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::{read_staging_toml, stage_dir, staging_toml_path, StagingToml};
use crate::{game_manifests, sha256_file, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageState {
    InSync,
    UserModified,
    DepotNewer,
    Unmanaged,
}

impl StageState {
    pub fn as_str(self) -> &'static str {
        match self {
            StageState::InSync => "in-sync",
            StageState::UserModified => "user-modified",
            StageState::DepotNewer => "depot-newer",
            StageState::Unmanaged => "unmanaged",
        }
    }
}

#[derive(Debug)]
pub struct StageLine {
    pub instance: String,
    pub file: String,
    pub state: StageState,
}

/// Hand-dropped staged files (claimed by no manifest) as `unmanaged` lines.
/// Planned dests are excluded even when their bookkeeping is missing, so a
/// managed file never double-reports.
fn push_unmanaged(
    dir: &Path,
    base: &Path,
    toml: &StagingToml,
    planned: &BTreeSet<&str>,
    inst: &str,
    out: &mut Vec<StageLine>,
) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() {
            push_unmanaged(&p, base, toml, planned, inst, out);
        } else if let Some(rel) = p
            .strip_prefix(base)
            .ok()
            .map(|r| r.to_string_lossy().replace('\\', "/"))
        {
            if !toml.files.contains_key(&rel) && !planned.contains(rel.as_str()) {
                out.push(StageLine {
                    instance: inst.into(),
                    file: rel,
                    state: StageState::Unmanaged,
                });
            }
        }
    }
}

/// Per-file sync state for every enabled or disabled manifest of a game.
/// A staged file missing from disk is work to redo (`depot-newer`).
/// `depot-newer` also fires when the manifest moved on (re-install) while
/// staging stayed. Omitted dests (E64) are not reported: they are not staged.
/// Staged files no manifest claims (hand-dropped) report as `unmanaged`.
pub fn stage_status(data_dir: &Path, game_id: &str) -> Result<Vec<StageLine>> {
    let mut out = Vec::new();
    for m in game_manifests(data_dir, game_id)? {
        let dir = stage_dir(data_dir, game_id, &m.instance);
        let toml = read_staging_toml(&staging_toml_path(data_dir, game_id, &m.instance))?;
        for planned in m.files.iter().filter(|f| f.enabled) {
            let dest = dir.join(&planned.dest);
            let current = dest.is_file().then(|| sha256_file(&dest)).transpose()?;
            let state = match (current, toml.files.get(&planned.dest)) {
                (Some(hash), Some(e)) if hash == e.staged_sha => {
                    if e.depot_sha != planned.sha256 {
                        StageState::DepotNewer
                    } else {
                        StageState::InSync
                    }
                }
                (Some(hash), None) if hash == planned.sha256 => StageState::InSync,
                (None, _) => StageState::DepotNewer,
                _ => StageState::UserModified,
            };
            out.push(StageLine {
                instance: m.instance.clone(),
                file: planned.dest.clone(),
                state,
            });
        }
        let planned: BTreeSet<&str> = m
            .files
            .iter()
            .filter(|f| f.enabled)
            .map(|f| f.dest.as_str())
            .collect();
        push_unmanaged(&dir, &dir, &toml, &planned, &m.instance, &mut out);
    }
    out.sort_by(|a, b| (&a.instance, &a.file).cmp(&(&b.instance, &b.file)));
    Ok(out)
}
