//! User-level frozen copy of one unpinned mod (`mods.version-pin`).
//!
//! A `/releases/latest/download/` URL is resolved once, to `tag_name`.
//! That failure writes nothing. The copy never reports Available.

mod files;
mod name;

use std::fs;
use std::path::Path;

use super::{
    export_recipe_text, list_mods, payload_dir, read_payload_provenance, user_mods_dir, Mod,
    SourceRef,
};
use crate::{Error, Result};
use name::{frozen_of, name_offline, named_version, Frozen};

/// One `pin_version` outcome. `already` means this parent and frozen source
/// already have a copy; nothing was written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinResult {
    pub id: String,
    pub label: String,
    pub version: String,
    pub already: bool,
}

/// True when Settings or a game card may offer `Pin this version`.
pub fn is_pinnable(inst: &Mod) -> bool {
    refusal(inst).is_none()
}

/// Display version of a frozen copy, or `None` when this mod is not one.
pub fn pinned_version(inst: &Mod) -> Option<String> {
    if inst.pin_of.is_none() {
        return None;
    }
    let url = pin_frozen_url(inst)?;
    name_offline(inst, &url).ok().map(|n| n.display)
}

/// Frozen download URL of a copy: the manual URL, or the literal GitHub
/// release URL for the pinned tag and asset. `None` when this is not a
/// copy or the source is still a moving glob.
pub(crate) fn pin_frozen_url(inst: &Mod) -> Option<String> {
    if inst.pin_of.is_none() {
        return None;
    }
    match &inst.source {
        SourceRef::ManualUrl { url } => Some(url.clone()),
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } if !*prerelease && !asset_glob.contains(['*', '?', '[']) => Some(
            crate::download::github_release_url(owner, repo, tag.as_deref(), asset_glob),
        ),
        _ => None,
    }
}

pub(crate) fn pin_unavailable(id: &str) -> String {
    format!("{id}: pinned version unavailable")
}

/// Duplicate `parent_id` into a user mod frozen to one named version.
/// `from_game` freezes that game's install record. `None` freezes the
/// catalog payload, or the recipe URL / one GitHub resolve when nothing
/// has been downloaded. A failure writes nothing.
pub async fn pin_version(
    config_dir: &Path,
    data_dir: &Path,
    parent_id: &str,
    from_game: Option<&str>,
) -> Result<PinResult> {
    let listed = list_mods(config_dir, data_dir)?;
    let parent = listed
        .mods
        .iter()
        .find(|m| m.id == parent_id)
        .cloned()
        .ok_or_else(|| Error::InvalidInstance(format!("unknown mod: {parent_id}")))?;
    if let Some(why) = refusal(&parent) {
        return Err(Error::InvalidInstance(format!("{parent_id}: {why}")));
    }
    let url = choose_url(data_dir, &parent, from_game).await?;
    let named = named_version(&parent, &url).await?;
    let frozen = frozen_of(&parent, &url, &named)?;
    if let Some(existing) = listed
        .mods
        .iter()
        .find(|m| same_frozen(m, parent_id, &frozen))
    {
        return Ok(PinResult {
            id: existing.id.clone(),
            label: existing.label.clone(),
            version: named.display.clone(),
            already: true,
        });
    }
    let slug = files::pin_slug(parent_id, &named.display)?;
    let id = files::free_id(&listed.mods, data_dir, &slug)?;
    let child = child_mod(&parent, &id, &named.display, &frozen);
    let text = export_recipe_text(&child, data_dir, None)?;
    fs::create_dir_all(user_mods_dir(data_dir))?;
    let dest = user_mods_dir(data_dir).join(format!("{id}.toml"));
    if let Err(e) = fs::write(&dest, &text) {
        let _ = fs::remove_file(&dest);
        return Err(e.into());
    }
    let copy_url = payload_copy_url(&parent, &frozen);
    if let Err(e) = files::copy_matching_payload(data_dir, &parent, &id, &copy_url) {
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_dir_all(payload_dir(data_dir, false, None, &id));
        crate::db::mark_cache_dirty();
        return Err(e);
    }
    crate::db::mark_cache_dirty();
    Ok(PinResult {
        id,
        label: child.label,
        version: named.display,
        already: false,
    })
}

fn refusal(inst: &Mod) -> Option<&'static str> {
    if inst.pin_of.is_some() {
        return Some("a pinned copy cannot be pinned");
    }
    if inst.sha256.as_ref().is_some_and(|s| !s.is_empty()) {
        return Some("recipe sha is not a user pin");
    }
    match &inst.source {
        SourceRef::Local { .. } => Some("local source cannot be pinned"),
        SourceRef::Provided { .. } => Some("provided source cannot be pinned"),
        SourceRef::Github { tag, .. } if tag.is_some() => Some("github tag is already pinned"),
        SourceRef::Github { .. } | SourceRef::ManualUrl { .. } => None,
    }
}

