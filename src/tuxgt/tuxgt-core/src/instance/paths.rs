use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn official_mods_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("mods").join("official")
}

pub(crate) fn user_mods_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("mods").join("user")
}

pub(crate) fn share_templates_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("share").join("templates")
}

/// Kind dir name under `mods/`: `official`, `user`, or a registry slug.
pub(crate) fn mod_kind(official: bool, registry: Option<&str>) -> &str {
    if official {
        "official"
    } else {
        registry.unwrap_or("user")
    }
}

pub(crate) fn payload_dir(
    data_dir: &Path,
    official: bool,
    registry: Option<&str>,
    id: &str,
) -> PathBuf {
    data_dir
        .join("mods")
        .join(mod_kind(official, registry))
        .join(id)
}

pub(crate) fn recipe_path(
    data_dir: &Path,
    official: bool,
    registry: Option<&str>,
    id: &str,
) -> PathBuf {
    data_dir
        .join("mods")
        .join(mod_kind(official, registry))
        .join(format!("{id}.toml"))
}

pub(crate) const PAYLOAD_PROV: &str = ".provenance.toml";

pub(crate) fn write_payload_provenance(payload: &Path, p: &crate::ModProvenance) {
    let _ = fs::create_dir_all(payload);
    if let Ok(text) = toml::to_string(p) {
        let _ = fs::write(payload.join(PAYLOAD_PROV), text);
    }
}

pub(crate) fn read_payload_provenance(payload: &Path) -> Option<crate::ModProvenance> {
    let text = fs::read_to_string(payload.join(PAYLOAD_PROV)).ok()?;
    toml::from_str(&text).ok()
}
