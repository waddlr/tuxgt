use std::path::Path;

use super::*;
use crate::{Error, Result};

#[derive(serde::Deserialize, Clone)]
pub(crate) struct GhAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(serde::Deserialize, Clone)]
pub(crate) struct GhRelease {
    #[serde(default)]
    pub(crate) tag_name: String,
    /// Release list entries only; the single-release endpoints omit it.
    /// Rolling nightlies share a workflow-run `created_at`, so this decides.
    #[serde(default)]
    pub(crate) published_at: String,
    /// Release list entries only; the single-release endpoints omit it.
    #[serde(default)]
    pub(crate) created_at: String,
    /// Unauthenticated list responses exclude drafts; the flag is checked
    /// anyway so a future token-scoped fetch cannot mint from one.
    #[serde(default)]
    pub(crate) draft: bool,
    #[serde(default)]
    pub(crate) assets: Box<[GhAsset]>,
}

/// Newest non-draft release from the `GET /releases` list (E63). Rolling
/// nightlies share a workflow-run `created_at`, so the newest `published_at`
/// entry wins (`created_at` as fallback); the result is order-explicit.
pub(crate) fn pick_prerelease_release(rels: &[GhRelease]) -> Option<&GhRelease> {
    rels.iter().filter(|r| !r.draft).max_by(|a, b| {
        (a.published_at.as_str(), a.created_at.as_str())
            .cmp(&(b.published_at.as_str(), b.created_at.as_str()))
    })
}

/// Newest sorted-name hit wins; zero hits is a Download error naming the glob.
/// Name comparison is ASCII case-insensitive: upstreams are sloppy
/// (`OptiScaler_*.7z` vs `Optiscaler_0.9.4-…`) and a missed case variant must
/// not read as "no release".
pub(crate) fn pick_release_asset(assets: &[GhAsset], asset_glob: &str) -> Result<String> {
    let want = asset_glob.to_lowercase();
    let mut hits: Vec<&GhAsset> = assets
        .iter()
        .filter(|a| glob_match(&want, &a.name.to_lowercase()))
        .collect();
    hits.sort_by(|a, b| a.name.cmp(&b.name));
    let Some(top) = hits.pop() else {
        return Err(Error::Download(format!(
            "{asset_glob}: no release asset matches"
        )));
    };
    tracing::info!(url = %top.browser_download_url, hits = hits.len() + 1, "github asset resolved");
    Ok(top.browser_download_url.clone())
}

/// Literal asset download URL: pinned tag, else GitHub's latest release.
pub(crate) fn github_release_url(
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    asset: &str,
) -> String {
    match tag {
        Some(tag) => {
            format!("https://github.com/{owner}/{repo}/releases/download/{tag}/{asset}")
        }
        None => {
            format!("https://github.com/{owner}/{repo}/releases/latest/download/{asset}")
        }
    }
}

/// Fetch one release JSON: a pinned tag, GitHub's latest non-prerelease
/// release, or — for `prerelease` (E63) — the newest non-draft entry of the
/// release list (nightlies publish only prereleases, so `/releases/latest`
/// 404s). A vanished pinned tag names re-minting from the family.
pub(crate) async fn github_release(
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    prerelease: bool,
) -> Result<GhRelease> {
    let api = match tag {
        Some(tag) => format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"),
        None if prerelease => {
            // Newest-first list; the pick takes the newest non-draft
            // entry. per_page=100 504s on repos with huge asset lists
            // (RenoDX nightlies).
            format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=10")
        }
        None => format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
    };
    let resp = client()?
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| Error::Download(e.to_string()))?;
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(Error::Download(format!(
            "{owner}/{repo}: GitHub release API rate-limited (HTTP {status}); retry later"
        )));
    }
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND && tag.is_some() {
            return Err(Error::Download(format!(
                "{api}: HTTP 404 (release tag vanished; re-pin or re-mint from the family)"
            )));
        }
        return Err(Error::Download(format!("{api}: HTTP {status}")));
    }
    if tag.is_none() && prerelease {
        let rels: Vec<GhRelease> = resp
            .json()
            .await
            .map_err(|e| Error::Download(e.to_string()))?;
        return pick_prerelease_release(&rels)
            .cloned()
            .ok_or_else(|| Error::Download(format!("{owner}/{repo}: no non-draft release found")));
    }
    resp.json()
        .await
        .map_err(|e| Error::Download(e.to_string()))
}