async fn choose_url(data_dir: &Path, parent: &Mod, from_game: Option<&str>) -> Result<String> {
    if let Some(game) = from_game {
        let manifest = crate::need_manifest(data_dir, game, &parent.id)
            .map_err(|e| Error::InvalidInstance(e.to_string()))?;
        if manifest.provenance.source.is_empty() {
            return Err(Error::InvalidInstance("empty manifest provenance".into()));
        }
        return Ok(manifest.provenance.source);
    }
    let payload = payload_dir(
        data_dir,
        parent.official,
        parent.registry.as_deref(),
        &parent.id,
    );
    if let Some(prov) = read_payload_provenance(&payload) {
        if !prov.source.is_empty() {
            return Ok(prov.source);
        }
    }
    match &parent.source {
        SourceRef::ManualUrl { url } => Ok(url.clone()),
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            crate::download::github_source_url(owner, repo, tag.as_deref(), *prerelease, asset_glob)
                .await
                .map_err(|e| Error::InvalidInstance(e.to_string()))
        }
        _ => Err(Error::InvalidInstance("not pinnable".into())),
    }
}

fn same_frozen(inst: &Mod, parent_id: &str, frozen: &Frozen) -> bool {
    if inst.pin_of.as_deref() != Some(parent_id) {
        return false;
    }
    match (&inst.source, frozen) {
        (SourceRef::ManualUrl { url }, Frozen::Manual(want)) => url == want,
        (
            SourceRef::Github {
                tag,
                asset_glob,
                prerelease,
                ..
            },
            Frozen::Github { tag: want, asset },
        ) => !*prerelease && tag.as_deref() == Some(want.as_str()) && asset_glob == asset,
        _ => false,
    }
}

/// URL the payload copy must already match. A latest-download record does
/// not match the tagged URL this freeze stores.
fn payload_copy_url(parent: &Mod, frozen: &Frozen) -> String {
    match frozen {
        Frozen::Manual(url) => url.clone(),
        Frozen::Github { tag, asset } => match &parent.source {
            SourceRef::Github { owner, repo, .. } => {
                crate::download::github_release_url(owner, repo, Some(tag), asset)
            }
            _ => String::new(),
        },
    }
}

fn child_mod(parent: &Mod, id: &str, display: &str, frozen: &Frozen) -> Mod {
    let mut child = parent.clone();
    child.id = id.to_string();
    child.label = format!("{} {display}", parent.label);
    child.description = String::new();
    child.sha256 = None;
    child.pin_of = Some(parent.id.clone());
    child.official = false;
    child.registry = None;
    child.enabled = true;
    child.source = match frozen {
        Frozen::Manual(url) => SourceRef::ManualUrl { url: url.clone() },
        Frozen::Github { tag, asset } => {
            let SourceRef::Github { owner, repo, .. } = &parent.source else {
                unreachable!("github freeze requires a github parent");
            };
            SourceRef::Github {
                owner: owner.clone(),
                repo: repo.clone(),
                asset_glob: asset.clone(),
                tag: Some(tag.clone()),
                prerelease: false,
            }
        }
    };
    child
}

#[cfg(test)]
mod tests {
    use super::super::testing::temp_config;
    use super::super::{list_mods, payload_dir, write_payload_provenance};
    use super::*;
    use std::fs;

    #[test]
    fn latest_provenance_is_not_copied_onto_a_tagged_pin() {
        let data = temp_config();
        let latest =
            "https://github.com/optiscaler/OptiScaler/releases/latest/download/OptiScaler_0.9.4.7z";
        let dir = payload_dir(&data, true, None, "optiscaler");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("payload.bin"), b"bytes").unwrap();
        write_payload_provenance(
            &dir,
            &crate::ModProvenance {
                source: latest.into(),
                asset_sha256: "ab".repeat(32),
                asset_bytes: 4,
                fetched_at: 1,
            },
        );
        let parent = list_mods(&data, &data)
            .unwrap()
            .mods
            .into_iter()
            .find(|m| m.id == "optiscaler")
            .unwrap();
        let frozen = name::Frozen::Github {
            tag: "v0.9.4".into(),
            asset: "OptiScaler_0.9.4.7z".into(),
        };
        let url = payload_copy_url(&parent, &frozen);
        assert_eq!(
            url,
            "https://github.com/optiscaler/OptiScaler/releases/download/v0.9.4/OptiScaler_0.9.4.7z"
        );
        files::copy_matching_payload(&data, &parent, "optiscaler-0-9-4", &url).unwrap();
        assert!(!payload_dir(&data, false, None, "optiscaler-0-9-4").exists());
        let _ = fs::remove_dir_all(&data);
    }
}
