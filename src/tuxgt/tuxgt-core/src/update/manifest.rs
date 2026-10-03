//! `$PREFIX/config/prefix-manifest.toml`: the owned PREFIX file set of the
//! last applied self-update. Mirrors the host inventory: the previous
//! manifest decides what a new package may delete, and a missing file
//! (legacy install) skips the stale pass.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// One owned PREFIX file, path relative to the prefix (`/` separators).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrefixFile {
    pub path: String,
    pub sha256: String,
}

/// Release tag + owned file set written by the last applied update.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrefixManifest {
    pub tag: String,
    pub files: Vec<PrefixFile>,
}

pub(crate) fn prefix_manifest_path(prefix: &Path) -> PathBuf {
    prefix.join("config/prefix-manifest.toml")
}

/// Load the last-applied manifest. Missing file → `Ok(None)`.
pub fn load_prefix_manifest(prefix: &Path) -> Result<Option<PrefixManifest>> {
    let path = prefix_manifest_path(prefix);
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
        Ok(text) => {
            let manifest: PrefixManifest = toml::from_str(&text)
                .map_err(|e| Error::Update(format!("prefix manifest parse: {e}")))?;
            Ok(Some(manifest))
        }
    }
}

/// Save the just-applied manifest. Atomic write via tmp + rename.
pub fn save_prefix_manifest(prefix: &Path, manifest: &PrefixManifest) -> Result<()> {
    let path = prefix_manifest_path(prefix);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(manifest)
        .map_err(|e| Error::Update(format!("prefix manifest encode: {e}")))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_missing_is_none() {
        let root =
            std::env::temp_dir().join(format!("tuxgt-update-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let prefix = root.join("prefix");
        assert_eq!(load_prefix_manifest(&prefix).expect("missing"), None);
        let manifest = PrefixManifest {
            tag: "v0.9.1".into(),
            files: vec![
                PrefixFile {
                    path: "bin/tuxgt".into(),
                    sha256: "abc".into(),
                },
                PrefixFile {
                    path: "mods/official/reshade.toml".into(),
                    sha256: "def".into(),
                },
            ],
        };
        save_prefix_manifest(&prefix, &manifest).expect("save");
        assert_eq!(load_prefix_manifest(&prefix).expect("load"), Some(manifest));
        let _ = std::fs::remove_dir_all(&root);
    }
}
