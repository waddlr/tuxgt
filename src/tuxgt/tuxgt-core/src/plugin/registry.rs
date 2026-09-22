//! Git-backed plugin registry: manifest, trust v1, cache, install/update.
//!
//! Catalog scope only (R14, `docs/dev/app/gui/plugin-registry.md`). A
//! registry is a git repo plus a manifest path; refs resolve to an
//! immutable commit SHA before any bytes move, payloads are subtrees of
//! the same repo, and every install verifies the manifest tree digest
//! before the lock updates. Installed payloads are inert data: nothing
//! here marks or launches them, and remote enable state lives in
//! `remote.toml` beside — never inside — first-party `plugins.toml`.
//!
//! Trust v1 is HTTPS + SHA pinning + digest verification. It does not
//! authenticate the author: Ed25519 signing is Phase 0 Later.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use super::PluginId;
use crate::download::{now_unix, sha256_hex, tree_digest};
use crate::fs::atomic_write;
use crate::{Error, Result};

/// Manifest schema version this build accepts.
pub const REGISTRY_MANIFEST_VERSION: u32 = 1;
/// Plugin ABI major this build accepts. Mismatch is a hard refusal.
pub const REGISTRY_ABI_MAJOR: u32 = 1;

const DEFAULT_MANIFEST: &str = "registry.toml";
const DEFAULT_REF: &str = "HEAD";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PAYLOAD_FILES: usize = 2048;
const MAX_PAYLOAD_BYTES: u64 = 256 * 1024 * 1024;
const MAX_REGISTRIES: usize = 32;
const MAX_LABEL_CHARS: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 512;
const MAX_AUTHOR_CHARS: usize = 64;

/// A registry the user added: where to fetch, which manifest, and the
/// commit every read resolves against.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryEntry {
    pub url: String,
    pub manifest_path: String,
    /// Discovery input (branch/tag) — never the installed identity.
    pub discovery_ref: String,
    /// Immutable commit every read and install resolves against.
    pub pinned: String,
    /// Dev provenance: a non-HTTPS URL the user opted into explicitly.
    pub local: bool,
    pub added_at: u64,
}

/// A verified installed plugin: exact registry URL, commit, payload path,
/// tree digest, and ABI at install time.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstalledEntry {
    pub id: String,
    pub registry: String,
    pub pinned: String,
    pub path: String,
    pub sha256: String,
    pub abi_major: u32,
    pub installed_at: u64,
}

/// Registry manifest. Unknown fields are rejected: an unrecognized key
/// means the registry is ahead of this build, not that it is benign.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryManifest {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub plugins: Vec<RegistryPlugin>,
}

/// Tree digest of a payload path in the git repo containing the current
/// directory, at `HEAD`. This is exactly the digest `install` verifies,
/// so a maintainer can author a manifest entry without reimplementing
/// the digest. Nothing is written outside a temp dir.
pub fn payload_digest_in_repo(path: &str) -> Result<(String, u64)> {
    validate_rel_path(path)?;
    let repo = repo_root()?;
    let pin = git(&repo, &["rev-parse", "HEAD"], "rev-parse")?
        .trim()
        .to_string();
    if pin.len() != 40 {
        return Err(Error::Registry("HEAD does not resolve to a commit".into()));
    }
    let stage = private_temp_dir("tuxgt-registry-digest")?;
    let result = (|| {
        let tar = stage.join("payload.tar");
        archive(&repo, &pin, path, &tar)?;
        let dest = stage.join("payload");
        fs::create_dir_all(&dest)?;
        untar(&tar, &dest)?;
        tree_digest(&dest)
    })();
    let _ = fs::remove_dir_all(&stage);
    result
}

fn repo_root() -> Result<PathBuf> {
    let out = git(
        Path::new("."),
        &["rev-parse", "--show-toplevel"],
        "rev-parse",
    )?;
    let root = out.trim();
    if root.is_empty() {
        return Err(Error::Registry("not inside a git repository".into()));
    }
    Ok(PathBuf::from(root))
}

