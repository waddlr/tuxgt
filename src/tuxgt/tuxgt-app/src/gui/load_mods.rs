use tuxgt_core::{
    config_dir, data_dir, diagnose, enabled_knobs, game_manifests, list_mods, mods_for_game,
    parse_mod_type, parse_slot, EnvKnob, GameRow, InstalledEntry, ModPackage, PluginHost,
    RegistryStore, RegistryView, Strings,
};

use super::*;

/// Mods tab rows for one game: recipes applicable to that game plus every
/// manifest already installed for it (an installed mod never disappears).
pub(crate) fn load_mods_for(
    game_id: &str,
    game: Option<&GameRow>,
    strings: &Strings,
) -> Vec<ModRow> {
    let data = data_dir();
    let name = game.and_then(|g| g.name.as_deref()).unwrap_or("");
    let appid = game
        .and_then(|g| g.resolved_appid())
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|a| *a != 0);
    let inst = mods_for_game(&config_dir(), name, appid, &data).ok();
    // Full catalog, including disabled and not-ready provided recipes.
    // `mods_for_game` omits those so they are not offered for install.
    let catalog = list_mods(&config_dir(), &data).ok();
    let manifests = game_manifests(&data, game_id).unwrap_or_default();
    let req_owned: Vec<Vec<&str>> = manifests
        .iter()
        .map(|m| {
            catalog
                .as_ref()
                .and_then(|l| l.mods.iter().find(|i| i.id == m.instance))
                .map(|i| i.requires.iter().map(String::as_str).collect())
                .unwrap_or_default()
        })
        .collect();
    let pkgs: Vec<ModPackage<'_>> = manifests
        .iter()
        .zip(req_owned.iter())
        .map(|(m, req)| ModPackage {
            name: &m.instance,
            type_: &m.mod_type,
            slot: proxy_slot(&m.files).and_then(|s| parse_slot(s).ok()),
            requires: &[],
            requires_mods: req,
        })
        .collect();
    let diag = diagnose(&pkgs).ok();
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(list) = inst.as_ref() {
        for i in &list.mods {
            seen.insert(i.id.clone());
            let effect_files = tuxgt_core::effect_names_for(i).into_boxed_slice();
            let asset = source_asset(&i.source);
            let payload_present = tuxgt_core::payload_has_files(&data, i);
            if let Some(m) = manifests.iter().find(|m| m.instance == i.id) {
                rows.push(mod_row_from_manifest(
                    m,
                    &i.label,
                    i.official,
                    tuxgt_core::recipe_slot_configurable(&i.mod_type, i.slot.as_deref()),
                    effect_files,
                    asset,
                    payload_present,
                    diag.as_ref(),
                    strings,
                    game_id,
                ));
            } else {
                let ids = ModIds::for_instance(&i.id, game_id);
                rows.push(ModRow {
                    instance: i.id.clone(),
                    label: i.label.clone(),
                    mod_type: i.mod_type.clone(),
                    official: i.official,
                    adapter: "preload".into(),
                    enabled: false,
                    files: 0,
                    load_order: 0,
                    installed: false,
                    slot: strings.get("gui-mod-slot-none"),
                    slot_capable: tuxgt_core::recipe_slot_configurable(
                        &i.mod_type,
                        i.slot.as_deref(),
                    ),
                    graph: strings.get("gui-state-not-installed"),
                    file_entries: Box::default(),
                    env_entries: Box::default(),
                    effect_files,
                    asset,
                    payload_present,
                    ids,
                });
            }
        }
    }
    for m in &manifests {
        if seen.contains(&m.instance) {
            continue;
        }
        // Still in the catalog, just not offered (disabled, or provided
        // files cleared). Keep its label, official flag, and metadata.
        if let Some(i) = catalog
            .as_ref()
            .and_then(|l| l.mods.iter().find(|i| i.id == m.instance))
        {
            rows.push(mod_row_from_manifest(
                m,
                &i.label,
                i.official,
                tuxgt_core::recipe_slot_configurable(&i.mod_type, i.slot.as_deref()),
                tuxgt_core::effect_names_for(i).into_boxed_slice(),
                source_asset(&i.source),
                tuxgt_core::payload_has_files(&data, i),
                diag.as_ref(),
                strings,
                game_id,
            ));
            continue;
        }
        // Recipe gone from the catalog: officialness unknown and no
        // effect/asset metadata to preview, treat as user.
        rows.push(mod_row_from_manifest(
            m,
            &m.instance,
            false,
            false,
            Box::default(),
            None,
            false,
            diag.as_ref(),
            strings,
            game_id,
        ));
    }
    rows
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mod_row_from_manifest(
    m: &tuxgt_core::FileManifest,
    label: &str,
    official: bool,
    slot_capable: bool,
    effect_files: Box<[String]>,
    asset: Option<String>,
    payload_present: bool,
    diag: Option<&tuxgt_core::Diagnosis>,
    strings: &Strings,
    game_id: &str,
) -> ModRow {
    let ids = ModIds::for_instance(&m.instance, game_id);
    ModRow {
        instance: m.instance.clone(),
        label: label.to_string(),
        mod_type: m.mod_type.clone(),
        official,
        adapter: m.adapter.clone(),
        enabled: m.enabled,
        files: m.files.len(),
        load_order: m.load_order,
        installed: true,
        slot: super::slot_show::closed_slot_label(&m.files, &m.include)
            .unwrap_or_else(|| strings.get("gui-mod-slot-none")),
        slot_capable,
        graph: graph_note(&m.instance, diag, strings),
        file_entries: m
            .files
            .iter()
            .map(|f| ModFileRow {
                dest: f.dest.clone(),
                source: f.source.clone(),
                enabled: f.enabled,
                required: tuxgt_core::is_required_dest(
                    &m.mod_type,
                    &f.dest,
                    &m.include,
                    m.files.len(),
                ),
                loaddll: tuxgt_core::file_is_loaddll(&f.dest, &m.include, f.load),
                recipe_load: tuxgt_core::is_dll(&f.dest)
                    && !tuxgt_core::include_covers(&m.include, &f.dest),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        env_entries: m
            .env
            .iter()
            .map(|e| ModEnvRow {
                key: e.key.clone(),
                value: e.value.clone(),
                enabled: e.enabled,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        effect_files,
        asset,
        payload_present,
        ids,
    }
}

/// Sibling `.dll` dest's file name: the preload/install proxy slot.
/// Only top-level dests (no `/` or `\`, no `pfx:` prefix) sit beside the
/// game exe and can be the proxy; subdir companions never claim it.
/// `None` when no sibling dll dest exists (stock-named ReShade dests pass through as-is).
pub(crate) fn proxy_slot(files: &[tuxgt_core::PlannedFile]) -> Option<&str> {
    let mut top = files.iter().map(|f| f.dest.as_str()).filter(|d| {
        d.to_ascii_lowercase().ends_with(".dll")
            && !d.contains('/')
            && !d.contains('\\')
            && !tuxgt_core::is_prefix_dest(d)
    });
    let first = top.next()?;
    Some(
        std::iter::once(first)
            .chain(top)
            .find(|d| tuxgt_core::parse_slot(d).is_ok())
            .unwrap_or(first),
    )
}
/// E91: Add-form Requires gate. True when the type carries kind-level
/// Requires (`reshade_addon`/`effect`/`texture` need a ReShade Mod): Save
/// stays blocked until one is picked. Reuses core `ModType::requires`.
pub(crate) fn add_requires_gate(mod_type: &str) -> bool {
    parse_mod_type(mod_type).is_ok_and(|t| t.requires().is_some())
}
/// E78 merged file row: basename of a depot path (`/`, `\`, or the cache
/// `instance#` separator).
pub(crate) fn file_basename(path: &str) -> &str {
    path.rsplit(['/', '\\', '#']).next().unwrap_or(path)
}

/// E78 merged file row label: `basename(source) → dest`, or `dest` alone
/// when both basenames match (case-insensitive: `FOO.dll` is `foo.dll`).
pub(crate) fn file_mapping_label(source: &str, dest: &str) -> String {
    let base = file_basename(source);
    if base.eq_ignore_ascii_case(file_basename(dest)) {
        dest.to_string()
    } else {
        format!("{base} → {dest}")
    }
}

/// Whole-token match so `optiscaler` never matches `optiscaler-xyz`.
pub(crate) fn name_mentioned(line: &str, instance: &str) -> bool {
    line.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .any(|tok| tok == instance)
}
/// Per-row deps/conflicts text. R50: empty when the diagnosis mentions this
/// instance nowhere — the card renders the graph line only for missing
/// requires or slot conflicts. Unavailable diagnosis still says so.
pub(crate) fn graph_note(
    instance: &str,
    diag: Option<&tuxgt_core::Diagnosis>,
    strings: &Strings,
) -> String {
    let Some(d) = diag else {
        return strings.get("gui-mod-graph-unavailable");
    };
    let mut hits: Vec<&str> = Vec::new();
    for line in d.missing_requires.iter().chain(d.slot_conflicts.iter()) {
        if name_mentioned(line, instance) {
            hits.push(line.as_str());
        }
    }
    hits.join(" · ")
}

pub(crate) fn load_plugins(strings: &Strings) -> Vec<PluginRow> {
    PluginHost::load()
        .map(|host| {
            host.list()
                .into_iter()
                .map(|e| PluginRow {
                    id: e.desc.id().to_string(),
                    label: strings.get(e.desc.label_id),
                    enabled: e.enabled,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn load_instances() -> Vec<InstanceRow> {
    let data = data_dir();
    let cfg = config_dir();
    let Ok(listed) = list_mods(&cfg, &data) else {
        return Vec::new();
    };
    let labels: std::collections::HashMap<String, String> = listed
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.label.clone()))
        .collect();
    listed
        .mods
        .into_iter()
        .map(|i| {
            let effect_files = tuxgt_core::effect_names_for(&i).into_boxed_slice();
            let asset = source_asset(&i.source);
            let payload_present = tuxgt_core::payload_has_files(&data, &i);
            let ids = InstanceIds::for_id(&i.id);
            let (note, provided) = provided_status(&data, &i);
            // A not-ready provided payload is not a version, even if a
            // previous `.provenance.toml` was left behind.
            let ready = provided.as_ref().is_none_or(|p| p.ready);
            let prov = match &i.source {
                tuxgt_core::SourceRef::Provided { .. } if !ready => None,
                _ => tuxgt_core::payload_provenance(&data, &i),
            };
            let facts = super::instance_facts::instance_facts(&i, &labels, prov.as_ref(), ready);
            InstanceRow {
                id: i.id,
                label: i.label,
                description: i.description,
                mod_type: i.mod_type,
                source: i.source.type_str().into(),
                official: i.official,
                enabled: i.enabled,
                effect_files,
                asset,
                payload_present,
                note,
                provided,
                ids,
                facts,
            }
        })
        .collect()
}

/// Note plus file status for a `provided` source. Other sources stay empty.
/// A read error still paints: `ready` is false and the error is the missing line.
fn provided_status(
    data: &std::path::Path,
    inst: &tuxgt_core::Mod,
) -> (String, Option<ProvidedRow>) {
    let tuxgt_core::SourceRef::Provided { note, .. } = &inst.source else {
        return (String::new(), None);
    };
    let note = note.clone();
    let provided = match tuxgt_core::provided_files(data, inst) {
        Ok(Some(files)) => ProvidedRow {
            present: files.present,
            missing: files.missing,
            ambiguous: files.ambiguous,
            ready: files.ready,
        },
        Ok(None) => ProvidedRow {
            present: Vec::new(),
            missing: Vec::new(),
            ambiguous: Vec::new(),
            ready: false,
        },
        Err(e) => ProvidedRow {
            present: Vec::new(),
            missing: vec![e.to_string()],
            ambiguous: Vec::new(),
            ready: false,
        },
    };
    (note, Some(provided))
}

pub(crate) fn load_knobs() -> Vec<&'static EnvKnob> {
    PluginHost::load()
        .map(|h| enabled_knobs(&h))
        .unwrap_or_default()
}

/// R14: the pinned commit a registry or install resolved to, short enough
/// for a row. The full sha stays in `remote.toml`; this is display only.
pub(crate) fn short_pin(pin: &str) -> String {
    pin.chars().take(12).collect()
}

/// R14 one added registry: its pin plus how many manifest rows it has.
/// A registry whose cache cannot be read keeps its pin and reports the
/// error — an unreadable registry is never an empty-but-plausible one.
pub(crate) fn registry_row(view: &RegistryView) -> RegistryRow {
    RegistryRow {
        url: view.entry.url.clone(),
        manifest_path: view.entry.manifest_path.clone(),
        pinned_short: short_pin(&view.entry.pinned),
        plugin_count: view.plugins.len(),
        error: view.error.clone(),
    }
}

/// R14 manifest rows joined with install state. `enabled_of` is
/// `RegistryStore::remote_enabled`, injected so this join stays a pure
/// function of the data and can be tested against a real store.
///
/// A pin that moved is an available update even when the payload digest
/// did not: the commit is the install identity. Only a manifest row's
/// registry can own its plugin, so a duplicate id in a second registry
/// cannot shadow the one that is installed.
///
/// `install_offered` is the half of that rule the row needs to be honest
/// about. Core's `install(&id)` walks `registries` in the same order this
/// function walks `views()` and takes the FIRST manifest holding the id,
/// so a later registry's duplicate would install the earlier registry's
/// bytes while the user clicked a row labelled with the later URL. Only
/// the first registry carrying an id offers the button.
pub(crate) fn remote_plugin_rows(
    views: &[RegistryView],
    installed: &[InstalledEntry],
    enabled_of: &dyn Fn(&str) -> bool,
) -> Vec<RemotePluginRow> {
    // Ownership resolves as the rows are built, in registry order: a plugin
    // id is offered by the first registry that publishes it, and every later
    // registry's duplicate is a shadowed row.
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for view in views.iter().filter(|view| view.error.is_none()) {
        for plugin in &view.plugins {
            let install_offered = seen.insert(plugin.id.as_str());
            let current = installed
                .iter()
                .find(|i| i.id == plugin.id && i.registry == view.entry.url);
            rows.push(RemotePluginRow {
                id: plugin.id.clone(),
                label: plugin.label.clone(),
                description: plugin.description.clone(),
                author: plugin.author.clone(),
                registry_url: view.entry.url.clone(),
                pinned_short: short_pin(&view.entry.pinned),
                installed: current.is_some(),
                installed_pin_short: current.map(|i| short_pin(&i.pinned)),
                update_available: current
                    .is_some_and(|i| i.pinned != view.entry.pinned || i.sha256 != plugin.sha256),
                enabled: enabled_of(&plugin.id),
                install_offered,
            });
        }
    }
    rows
}

/// R14 read the whole registry surface for the Core Plugins page: the
/// added registries and their plugin rows. One store read, so the rows a
/// registry shows and the install state under them come from the same
/// `remote.toml`. A failed read paints no rows and reports the error.
pub(crate) fn load_registry() -> tuxgt_core::Result<(Vec<RegistryRow>, Vec<RemotePluginRow>)> {
    let store = RegistryStore::load()?;
    let views = store.views();
    let rows = views.iter().map(registry_row).collect();
    let plugins = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
    Ok((rows, plugins))
}

/// R14 fixture tests. A real local git repo stands in for a registry, and
/// a real `RegistryStore` reads it, so these exercise the same code the
/// page renders from — not a stand-in for it. `local = true` is how the
/// store is pointed at a path; the GUI itself never passes it.
#[cfg(test)]
mod registry_tests {
    use super::{registry_row, remote_plugin_rows};
    use sha2::{Digest as _, Sha256};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tuxgt_core::RegistryStore;

    static N: AtomicU32 = AtomicU32::new(0);

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tuxgt-r14-gui-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            tag
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A registry repo with one committed payload and a manifest whose
    /// digest is computed the way `install` verifies it, so an install in
    /// these tests can only fail from tampering, never from drift.
    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
        config: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let root = scratch(tag);
            let repo = root.join("repo");
            let config = root.join("config");
            fs::create_dir_all(&repo).unwrap();
            fs::create_dir_all(&config).unwrap();
            git(&repo, &["init", "--quiet", "--initial-branch=main"]);
            git(&repo, &["config", "user.email", "registry@example.invalid"]);
            git(&repo, &["config", "user.name", "registry test"]);
            git(&repo, &["config", "commit.gpgsign", "false"]);
            Fixture { root, repo, config }
        }

        /// The digest `install` verifies: core hashes the unpacked payload
        /// tree as `sha256(rel \0 file_sha256_hex \0)` per file, in sorted
        /// path order. Mirrored here so a valid manifest can be written
        /// without reaching into core's private unpack helpers.
        fn digest(&self, rel: &str) -> String {
            let base = self.repo.join(rel);
            let mut files: Vec<PathBuf> = Vec::new();
            collect(&base, &mut files);
            files.sort();
            let mut hasher = Sha256::new();
            for file in &files {
                let rel_path = file
                    .strip_prefix(&base)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .into_owned();
                hasher.update(rel_path.as_bytes());
                hasher.update([0]);
                hasher.update(hex(&Sha256::digest(fs::read(file).unwrap())).as_bytes());
                hasher.update([0]);
            }
            hex(&hasher.finalize())
        }

        /// Write a payload file, commit it, and point the manifest at the
        /// subtree's new digest. `rel` is a directory, because that is
        /// what the registry archives. Returns the new commit.
        fn commit(&self, rel: &str, body: &str) -> String {
            let dir = self.repo.join(rel);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("payload.txt"), body).unwrap();
            self.write_manifest(&format!(
                "version = 1\nname = \"test\"\n\n[[plugins]]\nid = \"git.example/team/hello\"\nlabel = \"Hello\"\ndescription = \"Greeting plugin\"\nauthor = \"Example\"\npath = \"{rel}\"\nsha256 = \"{digest}\"\nabi_major = 1\n",
                digest = self.digest(rel),
            ))
        }

        fn write_manifest(&self, body: &str) -> String {
            fs::write(self.repo.join("registry.toml"), body).unwrap();
            git(&self.repo, &["add", "-A"]);
            git(
                &self.repo,
                &["commit", "--quiet", "-m", "manifest", "--no-gpg-sign"],
            );
            git(&self.repo, &["rev-parse", "HEAD"])
        }

        fn url(&self) -> String {
            self.repo.to_string_lossy().into_owned()
        }

        fn store(&self) -> RegistryStore {
            RegistryStore::load_in(&self.config).expect("store")
        }
    }

    /// A second registry repo in the same fixture, sharing one config dir
    /// so both can be added to the same store. Used to prove that a
    /// duplicate plugin id in a later registry does not offer Install.
    fn second_repo(tag: &str) -> PathBuf {
        let root = scratch(tag);
        let repo = root.join("repo2");
        fs::create_dir_all(repo.join("payload/hello")).unwrap();
        git(&repo, &["init", "--quiet", "--initial-branch=main"]);
        git(&repo, &["config", "user.email", "registry@example.invalid"]);
        git(&repo, &["config", "user.name", "registry test"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("payload/hello/payload.txt"), "second registry\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &["commit", "--quiet", "-m", "payload", "--no-gpg-sign"],
        );
        // Same manifest shape, and the digest is computed the same way so
        // the second registry is genuinely installable — the test is about
        // which one the GUI offers, not about the second one being broken.
        let stage = repo.clone();
        let digest = {
            let base = stage.join("payload/hello");
            let mut hasher = Sha256::new();
            hasher.update("payload.txt".as_bytes());
            hasher.update([0]);
            hasher.update(
                hex(&Sha256::digest(fs::read(base.join("payload.txt")).unwrap())).as_bytes(),
            );
            hasher.update([0]);
            hex(&hasher.finalize())
        };
        fs::write(
            repo.join("registry.toml"),
            format!(
                "version = 1\nname = \"second\"\n\n[[plugins]]\nid = \"git.example/team/hello\"\nlabel = \"Hello (second)\"\npath = \"payload/hello\"\nsha256 = \"{digest}\"\nabi_major = 1\n"
            ),
        )
        .unwrap();
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &["commit", "--quiet", "-m", "manifest", "--no-gpg-sign"],
        );
        repo
    }

    /// Sorted recursive file walk, matching core's `collect_files`.
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                collect(&entry, out);
            } else {
                out.push(entry);
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// The end-to-end path the page drives: add, list, install, and the
    /// rows the section paints at each step.
    #[test]
    fn add_list_install_joins_real_install_state() {
        let fx = Fixture::new("install");
        fx.commit("payload/hello", "hello v1\n");

        let mut store = fx.store();
        let entry = store
            .add(&fx.url(), "HEAD", "registry.toml", true)
            .expect("add");
        assert_eq!(entry.pinned.len(), 40, "pin is a full commit sha");

        let views = store.views();
        assert_eq!(views.len(), 1);
        assert!(views[0].error.is_none(), "readable: {:?}", views[0].error);
        let row = registry_row(&views[0]);
        assert_eq!(row.plugin_count, 1);
        assert_eq!(row.pinned_short.len(), 12);

        // Before install: one row, not installed, no update.
        let before = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].id, "git.example/team/hello");
        assert_eq!(before[0].label, "Hello");
        assert!(!before[0].installed);
        assert!(!before[0].update_available);
        assert!(before[0].installed_pin_short.is_none());

        // Install through the store the page calls.
        store.install("git.example/team/hello").expect("install");
        let views = store.views();
        let after = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert!(after[0].installed);
        assert_eq!(
            after[0].installed_pin_short.as_deref(),
            Some(&entry.pinned[..12])
        );
        // A fresh install is not an update of itself.
        assert!(!after[0].update_available);
        // Core's `remote_enabled` is "installed and not in `disabled`", so
        // a fresh install reads as enabled. The row reports that state
        // rather than inventing an "installed but off" third state.
        assert!(after[0].enabled, "installed and not disabled reads enabled");
    }

    /// A moved pin is an available update; the row shows both versions,
    /// and a tampered update leaves the installed one selected.
    #[test]
    fn update_appears_then_tamper_keeps_old_version() {
        let fx = Fixture::new("update");
        let v1 = fx.commit("payload/hello", "hello v1\n");
        let mut store = fx.store();
        store
            .add(&fx.url(), "HEAD", "registry.toml", true)
            .expect("add");
        store.install("git.example/team/hello").expect("install v1");
        assert_eq!(store.installed()[0].pinned, v1);

        // Same bytes, new commit: the commit is the install identity, so
        // this is an update even though the digest did not move.
        fx.write_manifest(&format!(
            "version = 1\nname = \"test\"\n\n[[plugins]]\nid = \"git.example/team/hello\"\nlabel = \"Hello\"\npath = \"payload/hello\"\nsha256 = \"{}\"\nabi_major = 1\n",
            store.installed()[0].sha256
        ));
        store.update_registry(&fx.url()).expect("repin");
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert!(rows[0].update_available, "moved pin offers an update");
        assert_eq!(rows[0].installed_pin_short.as_deref(), Some(&v1[..12]));

        // Tamper: a manifest claiming v2 whose digest does not match the
        // committed bytes. The update must fail and keep v1 selected.
        fx.write_manifest(&format!(
            "version = 1\nname = \"test\"\n\n[[plugins]]\nid = \"git.example/team/hello\"\nlabel = \"Hello\"\npath = \"payload/hello\"\nsha256 = \"{}\"\nabi_major = 1\n",
            "0".repeat(64)
        ));
        store.update_registry(&fx.url()).expect("repin tampered");
        let err = store
            .update_plugin("git.example/team/hello")
            .expect_err("tampered payload is refused");
        assert!(
            err.to_string().contains("sha256"),
            "digest mismatch is the error: {err}"
        );
        // The prior version is still installed, still selected, and the
        // row still paints it.
        let after = store.installed()[0].pinned.clone();
        assert_eq!(after, v1, "prior pin untouched after a failed update");
        // The page still paints the old version, not the tampered one.
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert_eq!(rows[0].installed_pin_short.as_deref(), Some(&v1[..12]));
    }

    /// Remove takes the plugin out of `remote.toml` and off the page;
    /// first-party plugin state is never touched by any of it.
    #[test]
    fn remove_drops_the_plugin_not_the_host() {
        let fx = Fixture::new("remove");
        fx.commit("payload/hello", "hello v1\n");
        // First-party state to protect: a known disabled id in this
        // fixture's own config, so the assertion below has something real
        // to lose. Registry state lives in plugins/remote.toml beside it.
        let first_party = fx.config.join("plugins.toml");
        let seed = "disabled = [\"heroic\"]\n";
        fs::write(&first_party, seed).expect("seed plugins.toml");
        let mut store = fx.store();
        store
            .add(&fx.url(), "HEAD", "registry.toml", true)
            .expect("add");
        store.install("git.example/team/hello").expect("install");
        store
            .set_remote_enabled("git.example/team/hello", true)
            .expect("enable");
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert!(rows[0].enabled);

        // Disable flips only remote state; the install stays.
        store
            .set_remote_enabled("git.example/team/hello", false)
            .expect("disable");
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert!(rows[0].installed && !rows[0].enabled);

        store
            .remove_plugin("git.example/team/hello")
            .expect("remove");
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        // Still offered by the registry, no longer installed, no update.
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].installed);
        assert!(!rows[0].update_available);
        assert!(store.installed().is_empty());

        // Review P2: first-party state must be untouched, asserted on THIS
        // fixture's own plugins.toml rather than the process config dir.
        // The old version called PluginHost::load(), which reads the global
        // config and only checks ids that FIRST_PARTY always lists — it
        // could not fail even if these ops rewrote plugins.toml. Seed a
        // known disabled list up front, then demand the file be byte-identical
        // after the whole registry lifecycle.
        assert_eq!(
            fs::read_to_string(&first_party).unwrap(),
            seed,
            "registry ops rewrote first-party plugins.toml"
        );
        // And the host, pointed at this config, still reads that same list.
        let host = tuxgt_core::PluginHost::load_with(tuxgt_core::plugin::FIRST_PARTY, &fx.config)
            .expect("host");
        assert!(
            !host.is_enabled("heroic"),
            "the seeded first-party disable survived"
        );
    }

    /// An unreadable registry keeps its row and reports the error instead
    /// of reading as a registry with no plugins.
    #[test]
    fn unreadable_registry_reports_instead_of_looking_empty() {
        let fx = Fixture::new("unreadable");
        fx.commit("payload/hello", "hello v1\n");
        let mut store = fx.store();
        store
            .add(&fx.url(), "HEAD", "registry.toml", true)
            .expect("add");

        // Drop the cached bare repo: the entry's pin can no longer be read,
        // which is exactly the state the row has to report honestly.
        let cache_dir = fx.config.join("plugins").join("cache");
        for entry in fs::read_dir(&cache_dir).expect("cache dir") {
            let path = entry.expect("cache entry").path();
            if path.join("HEAD").exists() {
                fs::remove_dir_all(&path).expect("drop bare cache");
            }
        }
        let views = store.views();
        let row = registry_row(&views[0]);
        assert!(row.error.is_some(), "read error is reported: {row:?}");
        // A registry that cannot be read contributes no plugin rows.
        let rows = remote_plugin_rows(&views, store.installed(), &|id| store.remote_enabled(id));
        assert!(rows.is_empty());
    }

    /// Review P2: two registries publish the SAME plugin id. Core's
    /// `install(&id)` walks `registries` in the order they were added and
    /// takes the first match, so only the first registry's row may offer
    /// Install — otherwise clicking the second row's button would fetch the
    /// first registry's payload under the second registry's URL.
    #[test]
    fn duplicate_id_offers_install_only_on_the_owning_registry() {
        let fx = Fixture::new("dup");
        fx.commit("payload/hello", "hello v1\n");
        let second = second_repo("dup2");
        let second_url = second.to_string_lossy().into_owned();

        let mut store = fx.store();
        // Order matters: the first `add` is the one core installs from.
        store
            .add(&fx.url(), "HEAD", "registry.toml", true)
            .expect("add first");
        store
            .add(&second_url, "HEAD", "registry.toml", true)
            .expect("add second");

        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id: &str| {
            store.remote_enabled(id)
        });
        assert_eq!(rows.len(), 2, "both registries list the id");
        assert_eq!(rows[0].registry_url, fx.url(), "first registry row");
        assert_eq!(rows[1].registry_url, second_url, "second registry row");
        assert!(
            rows[0].install_offered,
            "the registry core installs from offers Install"
        );
        assert!(
            !rows[1].install_offered,
            "a later duplicate must not offer Install"
        );
        // Both rows are honest about not being installed.
        assert!(!rows[0].installed && !rows[1].installed);

        // Installing by id resolves to the FIRST registry, so after the op
        // the owner row is installed and the duplicate still is not.
        store.install("git.example/team/hello").expect("install");
        let views = store.views();
        let rows = remote_plugin_rows(&views, store.installed(), &|id: &str| {
            store.remote_enabled(id)
        });
        assert_eq!(
            store.installed()[0].registry,
            fx.url(),
            "core installed from the first registry"
        );
        assert!(rows[0].installed && !rows[0].update_available);
        assert!(
            !rows[1].installed,
            "the duplicate row is not installed by the other's install"
        );
        assert!(!rows[1].install_offered, "still no offer after an install");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        file_basename, file_mapping_label, mod_row_from_manifest, proxy_slot, ProtonSummary,
    };
    use tuxgt_core::PlannedFile;

    fn planned(dest: &str) -> PlannedFile {
        PlannedFile {
            source: dest.into(),
            dest: dest.into(),
            sha256: String::new(),
            enabled: true,
            load: None,
        }
    }

    #[test]
    fn proxy_slot_ignores_subdir_companions() {
        let files = vec![planned("bin/D3D12Core.dll"), planned("dxgi.dll")];
        assert_eq!(proxy_slot(&files), Some("dxgi.dll"));
        let files = vec![planned("bin/dxgi.dll")];
        assert_eq!(proxy_slot(&files), None);
        let files = vec![planned("dxgi.dll")];
        assert_eq!(proxy_slot(&files), Some("dxgi.dll"));
        let files = vec![planned("amd_fidelityfx_dx12.dll"), planned("dxgi.dll")];
        assert_eq!(proxy_slot(&files), Some("dxgi.dll"));
    }

    #[test]
    fn installed_row_carries_recipe_slot_capability() {
        let strings = tuxgt_core::Strings::en_us().unwrap();
        let manifest = tuxgt_core::FileManifest {
            game: "g".into(),
            instance: "m".into(),
            mod_type: "custom".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            files: Box::default(),
            env: Box::default(),
            backups: Default::default(),
            generated_globs: Box::default(),
            include: Box::default(),
            harvested: Default::default(),
            provenance: Default::default(),
        };
        let row = mod_row_from_manifest(
            &manifest,
            "m",
            false,
            true,
            Box::default(),
            None,
            false,
            None,
            &strings,
            "g",
        );
        assert!(row.installed && row.slot_capable);
        let row = mod_row_from_manifest(
            &manifest,
            "m",
            false,
            false,
            Box::default(),
            None,
            false,
            None,
            &strings,
            "g",
        );
        assert!(row.installed && !row.slot_capable);
    }

    #[test]
    fn mapping_shows_source_base_when_renamed() {
        assert_eq!(
            file_mapping_label("depot/OptiScaler.dll", "dxgi.dll"),
            "OptiScaler.dll → dxgi.dll"
        );
        assert_eq!(
            file_mapping_label("cache/e567eb2e47363f15/OS-v3#OptiScaler.dll", "dxgi.dll"),
            "OptiScaler.dll → dxgi.dll"
        );
    }

    #[test]
    fn mapping_shows_dest_only_when_basenames_match() {
        assert_eq!(file_mapping_label("depot/dxgi.dll", "dxgi.dll"), "dxgi.dll");
        assert_eq!(
            file_mapping_label("depot/ReShade64.dll", "win32\\ReShade64.dll"),
            "win32\\ReShade64.dll"
        );
    }

    #[test]
    fn basename_matches_case_insensitively() {
        assert_eq!(file_basename("win32\\FOO.DLL"), "FOO.DLL");
        assert_eq!(file_mapping_label("depot/FOO.dll", "foo.DLL"), "foo.DLL");
    }

    /// E84: the Info block parses the whitelisted E83 tier; `pending`
    /// falls back to the provisional tier and a missing cache stays an
    /// honest `none`.
    #[test]
    fn proton_summary_parses_cached_whitelist() {
        let full = serde_json::json!({"tier": "gold"});
        let s = ProtonSummary::parse(&full);
        assert_eq!(s.tier, "gold");

        let pending = serde_json::json!({"tier": "pending", "provisionalTier": "silver"});
        let s = ProtonSummary::parse(&pending);
        assert_eq!(s.tier, "silver");

        let s = ProtonSummary::default();
        assert_eq!(s.tier, "none");
    }
}
