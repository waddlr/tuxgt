use std::path::Path;

use super::*;
use crate::{Error, Result};

pub(crate) const EFFECT_PACKAGES_URL: &str =
    "https://raw.githubusercontent.com/crosire/reshade-shaders/list/EffectPackages.ini";
pub(crate) const ADDONS_INI_URL: &str =
    "https://raw.githubusercontent.com/crosire/reshade-shaders/list/Addons.ini";

/// One row from crosire’s Setup extras list (E96).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReshadePackage {
    pub kind: ReshadePackageKind,
    pub name: String,
    pub description: String,
    /// 64-bit URL (`DownloadUrl64` else `DownloadUrl`). None = listed, not mintable.
    pub url: Option<String>,
    /// 32-bit URL (`DownloadUrl32`) when present.
    pub url32: Option<String>,
    pub repository_url: Option<String>,
    pub shader_dir: Option<String>,
    pub texture_dir: Option<String>,
    /// `DenyEffectFiles` basenames.
    pub deny_files: Box<[String]>,
    /// `EffectFiles` basenames (extras preview, deny applied at preview time).
    pub effect_files: Box<[String]>,
    pub in_catalog: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReshadePackageKind {
    Effect,
    Addon,
}

impl ReshadePackageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Effect => "effect",
            Self::Addon => "addon",
        }
    }

    pub fn mod_type(self) -> &'static str {
        match self {
            Self::Effect => "effect",
            Self::Addon => "reshade_addon",
        }
    }
}

impl ReshadePackage {
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.name)
    }

    pub fn mintable(&self) -> bool {
        self.url.is_some() && !self.in_catalog
    }

    /// URL for a given arch: 32-bit prefers `DownloadUrl32` when present.
    pub fn url_for_arch(&self, arch: &str) -> Option<&String> {
        if arch == "32" {
            self.url32.as_ref().or(self.url.as_ref())
        } else {
            self.url.as_ref()
        }
    }
}

/// Live extras list (E96): GET both INIs (no cache), parse, mark already-in-catalog.
pub async fn list_reshade_packages(
    config_dir: &Path,
    data_dir: &Path,
) -> Result<Vec<ReshadePackage>> {
    let effects = fetch_text(EFFECT_PACKAGES_URL).await?;
    let addons = fetch_text(ADDONS_INI_URL).await?;
    let mut pkgs = parse_reshade_packages(&effects, &addons);
    let listed = crate::instance::list_mods(config_dir, data_dir)?;
    for p in &mut pkgs {
        p.in_catalog = package_in_catalog(p, &listed.mods);
    }
    Ok(pkgs)
}

pub(crate) async fn fetch_text(url: &str) -> Result<String> {
    let resp = client()?
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .map_err(|e| Error::Download(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Download(format!("{url}: HTTP {status}")));
    }
    resp.text()
        .await
        .map_err(|e| Error::Download(e.to_string()))
}

pub(crate) fn parse_reshade_packages(effects: &str, addons: &str) -> Vec<ReshadePackage> {
    let mut out = parse_ini_packages(effects, ReshadePackageKind::Effect);
    out.extend(parse_ini_packages(addons, ReshadePackageKind::Addon));
    out
}

pub(crate) struct IniRow {
    name: String,
    description: String,
    download: Option<String>,
    download64: Option<String>,
    download32: Option<String>,
    repository: Option<String>,
    install: Option<String>,
    texture_install: Option<String>,
    deny: Option<String>,
    effects: Option<String>,
}

impl IniRow {
    fn new() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            download: None,
            download64: None,
            download32: None,
            repository: None,
            install: None,
            texture_install: None,
            deny: None,
            effects: None,
        }
    }

    fn into_package(self, kind: ReshadePackageKind) -> Option<ReshadePackage> {
        if self.name.is_empty() {
            return None;
        }
        let url = self
            .download64
            .filter(|s| !s.is_empty())
            .or(self.download.filter(|s| !s.is_empty()));
        let url32 = self.download32.filter(|s| !s.is_empty());
        Some(ReshadePackage {
            kind,
            name: self.name,
            description: self.description,
            url,
            url32,
            repository_url: self.repository.filter(|s| !s.is_empty()),
            shader_dir: path_leaf_dir(self.install.as_deref(), "Shaders"),
            texture_dir: path_leaf_dir(self.texture_install.as_deref(), "Textures"),
            deny_files: csv_basenames(self.deny.as_deref().unwrap_or("")).into(),
            effect_files: csv_basenames(self.effects.as_deref().unwrap_or("")).into(),
            in_catalog: false,
        })
    }
}