/// Create a private temp directory nobody else can pre-plant. The name
/// is guessable (pid + counter), so ownership is taken exclusively:
/// `create_dir` fails if the path already exists — including a symlink
/// another local user planted — and 0700 keeps the contents out of
/// reach. The caller removes the directory.
fn private_temp_dir(prefix: &str) -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt as _;
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for attempt in 0..64u32 {
        let path = base.join(format!("{prefix}-{pid}-{attempt}"));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::Registry(format!(
        "could not create a private temp dir for {prefix}"
    )))
}

/// One catalog entry: a payload subtree inside the registry repo.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]

pub struct RegistryPlugin {
    /// Display id, `PluginId` grammar without a tag (the commit is the
    /// version).
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Repo-relative payload subtree at the pinned commit.
    pub path: String,
    /// `download::tree_digest` hex of `path`.
    pub sha256: String,
    pub abi_major: u32,
}

/// One registry plus its manifest rows, resolved from the local cache
/// only. `error` is the reason a registry could not be read; its rows
/// are then empty rather than guessed.
pub struct RegistryView {
    pub entry: RegistryEntry,
    pub plugins: Vec<RegistryPlugin>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RemoteFile {
    #[serde(default)]
    registries: Vec<RegistryEntry>,
    #[serde(default)]
    installed: Vec<InstalledEntry>,
    /// Exact remote display ids the user disabled.
    #[serde(default)]
    disabled: Vec<String>,
}

/// Registry state under `<config>/plugins/`. The whole surface is
/// methods on this type: every mutation rewrites `remote.toml` through
/// one atomic writer, so a failed operation leaves the previous lock
/// intact.
pub struct RegistryStore {
    root: PathBuf,
    file: RemoteFile,
}

impl RegistryStore {
    pub fn load() -> Result<Self> {
        Self::load_in(crate::config_dir())
    }

    pub fn load_in(config_dir: impl AsRef<Path>) -> Result<Self> {
        let root = config_dir.as_ref().join("plugins");
        let path = root.join("remote.toml");
        let file = if path.exists() {
            let text = fs::read_to_string(&path)?;
            toml::from_str(&text).map_err(|e| Error::Registry(e.to_string()))?
        } else {
            RemoteFile::default()
        };
        Ok(Self { root, file })
    }

    /// Registries with their manifest rows, read from the local cache.
    /// No network: a registry whose cache cannot be read reports an
    /// error instead of an empty-but-plausible list.
    pub fn views(&self) -> Vec<RegistryView> {
        self.file
            .registries
            .iter()
            .map(|entry| match self.manifest(entry) {
                Ok(manifest) => RegistryView {
                    entry: entry.clone(),
                    plugins: manifest.plugins,
                    error: None,
                },
                Err(e) => RegistryView {
                    entry: entry.clone(),
                    plugins: Vec::new(),
                    error: Some(e.to_string()),
                },
            })
            .collect()
    }

    pub fn installed(&self) -> &[InstalledEntry] {
        &self.file.installed
    }

    pub fn installed_entry(&self, id: &str) -> Option<&InstalledEntry> {
        let key = PluginId::parse(id).ok()?.to_string();
        self.file.installed.iter().find(|i| i.id == key)
    }

    /// Remote enable state. Only an installed id can read as enabled, so
    /// an unknown or uninstalled id can never look enabled.
    pub fn remote_enabled(&self, id: &str) -> bool {
        let Ok(parsed) = PluginId::parse(id) else {
            return false;
        };
        let key = parsed.to_string();
        self.file.installed.iter().any(|i| i.id == key) && !self.file.disabled.contains(&key)
    }

    /// Add a registry: validate the URL, resolve the ref to a commit,
    /// clone into the cache, fetch that commit, and validate its
    /// manifest before recording anything.
    pub fn add(
        &mut self,
        url: &str,
        git_ref: &str,
        manifest_path: &str,
        local: bool,
    ) -> Result<RegistryEntry> {
        let url = url.trim();
        let git_ref = trim_or(git_ref, DEFAULT_REF);
        let manifest_path = trim_or(manifest_path, DEFAULT_MANIFEST);
        check_url(url, local)?;
        validate_rel_path(manifest_path)?;
        if self.file.registries.iter().any(|r| r.url == url) {
            return Err(Error::Registry(format!("already added: {url}")));
        }
        if self.file.registries.len() >= MAX_REGISTRIES {
            return Err(Error::Registry(format!(
                "too many registries (max {MAX_REGISTRIES})"
            )));
        }
        let pinned = ls_remote(url, git_ref)?;
        let bare = self.ensure_cache(url, manifest_path, local)?;
        fetch_pin(&bare, &pinned)?;
        // Validate before the lock changes: a registry that cannot be
        // read is not added.
        read_manifest_at(&bare, &pinned, manifest_path)?;
        let entry = RegistryEntry {
            url: url.into(),
            manifest_path: manifest_path.into(),
            discovery_ref: git_ref.into(),
            pinned,
            local,
            added_at: now_unix(),
        };
        self.file.registries.push(entry.clone());
        self.save()?;
        Ok(entry)
    }