pub(crate) async fn github_asset_url(
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    prerelease: bool,
    asset_glob: &str,
) -> Result<String> {
    let rel = github_release(owner, repo, tag, prerelease).await?;
    pick_release_asset(&rel.assets, asset_glob)
}

/// Download URL for a github source. Literal assets with a pin or on the
/// latest-stable path are direct download URLs; everything else resolves
/// through the release API.
pub(crate) async fn github_source_url(
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    prerelease: bool,
    asset_glob: &str,
) -> Result<String> {
    let literal = !asset_glob.contains(['*', '?', '[']);
    if literal && (tag.is_some() || !prerelease) {
        return Ok(github_release_url(owner, repo, tag, asset_glob));
    }
    github_asset_url(owner, repo, tag, prerelease, asset_glob).await
}

/// One listed asset of a family release (E63): the asset file name and the
/// release tag it ships in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FamilyAsset {
    pub name: String,
    pub tag: String,
}

/// Live asset list of one family template (E63): resolves the family's
/// release exactly as an install would (prerelease-aware) and lists its
/// assets matching the template glob (case-insensitive), sorted by name.
pub async fn list_family_assets(data_dir: &Path, template_id: &str) -> Result<Vec<FamilyAsset>> {
    let tpl = crate::instance::list_templates(data_dir)?
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| Error::InvalidInstance(format!("unknown template: {template_id}")))?;
    let Some(family) = tpl.family else {
        return Err(Error::InvalidInstance(format!(
            "{template_id}: not a family template"
        )));
    };
    list_family_assets_for(&family).await
}
/// Concurrent live asset lists for several family templates: the catalog
/// reads once, then one release fetch per family joins. Each id keeps its
/// own error (unknown id, non-family template, or its vendor's API
/// failure), so a reachable list still paints with the other's error shown.
pub async fn list_family_assets_many(
    data_dir: &Path,
    template_ids: &[String],
) -> Vec<(String, Result<Vec<FamilyAsset>>)> {
    let list = match crate::instance::list_templates(data_dir) {
        Ok(list) => list,
        Err(e) => {
            let msg = e.to_string();
            return template_ids
                .iter()
                .map(|tid| (tid.clone(), Err(Error::InvalidInstance(msg.clone()))))
                .collect();
        }
    };
    let futs = template_ids.iter().map(|tid| {
        let tid2 = tid.clone();
        let resolved: Result<crate::instance::TemplateFamily> =
            match list.iter().find(|t| t.id == *tid) {
                None => Err(Error::InvalidInstance(format!("unknown template: {tid}"))),
                Some(t) => match &t.family {
                    Some(f) => Ok(f.clone()),
                    None => Err(Error::InvalidInstance(format!(
                        "{tid}: not a family template"
                    ))),
                },
            };
        async move {
            match resolved {
                Ok(family) => {
                    let out = list_family_assets_for(&family).await;
                    (tid2, out)
                }
                Err(e) => (tid2, Err(e)),
            }
        }
    });
    futures_util::future::join_all(futs).await
}
pub(crate) async fn list_family_assets_for(
    family: &crate::instance::TemplateFamily,
) -> Result<Vec<FamilyAsset>> {
    let rel = github_release(&family.owner, &family.repo, None, family.prerelease).await?;
    let want = family.asset_glob.to_lowercase();
    let mut out: Vec<FamilyAsset> = rel
        .assets
        .iter()
        .filter(|a| glob_match(&want, &a.name.to_lowercase()))
        .map(|a| FamilyAsset {
            name: a.name.clone(),
            tag: rel.tag_name.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
