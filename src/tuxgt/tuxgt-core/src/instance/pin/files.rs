//! Slug allocation and the payload copy that follows a successful recipe write.

use std::fs;
use std::path::Path;

use super::super::{
    payload_dir, read_payload_provenance, sanitize_slug, truncate_slug_variant, user_mods_dir,
    valid_id, Mod,
};
use crate::{Error, Result};

pub(super) fn pin_slug(parent_id: &str, version: &str) -> Result<String> {
    let raw = format!("{parent_id}-{}", version.replace('.', "-"));
    let slug = truncate_slug_variant(&sanitize_slug(&raw));
    if !valid_id(&slug) {
        return Err(Error::InvalidInstance(format!("bad pin id: {slug}")));
    }
    Ok(slug)
}

pub(super) fn free_id(listed: &[Mod], data_dir: &Path, slug: &str) -> Result<String> {
    let taken = |id: &str| {
        listed.iter().any(|m| m.id == id)
            || user_mods_dir(data_dir).join(format!("{id}.toml")).exists()
    };
    if valid_id(slug) && !taken(slug) {
        return Ok(slug.to_string());
    }
    let mut n = 2u32;
    loop {
        let suffix = format!("-{n}");
        let allow = 32usize.saturating_sub(suffix.len());
        let mut base = slug.to_string();
        if base.len() > allow {
            base.truncate(allow);
            while base.ends_with('-') {
                base.pop();
            }
        }
        let cand = format!("{base}{suffix}");
        if valid_id(&cand) && !taken(&cand) {
            return Ok(cand);
        }
        n += 1;
        if n > 99 {
            return Err(Error::InvalidInstance(format!("{slug}: id already listed")));
        }
    }
}

pub(super) fn copy_matching_payload(
    data_dir: &Path,
    parent: &Mod,
    child_id: &str,
    url: &str,
) -> Result<()> {
    let src = payload_dir(
        data_dir,
        parent.official,
        parent.registry.as_deref(),
        &parent.id,
    );
    let recorded = read_source(&src);
    if recorded != url || !crate::mods::dir_has_files(&src) {
        return Ok(());
    }
    let dst = payload_dir(data_dir, false, None, child_id);
    copy_dir_all(&src, &dst).map_err(Error::from)
}

fn read_source(payload: &Path) -> String {
    read_payload_provenance(payload)
        .map(|p| p.source)
        .unwrap_or_default()
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}