    /// Re-resolve the discovery ref and re-pin. A new commit is
    /// validated before the pin moves, so a broken upstream leaves the
    /// old pin and the old manifest in place. Installed plugins keep
    /// their own pin until they are updated.
    pub fn update_registry(&mut self, url: &str) -> Result<RegistryEntry> {
        let index = self.registry_index(url)?;
        let entry = self.file.registries[index].clone();
        let pinned = ls_remote(&entry.url, &entry.discovery_ref)?;
        if pinned == entry.pinned {
            return Ok(entry);
        }
        let bare = self.ensure_cache(&entry.url, &entry.manifest_path, entry.local)?;
        fetch_pin(&bare, &pinned)?;
        read_manifest_at(&bare, &pinned, &entry.manifest_path)?;
        self.file.registries[index].pinned = pinned;
        self.save()?;
        Ok(self.file.registries[index].clone())
    }

    /// Remove a registry, every payload it installed, and its cache.
    /// First-party state is never touched.
    pub fn remove_registry(&mut self, url: &str) -> Result<()> {
        let index = self.registry_index(url)?;
        let entry = self.file.registries.remove(index);
        for installed in self
            .file
            .installed
            .iter()
            .filter(|i| i.registry == entry.url)
        {
            let _ = fs::remove_dir_all(self.root.join("installed").join(slug_for(&installed.id)));
        }
        self.file.installed.retain(|i| i.registry != entry.url);
        self.file.disabled.retain(|id| {
            self.file
                .installed
                .iter()
                .all(|i| !i.id.eq_ignore_ascii_case(id))
        });
        let _ = fs::remove_dir_all(git_cache(&self.root, &entry.url, &entry.manifest_path));
        self.save()
    }

    /// Install (or re-install) a plugin at its registry's current pin.
    /// Staging is verified, then atomically renamed into place; a
    /// failure removes only what this transaction created.
    pub fn install(&mut self, id: &str) -> Result<InstalledEntry> {
        let key = PluginId::parse(id)?.to_string();
        let mut unreadable: Vec<String> = Vec::new();
        for entry in self.file.registries.clone() {
            match self.manifest(&entry) {
                Ok(manifest) => {
                    if let Some(plugin) = manifest.plugins.iter().find(|p| p.id == key) {
                        let plugin = plugin.clone();
                        return self.install_from(&entry, &plugin);
                    }
                }
                Err(e) => unreadable.push(format!("{}: {e}", entry.url)),
            }
        }
        if !unreadable.is_empty() {
            return Err(Error::Registry(format!(
                "registry unreadable: {}",
                unreadable.join("; ")
            )));
        }
        Err(Error::UnknownPlugin(key))
    }

    /// Update an installed plugin to its registry's current pin. A pin
    /// that has not moved is a no-op; a failed update keeps the previous
    /// version selected and its lock entry unchanged.
    pub fn update_plugin(&mut self, id: &str) -> Result<InstalledEntry> {
        let key = PluginId::parse(id)?.to_string();
        let Some(current) = self.installed_entry(&key).cloned() else {
            return Err(Error::UnknownPlugin(key));
        };
        for entry in self.file.registries.clone() {
            if entry.url != current.registry {
                continue;
            }
            let manifest = self.manifest(&entry)?;
            let Some(plugin) = manifest.plugins.iter().find(|p| p.id == key) else {
                return Err(Error::UnknownPlugin(key));
            };
            if entry.pinned == current.pinned && plugin.sha256 == current.sha256 {
                return Ok(current);
            }
            let plugin = plugin.clone();
            return self.install_from(&entry, &plugin);
        }
        Err(Error::UnknownPlugin(key))
    }

