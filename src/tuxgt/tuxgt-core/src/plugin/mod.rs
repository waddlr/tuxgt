use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const PLUGINS_TOML: &str = "plugins.toml";

macro_rules! first_party {
    ($name:literal) => {
        PluginDesc {
            registry: None,
            name: $name,
            tag: None,
            label_id: concat!("plugin-", $name, "-label"),
            requires: None,
        }
    };
}

const DESC_STEAM: PluginDesc = first_party!("steam");

const DESC_HEROIC: PluginDesc = first_party!("heroic");

const DESC_MANUAL: PluginDesc = first_party!("manual");

const DESC_ENV: PluginDesc = first_party!("env");

const DESC_PROTONDB: PluginDesc = first_party!("protondb");

const DESC_STEAMGRIDDB: PluginDesc = first_party!("steamgriddb");

const DESC_AWACY: PluginDesc = first_party!("awacy");

const DESC_WRAPPER: PluginDesc = first_party!("wrapper");

/// In-tree first-party plugins.
pub const FIRST_PARTY: &[PluginDesc] = &[
    DESC_STEAM,
    DESC_HEROIC,
    DESC_MANUAL,
    DESC_ENV,
    DESC_PROTONDB,
    DESC_STEAMGRIDDB,
    DESC_AWACY,
    DESC_WRAPPER,
];

/// Static descriptor. Fields may be added in later execs.
///
/// `requires`: `None` = no plugin-level deps; `Some` is never empty.
#[derive(Clone, Copy, Debug)]
pub struct PluginDesc {
    pub registry: Option<&'static str>,
    pub name: &'static str,
    pub tag: Option<&'static str>,
    pub label_id: &'static str,
    pub requires: Option<&'static [&'static str]>,
}

impl PluginDesc {
    pub fn id(&self) -> PluginId {
        PluginId {
            registry: self.registry.map(str::to_string),
            name: self.name.to_string(),
            tag: self.tag.map(str::to_string),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginId {
    pub registry: Option<String>,
    pub name: String,
    pub tag: Option<String>,
}

impl PluginId {
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::InvalidPluginId(s.into()));
        }
        let (main, tag) = match s.rsplit_once(':') {
            Some((main, tag)) => {
                if main.is_empty() || tag.is_empty() || !valid_tag(tag) {
                    return Err(Error::InvalidPluginId(s.into()));
                }
                (main, Some(tag.to_string()))
            }
            None => (s, None),
        };
        let (registry, name) = match main.rsplit_once('/') {
            Some((reg, name)) => (Some(reg), name),
            None => (None, main),
        };
        if !valid_name(name) || name == "core" {
            return Err(Error::InvalidPluginId(s.into()));
        }
        if let Some(reg) = registry {
            if !valid_registry(reg) {
                return Err(Error::InvalidPluginId(s.into()));
            }
        }
        Ok(Self {
            registry: registry.map(str::to_string),
            name: name.to_string(),
            tag,
        })
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.registry, &self.tag) {
            (Some(reg), Some(tag)) => write!(f, "{reg}/{name}:{tag}", name = self.name),
            (Some(reg), None) => write!(f, "{reg}/{name}", name = self.name),
            (None, Some(tag)) => write!(f, "{name}:{tag}", name = self.name),
            (None, None) => write!(f, "{name}", name = self.name),
        }
    }
}

fn valid_name(n: &str) -> bool {
    let mut chars = n.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && n.len() <= 32
}