pub(crate) fn parse_ini_packages(text: &str, kind: ReshadePackageKind) -> Vec<ReshadePackage> {
    let mut out = Vec::new();
    let mut cur = IniRow::new();
    let mut in_section = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if rest.strip_suffix(']').is_some() {
                if in_section {
                    if let Some(p) = std::mem::replace(&mut cur, IniRow::new()).into_package(kind) {
                        out.push(p);
                    }
                }
                in_section = true;
                continue;
            }
        }
        if !in_section {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim() {
            "PackageName" => cur.name = v,
            "PackageDescription" => cur.description = v,
            "DownloadUrl" => cur.download = Some(v),
            "DownloadUrl64" => cur.download64 = Some(v),
            "DownloadUrl32" => cur.download32 = Some(v),
            "RepositoryUrl" => cur.repository = Some(v),
            "InstallPath" => cur.install = Some(v),
            "TextureInstallPath" => cur.texture_install = Some(v),
            "DenyEffectFiles" => cur.deny = Some(v),
            "EffectFiles" => cur.effects = Some(v),
            _ => {}
        }
    }
    if in_section {
        if let Some(p) = cur.into_package(kind) {
            out.push(p);
        }
    }
    out
}

pub(crate) fn path_leaf_dir(path: Option<&str>, skip: &str) -> Option<String> {
    let path = path?.trim();
    if path.is_empty() {
        return None;
    }
    let trimmed = path.trim_start_matches('.').trim_matches(['\\', '/']);
    let leaf = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    if leaf.is_empty() || leaf.eq_ignore_ascii_case(skip) {
        return None;
    }
    Some(leaf.to_string())
}

pub(crate) fn csv_basenames(deny: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in deny.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let base = part.rsplit(['/', '\\']).next().unwrap_or(part);
        if !out.iter().any(|s| s == base) {
            out.push(base.to_string());
        }
    }
    out
}

pub(crate) fn deny_drop_globs(deny_files: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for base in deny_files {
        out.push(base.clone());
        out.push(format!("*/{base}"));
    }
    out
}

pub(crate) const PREVIEW_CAP: usize = 30;

pub(crate) fn cap_names(mut sorted: Vec<String>, cap: usize) -> (Vec<String>, usize) {
    let total = sorted.len();
    sorted.truncate(cap);
    (sorted, total)
}

/// Display preview of a sorted name list: the first `PREVIEW_CAP` names plus
/// the total, so callers can paint a `+ N more` tail.
pub fn cap_preview(names: Vec<String>) -> (Vec<String>, usize) {
    cap_names(names, PREVIEW_CAP)
}

/// Extras preview: EffectFiles minus deny globs, sorted, capped.
pub fn preview_effect_files(pkg: &ReshadePackage) -> (Vec<String>, usize) {
    let deny = deny_drop_globs(&pkg.deny_files);
    let mut names: Vec<String> = pkg
        .effect_files
        .iter()
        .filter(|f| !deny.iter().any(|g| glob_match(g, f)))
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    cap_preview(names)
}

pub(crate) fn github_path(url: &str) -> Option<&str> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("http://github.com/"))?;
    Some(rest.split(['?', '#']).next().unwrap_or(rest))
}

pub(crate) fn github_owner_repo(url: &str) -> Option<(String, String)> {
    let mut parts = github_path(url)?.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// GitHub release asset URL → owner, repo, optional tag, asset name.
pub(crate) fn parse_github_release_url(
    url: &str,
) -> Option<(String, String, Option<String>, String)> {
    let mut parts = github_path(url)?.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if parts.next()? != "releases" {
        return None;
    }
    let next = parts.next()?;
    let (tag, asset) = if next == "latest" {
        if parts.next()? != "download" {
            return None;
        }
        (None, parts.next()?.to_string())
    } else if next == "download" {
        let tag = parts.next()?.to_string();
        let asset = parts.next()?.to_string();
        (Some(tag), asset)
    } else {
        return None;
    };
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() || asset.is_empty() {
        return None;
    }
    if let Some(t) = tag.as_deref() {
        if t.is_empty() {
            return None;
        }
    }
    Some((owner, repo, tag, asset))
}

pub(crate) fn package_in_catalog(pkg: &ReshadePackage, listed: &[crate::instance::Mod]) -> bool {
    let pkg_gh = pkg
        .repository_url
        .as_deref()
        .and_then(github_owner_repo)
        .or_else(|| pkg.url.as_deref().and_then(github_owner_repo));
    let pkg_asset = pkg.url.as_deref().and_then(parse_github_release_url);
    for m in listed {
        match &m.source {
            crate::instance::SourceRef::Github {
                owner,
                repo,
                asset_glob,
                ..
            } => {
                let Some((o, r)) = pkg_gh.as_ref() else {
                    continue;
                };
                if !o.eq_ignore_ascii_case(owner) || !r.eq_ignore_ascii_case(repo) {
                    continue;
                }
                if let Some((_, _, _, asset)) = &pkg_asset {
                    if glob_match(asset_glob, asset) || asset_glob == asset {
                        return true;
                    }
                    // Same repo, different release asset (cot6 addons).
                    continue;
                }
                // Archive / non-release URL for a listed github repo.
                return true;
            }
            crate::instance::SourceRef::ManualUrl { url } => {
                if pkg.url.as_deref() == Some(url.as_str()) {
                    return true;
                }
            }
            crate::instance::SourceRef::Local { .. } => {}
        }
    }
    false
}