    /// Remove an installed plugin and every staged version of it.
    pub fn remove_plugin(&mut self, id: &str) -> Result<()> {
        let key = PluginId::parse(id)?.to_string();
        let Some(index) = self.file.installed.iter().position(|i| i.id == key) else {
            return Err(Error::UnknownPlugin(key));
        };
        let entry = self.file.installed.remove(index);
        self.file.disabled.retain(|d| *d != entry.id);
        let _ = fs::remove_dir_all(self.root.join("installed").join(slug_for(&entry.id)));
        self.save()
    }

    /// Remote enable state, stored as exact display ids in
    /// `remote.toml`. Refused without a verified install so the toggle
    /// can never precede the bytes it names.
    pub fn set_remote_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        let key = PluginId::parse(id)?.to_string();
        if !self.file.installed.iter().any(|i| i.id == key) {
            return Err(Error::UnknownPlugin(key));
        }
        let mut next = self.file.disabled.clone();
        if enabled {
            next.retain(|d| *d != key);
        } else {
            if !next.contains(&key) {
                next.push(key);
            }
            next.sort();
        }
        if next == self.file.disabled {
            return Ok(());
        }
        self.file.disabled = next;
        self.save()
    }

    /// Resolve a registry's manifest at its stored pin. Cache only.
    pub fn manifest(&self, entry: &RegistryEntry) -> Result<RegistryManifest> {
        let bare = git_cache(&self.root, &entry.url, &entry.manifest_path);
        read_manifest_at(&bare, &entry.pinned, &entry.manifest_path)
    }

    fn registry_index(&self, url: &str) -> Result<usize> {
        self.file
            .registries
            .iter()
            .position(|r| r.url == url)
            .ok_or_else(|| Error::Registry(format!("unknown registry: {url}")))
    }

