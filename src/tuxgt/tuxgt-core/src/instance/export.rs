use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::*;
use crate::{Error, Result};

/// Export a user Mod recipe as TOML, or as a tar.gz bundling the kept payload files.
pub fn export_mod(
    config_dir: &Path,
    data_dir: &Path,
    id: &str,
    out: &Path,
    files: bool,
) -> Result<()> {
    let listed = list_mods(config_dir, data_dir)?;
    let inst = listed
        .mods
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::UnknownInstance(id.into()))?;
    if inst.official {
        return Err(Error::InvalidInstance(format!(
            "{id}: cannot export an official mod"
        )));
    }
    if let Some(reg) = inst.registry.as_deref() {
        return Err(Error::InvalidInstance(format!(
            "{id}: cannot export a registry mod ({reg})"
        )));
    }
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    if !files {
        let text = export_recipe_text(inst, data_dir, None)?;
        fs::write(out, text)?;
        return Ok(());
    }
    let SourceRef::Local { path } = &inst.source else {
        return Err(Error::InvalidInstance(format!(
            "{id}: no local payload to bundle (remote source exports recipe-only)"
        )));
    };
    let stored = Path::new(path);
    if !stored.exists() {
        return Err(Error::InvalidInstance(format!(
            "{id}: local payload missing: {}",
            stored.display()
        )));
    }
    export_tar_present()?;
    // `tar -czf` resolves `out` against its cwd, so absolutize before staging.
    let out_abs = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir()?.join(out)
    };
    let stage = export_stage_dir(id);
    let _ = fs::remove_dir_all(&stage);
    let result = (|| {
        fs::create_dir_all(&stage)?;
        // Bundle path the archived TOML points at (relative to the archive root).
        let bundle = if stored.is_file() {
            let name = stored.file_name().ok_or_else(|| {
                Error::InvalidInstance(format!("no file name: {}", stored.display()))
            })?;
            let rel = Path::new("payload").join(name);
            fs::create_dir_all(stage.join("payload"))?;
            fs::copy(stored, stage.join(&rel))?;
            rel.to_string_lossy().into_owned()
        } else {
            bundle_payload_tree(id, &inst.payload, stored, &stage.join("payload"))?;
            "payload".to_string()
        };
        let text = export_recipe_text(inst, data_dir, Some(&bundle))?;
        let name = format!("{id}.toml");
        fs::write(stage.join(&name), text)?;
        if out_abs.exists() {
            fs::remove_file(&out_abs)?;
        }
        let out_arg = out_abs.to_string_lossy().into_owned();
        let status = std::process::Command::new("tar")
            .args(["-czf", &out_arg, &name, "payload"])
            .current_dir(&stage)
            .status()?;
        if !status.success() {
            return Err(Error::Unpack(format!("tar exited {status}")));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

pub(crate) fn export_tar_present() -> Result<()> {
    let probe = std::process::Command::new("tar").arg("--version").output();
    match probe {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(Error::MissingTool("tar".into())),
    }
}

pub(crate) fn export_stage_dir(id: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("tuxgt-export-{id}-{}-{nanos}", std::process::id()))
}

pub(crate) fn bundle_payload_tree(
    id: &str,
    payload: &[PayloadRule],
    stored: &Path,
    dest: &Path,
) -> Result<()> {
    let mut keep: Vec<&str> = Vec::new();
    for rule in payload {
        keep.extend(rule.keep.iter().map(String::as_str));
    }
    keep.sort();
    keep.dedup();
    if keep.is_empty() {
        crate::download::copy_tree(stored, dest)?;
        return Ok(());
    }
    for src in keep {
        let rel = Path::new(src);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::InvalidInstance(format!(
                "{id}: payload src escapes: {src}"
            )));
        }
        if src.contains(['*', '?', '[']) {
            bundle_glob_srcs(id, stored, src, dest)?;
            continue;
        }
        let from = stored.join(rel);
        let to = dest.join(rel);
        if from.is_dir() {
            crate::download::copy_tree(&from, &to)?;
        } else if from.is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        } else {
            return Err(Error::InvalidInstance(format!(
                "{id}: payload src missing: {src}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn bundle_glob_srcs(id: &str, stored: &Path, pat: &str, dest: &Path) -> Result<()> {
    let mut matched = 0usize;
    let mut stack = vec![stored.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in fs::read_dir(&dir)? {
            let ent = ent?;
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if !p.is_file() {
                continue;
            }
            let rel = p
                .strip_prefix(stored)
                .map_err(|e| Error::InvalidInstance(format!("{id}: bundle walk: {e}")))?;
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if crate::download::glob_match(pat, &rel_str) {
                let to = dest.join(rel);
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&p, &to)?;
                matched += 1;
            }
        }
    }
    if matched == 0 {
        return Err(Error::InvalidInstance(format!(
            "{id}: payload keep matches nothing: {pat}"
        )));
    }
    Ok(())
}

pub(crate) fn export_slice_empty<T>(s: &[T]) -> bool {
    s.is_empty()
}

pub(crate) fn export_map_empty<K, V>(m: &BTreeMap<K, V>) -> bool {
    m.is_empty()
}

pub(crate) fn export_opt_none<T>(o: &Option<T>) -> bool {
    o.is_none()
}

pub(crate) fn export_is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize)]
pub(crate) struct ExportRecipeToml<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    mod_type: &'a str,
    label: &'a str,
    plans_allowed: Vec<&'static str>,
    #[serde(skip_serializing_if = "export_slice_empty")]
    games: &'a [String],
    #[serde(skip_serializing_if = "export_slice_empty")]
    appids: &'a [u32],
    #[serde(skip_serializing_if = "export_slice_empty")]
    requires: &'a [String],
    #[serde(skip_serializing_if = "export_slice_empty")]
    payload: &'a [PayloadRule],
    #[serde(skip_serializing_if = "export_map_empty")]
    dests: &'a BTreeMap<String, String>,
    #[serde(skip_serializing_if = "export_opt_none")]
    slot: &'a Option<String>,
    #[serde(skip_serializing_if = "export_slice_empty")]
    include: &'a [String],
    #[serde(skip_serializing_if = "export_map_empty")]
    env: &'a BTreeMap<String, String>,
    #[serde(skip_serializing_if = "export_opt_none")]
    sha256: &'a Option<String>,
    #[serde(skip_serializing_if = "export_opt_none")]
    shader_dir: &'a Option<String>,
    #[serde(skip_serializing_if = "export_opt_none")]
    texture_dir: &'a Option<String>,
    #[serde(skip_serializing_if = "export_slice_empty")]
    effect_files: &'a [String],
    source: ExportSourceToml<'a>,
}

