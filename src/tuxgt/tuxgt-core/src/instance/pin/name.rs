//! Name the frozen version from a concrete download URL. A `/releases/latest/`
//! URL is the only path that talks to GitHub, and only to learn `tag_name`.

use crate::instance::{Mod, SourceRef};
use crate::{Error, Result};

pub(super) struct Named {
    pub(super) display: String,
    pub(super) tag: Option<String>,
    pub(super) asset: Option<String>,
}

pub(super) enum Frozen {
    Manual(String),
    Github { tag: String, asset: String },
}

pub(super) async fn named_version(parent: &Mod, url: &str) -> Result<Named> {
    if let SourceRef::Github {
        owner,
        repo,
        prerelease,
        ..
    } = &parent.source
    {
        if let Some(parts) = github_parts(url) {
            if parts.tag.is_none() {
                let rel = crate::download::github_release(owner, repo, None, *prerelease)
                    .await
                    .map_err(|e| Error::InvalidInstance(e.to_string()))?;
                if rel.tag_name.is_empty() {
                    return Err(Error::InvalidInstance("cannot name pinned version".into()));
                }
                let concrete = format!(
                    "https://github.com/{owner}/{repo}/releases/download/{}/{}",
                    rel.tag_name, parts.asset
                );
                return name_offline(parent, &concrete);
            }
        }
    }
    name_offline(parent, url)
}

pub(super) fn name_offline(parent: &Mod, url: &str) -> Result<Named> {
    if matches!(parent.source, SourceRef::ManualUrl { .. }) {
        if let Some(ver) = reshade_setup_version(url) {
            return Ok(Named {
                display: ver,
                tag: None,
                asset: None,
            });
        }
        let stem = url_stem(url)
            .ok_or_else(|| Error::InvalidInstance("cannot name pinned version".into()))?;
        return Ok(Named {
            display: display_version(&stem),
            tag: None,
            asset: None,
        });
    }
    if !matches!(parent.source, SourceRef::Github { .. }) {
        return Err(Error::InvalidInstance("cannot name pinned version".into()));
    }
    let parts = github_parts(url)
        .ok_or_else(|| Error::InvalidInstance("cannot name pinned version".into()))?;
    let Some(tag) = parts.tag else {
        return Err(Error::InvalidInstance("cannot name pinned version".into()));
    };
    if !good_tag(&tag) || parts.asset.contains(['*', '?', '[']) {
        return Err(Error::InvalidInstance("cannot name pinned version".into()));
    }
    Ok(Named {
        display: display_version(&tag),
        tag: Some(tag),
        asset: Some(parts.asset),
    })
}

pub(super) fn frozen_of(parent: &Mod, url: &str, named: &Named) -> Result<Frozen> {
    match &parent.source {
        SourceRef::ManualUrl { .. } => Ok(Frozen::Manual(url.to_string())),
        SourceRef::Github { .. } => Ok(Frozen::Github {
            tag: named
                .tag
                .clone()
                .ok_or_else(|| Error::InvalidInstance("cannot name pinned version".into()))?,
            asset: named
                .asset
                .clone()
                .ok_or_else(|| Error::InvalidInstance("cannot name pinned version".into()))?,
        }),
        _ => Err(Error::InvalidInstance("not pinnable".into())),
    }
}

fn reshade_setup_version(url: &str) -> Option<String> {
    let rest = url.split("ReShade_Setup_").nth(1)?;
    let ver: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let ver = ver.trim_matches('.').to_string();
    if ver.is_empty() || !ver.chars().any(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(ver)
    }
}

fn display_version(raw: &str) -> String {
    let mut chars = raw.chars();
    if chars.next() == Some('v') && chars.next().is_some_and(|c| c.is_ascii_digit()) {
        raw[1..].to_string()
    } else {
        raw.to_string()
    }
}

fn url_stem(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().filter(|s| !s.is_empty())?;
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    if stem.is_empty() || is_hex64(stem) {
        None
    } else {
        Some(stem.to_string())
    }
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

struct GhParts {
    tag: Option<String>,
    asset: String,
}

fn github_parts(url: &str) -> Option<GhParts> {
    let url = url.split(['?', '#']).next().unwrap_or(url);
    if let Some(rest) = url.split("/releases/download/").nth(1) {
        let mut parts = rest.split('/');
        let tag = parts.next().filter(|s| !s.is_empty())?;
        let asset = parts.next().filter(|s| !s.is_empty())?;
        if parts.next().is_some() {
            return None;
        }
        return Some(GhParts {
            tag: Some(tag.to_string()),
            asset: asset.to_string(),
        });
    }
    let rest = url.split("/releases/latest/download/").nth(1)?;
    let asset = rest.split('/').next().filter(|s| !s.is_empty())?;
    if rest.split('/').nth(1).is_some() {
        return None;
    }
    Some(GhParts {
        tag: None,
        asset: asset.to_string(),
    })
}

fn good_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