    fn ensure_cache(&self, url: &str, manifest_path: &str, local: bool) -> Result<PathBuf> {
        let bare = git_cache(&self.root, url, manifest_path);
        if bare.join("HEAD").exists() {
            return Ok(bare);
        }
        if let Some(parent) = bare.parent() {
            fs::create_dir_all(parent)?;
        }
        // Clone beside the destination, then rename: an interrupted or
        // failed clone never leaves a half-repo that reads as valid.
        let staging = bare.with_extension("cloning");
        let _ = fs::remove_dir_all(&staging);
        let mut cmd = Command::new("git");
        cmd.arg("clone")
            .arg("--bare")
            .arg("--quiet")
            .arg("--no-tags")
            .arg(url)
            .arg(&staging);
        if !local {
            cmd.arg("--single-branch");
        }
        let out = cmd
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::Registry(format!("git clone: {e}")))?;
        if !out.status.success() {
            let _ = fs::remove_dir_all(&staging);
            return Err(Error::Registry(format!(
                "git clone {}: {}",
                url,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        fs::rename(&staging, &bare)?;
        Ok(bare)
    }

    fn install_from(
        &mut self,
        entry: &RegistryEntry,
        plugin: &RegistryPlugin,
    ) -> Result<InstalledEntry> {
        let bare = git_cache(&self.root, &entry.url, &entry.manifest_path);
        if !bare.join("HEAD").exists() {
            return Err(Error::Registry(format!(
                "{}: cache missing, re-add the registry",
                entry.url
            )));
        }
        // The pin is the install identity: never fall back to a branch.
        fetch_pin(&bare, &entry.pinned)?;
        let slug = slug_for(&plugin.id);
        let stage = self.root.join("installed").join(&slug).join("staging");
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(&stage)?;
        let result = self.stage_payload(&bare, entry, plugin, &slug, &stage);
        let record = match result {
            Ok(record) => record,
            Err(e) => {
                // Only this transaction's staging is removed; an earlier
                // installed version is never touched on failure. The
                // per-plugin parent goes only when nothing else is in it.
                let _ = fs::remove_dir_all(&stage);
                let _ = fs::remove_dir(stage.parent().unwrap_or(&self.root));
                return Err(e);
            }
        };
        let _ = fs::remove_dir_all(&stage);
        let _ = fs::remove_dir(stage.parent().unwrap_or(&self.root));
        match self.file.installed.iter_mut().find(|i| i.id == record.id) {
            Some(existing) => *existing = record.clone(),
            None => self.file.installed.push(record.clone()),
        }
        self.file.installed.sort_by(|a, b| a.id.cmp(&b.id));
        self.save()?;
        Ok(record)
    }

    /// Archive the payload subtree, verify its tree digest, then land it
    /// atomically under its own short pin.
    fn stage_payload(
        &self,
        bare: &Path,
        entry: &RegistryEntry,
        plugin: &RegistryPlugin,
        slug: &str,
        stage: &Path,
    ) -> Result<InstalledEntry> {
        let tar = stage.join("payload.tar");
        archive(bare, &entry.pinned, &plugin.path, &tar)?;
        let dest = stage.join("payload");
        fs::create_dir_all(&dest)?;
        untar(&tar, &dest)?;
        let _ = fs::remove_file(&tar);
        let (digest, _) = tree_digest(&dest)?;
        if digest != plugin.sha256 {
            return Err(Error::HashMismatch {
                want: plugin.sha256.clone(),
                got: digest,
            });
        }
        let version = self
            .root
            .join("installed")
            .join(&slug)
            .join(short_pin(&entry.pinned));
        let _ = fs::remove_dir_all(&version);
        fs::rename(dest, &version)?;
        sync_dir(version.parent().unwrap_or(&self.root))?;
        Ok(InstalledEntry {
            id: plugin.id.clone(),
            registry: entry.url.clone(),
            pinned: entry.pinned.clone(),
            path: plugin.path.clone(),
            sha256: plugin.sha256.clone(),
            abi_major: plugin.abi_major,
            installed_at: now_unix(),
        })
    }

    fn save(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let text = toml::to_string(&self.file).map_err(|e| Error::Toml(e.to_string()))?;
        atomic_write(&self.root.join("remote.toml"), text.as_bytes())
    }
}

fn trim_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

/// HTTPS only unless the caller explicitly opted into local provenance
/// (CLI `--local`; the GUI never does). A typed URL is not trust.
fn check_url(url: &str, local: bool) -> Result<()> {
    if url.is_empty() {
        return Err(Error::Registry("empty registry url".into()));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(Error::Registry(format!(
            "registry url has whitespace: {url}"
        )));
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return if rest.is_empty() {
            Err(Error::Registry("registry url has no host".into()))
        } else {
            Ok(())
        };
    }
    if local {
        return Ok(());
    }
    Err(Error::Registry(format!(
        "registry url must be https:// (pass --local to opt into a local path): {url}"
    )))
}

/// Repo-relative POSIX path: no absolute root, no `..`, no empty
/// segment, and no character that git rev/path syntax would reinterpret.
fn validate_rel_path(path: &str) -> Result<()> {
    let bad = |why: &str| Error::Registry(format!("invalid path {path:?}: {why}"));
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(bad("not a repo-relative file or directory"));
    }
    if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return Err(bad("unsupported character"));
    }
    if path
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err(bad("empty or relative segment"));
    }
    Ok(())
}