#[derive(Serialize)]
pub(crate) struct ExportSourceToml<'a> {
    #[serde(rename = "type")]
    source_type: &'static str,
    #[serde(skip_serializing_if = "export_opt_none")]
    owner: Option<&'a str>,
    #[serde(skip_serializing_if = "export_opt_none")]
    repo: Option<&'a str>,
    #[serde(skip_serializing_if = "export_opt_none")]
    asset_glob: Option<&'a str>,
    #[serde(skip_serializing_if = "export_opt_none")]
    tag: Option<&'a str>,
    #[serde(skip_serializing_if = "export_opt_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "export_opt_none")]
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "export_is_false")]
    prerelease: bool,
}

pub(crate) fn export_recipe_text(
    inst: &Mod,
    data_dir: &Path,
    bundle: Option<&str>,
) -> Result<String> {
    let source = match &inst.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => ExportSourceToml {
            source_type: "github",
            owner: Some(owner.as_str()),
            repo: Some(repo.as_str()),
            asset_glob: Some(asset_glob.as_str()),
            tag: tag.as_deref(),
            path: None,
            url: None,
            prerelease: *prerelease,
        },
        SourceRef::Local { path } => {
            let rel = match bundle {
                Some(b) => b.to_string(),
                None => match Path::new(path).strip_prefix(data_dir) {
                    Ok(rel) => rel.to_string_lossy().into_owned(),
                    Err(_) => path.clone(),
                },
            };
            ExportSourceToml {
                source_type: "local",
                owner: None,
                repo: None,
                asset_glob: None,
                tag: None,
                path: Some(rel),
                url: None,
                prerelease: false,
            }
        }
        SourceRef::ManualUrl { url } => ExportSourceToml {
            source_type: "manual_url",
            owner: None,
            repo: None,
            asset_glob: None,
            tag: None,
            path: None,
            url: Some(url.as_str()),
            prerelease: false,
        },
    };
    let out = ExportRecipeToml {
        id: &inst.id,
        mod_type: &inst.mod_type,
        label: &inst.label,
        plans_allowed: inst.plans_allowed.iter().map(|p| p.as_str()).collect(),
        games: &inst.games,
        appids: &inst.appids,
        requires: &inst.requires,
        payload: &inst.payload,
        dests: &inst.dests,
        slot: &inst.slot,
        include: &inst.include,
        env: &inst.env,
        sha256: &inst.sha256,
        shader_dir: &inst.shader_dir,
        texture_dir: &inst.texture_dir,
        effect_files: &inst.effect_files,
        source,
    };
    toml::to_string(&out).map_err(|e| Error::Toml(e.to_string()))
}