fn valid_tag(t: &str) -> bool {
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

fn valid_registry(r: &str) -> bool {
    !r.contains(':') && r.contains('/') && r.split('/').all(|seg| !seg.is_empty())
}

pub struct PluginEntry<'a> {
    pub desc: &'a PluginDesc,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct PluginHost {
    descs: Box<[PluginDesc]>,
    disabled: BTreeSet<String>,
    config_path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PluginsFile {
    #[serde(default)]
    disabled: Vec<String>,
}

impl PluginHost {
    pub fn load() -> Result<Self> {
        Self::load_with(FIRST_PARTY, crate::config_dir())
    }

    pub fn load_with(descs: &[PluginDesc], config_dir: impl AsRef<Path>) -> Result<Self> {
        let mut kept = Vec::new();
        let mut seen = BTreeMap::new();
        for (i, d) in descs.iter().enumerate() {
            let id = d.id().to_string();
            if let Some(&kept_index) = seen.get(&id) {
                let kept_desc: &PluginDesc = &descs[kept_index];
                tracing::warn!(
                    plugin_id = %id,
                    duplicate_index = i,
                    kept_index,
                    duplicate_label_id = d.label_id,
                    kept_label_id = kept_desc.label_id,
                    "duplicate plugin id; keeping first descriptor, ignoring this one"
                );
                continue;
            }
            validate_desc(d)?;
            seen.insert(id, i);
            kept.push(*d);
        }
        let config_path = config_dir.as_ref().join(PLUGINS_TOML);
        let disabled = read_disabled(&config_path)?;
        Ok(Self {
            descs: kept.into_boxed_slice(),
            disabled,
            config_path,
        })
    }

    pub fn list(&self) -> Vec<PluginEntry<'_>> {
        self.descs
            .iter()
            .map(|desc| {
                let id = desc.id().to_string();
                PluginEntry {
                    desc,
                    enabled: !self.disabled.contains(&id),
                }
            })
            .collect()
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        let Ok(parsed) = PluginId::parse(id) else {
            return false;
        };
        let key = parsed.to_string();
        self.descs.iter().any(|d| d.id().to_string() == key) && !self.disabled.contains(&key)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        let key = PluginId::parse(id)?.to_string();
        if !self.descs.iter().any(|d| d.id().to_string() == key) {
            return Err(Error::UnknownPlugin(key));
        }
        let mut next = self.disabled.clone();
        if enabled {
            next.remove(&key);
        } else {
            next.insert(key);
        }
        self.write_disabled(&next)?;
        self.disabled = next;
        Ok(())
    }

    fn write_disabled(&self, disabled: &BTreeSet<String>) -> Result<()> {
        if let Some(dir) = self.config_path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = PluginsFile {
            disabled: disabled.iter().cloned().collect(),
        };
        let text = toml::to_string(&file).map_err(|e| Error::Toml(e.to_string()))?;
        fs::write(&self.config_path, text)?;
        Ok(())
    }
}

fn read_disabled(path: &Path) -> Result<BTreeSet<String>> {
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let text = fs::read_to_string(path)?;
    let file: PluginsFile = toml::from_str(&text).map_err(|e| Error::Toml(e.to_string()))?;
    Ok(file.disabled.into_iter().collect())
}

fn validate_desc(d: &PluginDesc) -> Result<()> {
    if d.registry.is_some() {
        return Err(Error::InvalidPluginDesc(format!(
            "{}: FirstParty omits registry",
            d.name
        )));
    }
    let expect_label = format!("plugin-{}-label", d.name);
    if d.label_id != expect_label {
        return Err(Error::InvalidPluginDesc(format!(
            "{}: label_id must be {expect_label}",
            d.name
        )));
    }
    if d.label_id.is_empty() {
        return Err(Error::InvalidPluginDesc(format!(
            "{}: empty label_id",
            d.name
        )));
    }
    match d.requires {
        None => {}
        Some([]) => {
            return Err(Error::InvalidPluginDesc(format!(
                "{}: requires is empty (use None)",
                d.name
            )));
        }
        Some(deps) => {
            for dep in deps {
                PluginId::parse(dep)?;
            }
        }
    }
    let id = d.id();
    let parsed = PluginId::parse(&id.to_string())?;
    if parsed != id {
        return Err(Error::InvalidPluginDesc(format!("{id}: id round-trip")));
    }
    Ok(())
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests_0;