fn read_manifest_at(bare: &Path, pin: &str, manifest_path: &str) -> Result<RegistryManifest> {
    let bytes = read_blob(bare, pin, manifest_path)?;
    let manifest: RegistryManifest =
        toml::from_str(std::str::from_utf8(&bytes).map_err(|e| Error::Registry(e.to_string()))?)
            .map_err(|e| Error::Registry(format!("{manifest_path}: {e}")))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &RegistryManifest) -> Result<()> {
    let bad = |why: String| Error::Registry(why);
    if manifest.version != REGISTRY_MANIFEST_VERSION {
        return Err(bad(format!(
            "manifest version {} (this build reads {REGISTRY_MANIFEST_VERSION})",
            manifest.version
        )));
    }
    if manifest.name.trim().is_empty() {
        return Err(bad("registry name is empty".into()));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for plugin in &manifest.plugins {
        let id = PluginId::parse(&plugin.id)
            .map_err(|e| Error::Registry(format!("{}: {e}", plugin.id)))?;
        if id.tag.is_some() {
            return Err(bad(format!(
                "{}: manifest ids carry no tag (the commit is the version)",
                plugin.id
            )));
        }
        if !seen.insert(plugin.id.as_str()) {
            return Err(bad(format!("{}: duplicate plugin id", plugin.id)));
        }
        if plugin.label.trim().is_empty() {
            return Err(bad(format!("{}: empty label", plugin.id)));
        }
        if plugin.label.chars().count() > MAX_LABEL_CHARS
            || plugin.description.chars().count() > MAX_DESCRIPTION_CHARS
            || plugin.author.chars().count() > MAX_AUTHOR_CHARS
        {
            return Err(bad(format!(
                "{}: label/description/author too long",
                plugin.id
            )));
        }
        validate_rel_path(&plugin.path)
            .map_err(|e| Error::Registry(format!("{}: {e}", plugin.id)))?;
        if plugin.sha256.len() != 64
            || !plugin
                .sha256
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(bad(format!(
                "{}: sha256 must be 64 lowercase hex chars",
                plugin.id
            )));
        }
        if plugin.abi_major != REGISTRY_ABI_MAJOR {
            return Err(bad(format!(
                "{}: abi_major {} (this build runs {REGISTRY_ABI_MAJOR})",
                plugin.id, plugin.abi_major
            )));
        }
    }
    Ok(())
}

fn git_cache(root: &Path, url: &str, manifest_path: &str) -> PathBuf {
    root.join("cache")
        .join(sha256_hex(format!("{url}\0{manifest_path}").as_bytes())[..16].to_string())
}

fn short_pin(pin: &str) -> &str {
    &pin[..pin.len().min(12)]
}

/// Filesystem-safe per-plugin directory: readable slug plus an id digest
/// so two ids can never share a directory.
fn slug_for(id: &str) -> String {
    let readable: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{readable}-{}", &sha256_hex(id.as_bytes())[..8])
}

