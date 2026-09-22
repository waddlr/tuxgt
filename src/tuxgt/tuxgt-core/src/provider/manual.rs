use std::collections::HashSet;
use std::path::Path;

use crate::game::{id8_prefixed, GameId, STANDALONE};
use crate::provider::{GameProvider, GameRecord};
use crate::{Error, Result};

pub struct ManualProvider;

impl GameProvider for ManualProvider {
    fn plugin_id(&self) -> &'static str {
        "manual"
    }

    fn scan(&self) -> Result<Vec<GameRecord>> {
        Ok(Vec::new())
    }
}

pub fn record_from_exe(exe: &Path, taken: &HashSet<String>) -> Result<GameRecord> {
    if !exe.is_file() {
        return Err(Error::NotAFile(exe.display().to_string()));
    }
    let canon = exe.canonicalize()?;
    let eight = id8_prefixed("manual:standalone:", &canon.to_string_lossy(), taken);
    let id = GameId::new("manual", STANDALONE, eight)?;
    let name = canon
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.game.clone());
    let mut rec = GameRecord::new(id, name);
    rec.install_dir = canon.parent().map(|p| p.to_path_buf());
    rec.exe_path = Some(canon);
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_empty() {
        assert!(ManualProvider.scan().unwrap().is_empty());
    }

    #[test]
    fn add_exe_stable_id() {
        let dir = std::env::temp_dir().join(format!("tuxgt-manual-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("game.bin");
        fs::write(&exe, b"x").unwrap();
        let a = record_from_exe(&exe, &HashSet::new()).unwrap();
        let b = record_from_exe(&exe, &HashSet::new()).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.manager, "manual");
        assert_eq!(a.id.store, STANDALONE);
        assert_eq!(a.id.game.len(), 8);
        assert_eq!(a.name, "game");
        let _ = fs::remove_dir_all(&dir);
    }
}