fn ls_remote(url: &str, git_ref: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("ls-remote")
        .arg(url)
        .arg(git_ref)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::Registry(format!("git ls-remote: {e}")))?;
    if !out.status.success() {
        return Err(Error::Registry(format!(
            "git ls-remote {url} {git_ref}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let pin = first
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    if pin.len() != 40 || !pin.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Registry(format!(
            "git ls-remote {url} {git_ref}: no commit for this ref"
        )));
    }
    Ok(pin)
}

/// Make sure the bare cache holds exactly this commit.
fn fetch_pin(bare: &Path, pin: &str) -> Result<()> {
    if has_commit(bare, pin) {
        return Ok(());
    }
    git(
        bare,
        &["fetch", "--quiet", "--no-tags", "origin", pin],
        "fetch",
    )?;
    if !has_commit(bare, pin) {
        return Err(Error::Registry(format!(
            "commit {pin} is not fetchable from this remote"
        )));
    }
    Ok(())
}

fn has_commit(bare: &Path, pin: &str) -> bool {
    git(
        bare,
        &["cat-file", "-e", &format!("{pin}^{{commit}}")],
        "cat-file",
    )
    .is_ok()
}

fn read_blob(bare: &Path, pin: &str, path: &str) -> Result<Vec<u8>> {
    let spec = format!("{pin}:{path}");
    let size = git(bare, &["cat-file", "-s", &spec], "cat-file")?;
    let size: u64 = size
        .trim()
        .parse()
        .map_err(|_| Error::Registry(format!("{path}: unreadable size")))?;
    if size > MAX_MANIFEST_BYTES {
        return Err(Error::Registry(format!(
            "{path}: manifest is {size} bytes (max {MAX_MANIFEST_BYTES})"
        )));
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(["cat-file", "blob", &spec])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::Registry(format!("git cat-file: {e}")))?;
    if !out.status.success() {
        return Err(Error::Registry(format!(
            "{path}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(out.stdout)
}

fn archive(bare: &Path, pin: &str, path: &str, out: &Path) -> Result<()> {
    let spec = format!("{pin}:{path}");
    git(
        bare,
        &[
            "archive",
            "--format=tar",
            "--output",
            &out.to_string_lossy(),
            &spec,
        ],
        "archive",
    )?;
    Ok(())
}

fn git(bare: &Path, args: &[&str], what: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::Registry(format!("git {what}: {e}")))?;
    if !out.status.success() {
        return Err(Error::Registry(format!(
            "git {what}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)?.sync_all()?;
    Ok(())
}

/// Minimal ustar reader for the uncompressed `git archive` stream.
/// Regular files and directories only: a symlink, hardlink, device, or
/// pax extension header is an error, not something to skip past, and
/// every extracted path must stay under `dest`.
fn untar(tar: &Path, dest: &Path) -> Result<()> {
    let mut reader = io::BufReader::new(File::open(tar)?);
    let mut header = [0u8; 512];
    let mut files = 0usize;
    let mut bytes = 0u64;
    loop {
        match reader.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(Error::Registry("payload archive is truncated".into()))
            }
            Err(e) => return Err(e.into()),
        }
        if header == [0u8; 512] {
            return Ok(());
        }
        let name = tar_str(&header[0..100])?;
        let prefix = tar_str(&header[345..500])?;
        let size = tar_octal(&header[124..136])?;
        let kind = header[156];
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        match kind {
            b'0' | 0 => {}
            b'5' => {
                let out = safe_join(dest, &relative)?;
                fs::create_dir_all(&out)?;
                skip(&mut reader, size)?;
                continue;
            }
            other => {
                return Err(Error::Registry(format!(
                    "payload archive has an unsupported entry ({:?} {relative})",
                    other as char
                )))
            }
        }
        files += 1;
        bytes += size;
        if files > MAX_PAYLOAD_FILES {
            return Err(Error::Registry(format!(
                "payload has more than {MAX_PAYLOAD_FILES} files"
            )));
        }
        if bytes > MAX_PAYLOAD_BYTES {
            return Err(Error::Registry(format!(
                "payload is larger than {MAX_PAYLOAD_BYTES} bytes"
            )));
        }
        let out = safe_join(dest, &relative)?;
        if out.file_name().is_some_and(|n| n == ".provenance.toml") {
            // The digest skips this name, so a payload that carries one
            // could smuggle unverified bytes past the check.
            return Err(Error::Registry(format!(
                "payload may not contain {relative}"
            )));
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&out)?;
        let written = io::copy(&mut reader.by_ref().take(size), &mut file)?;
        if written != size {
            return Err(Error::Registry(format!(
                "payload archive is truncated at {relative}"
            )));
        }
        skip(&mut reader, (512 - size % 512) % 512)?;
    }
}

fn skip(reader: &mut impl Read, mut n: u64) -> Result<()> {
    let mut buf = [0u8; 4096];
    while n > 0 {
        let want = n.min(buf.len() as u64) as usize;
        let read = reader.read(&mut buf[..want])?;
        if read == 0 {
            return Err(Error::Registry("payload archive is truncated".into()));
        }
        n -= read as u64;
    }
    Ok(())
}

/// Join a tar member name under `dest`, refusing anything that escapes
/// or lands outside as a device/symlink would.
fn safe_join(dest: &Path, relative: &str) -> Result<PathBuf> {
    // `git archive` emits directory members with a trailing slash
    // (`sub/`). Accept that single marker; every other empty segment
    // (`a//b`, `/`, `..`) still fails below.
    let relative = relative.strip_suffix('/').unwrap_or(relative);
    if relative.is_empty() || relative.starts_with('/') || relative.contains('\\') {
        return Err(Error::Registry(format!("unsafe archive path: {relative}")));
    }
    let mut out = dest.to_path_buf();
    for segment in relative.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(Error::Registry(format!("unsafe archive path: {relative}")));
        }
        out.push(segment);
    }
    Ok(out)
}

fn tar_str(field: &[u8]) -> Result<String> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map(str::to_string)
        .map_err(|e| Error::Registry(format!("archive name is not utf-8: {e}")))
}

fn tar_octal(field: &[u8]) -> Result<u64> {
    let text = tar_str(field)?;
    let trimmed = text.trim_matches([' ', '\0', '\n']);
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8)
        .map_err(|e| Error::Registry(format!("archive size is not octal: {e}")))
}

#[cfg(test)]
mod registry_tests;
