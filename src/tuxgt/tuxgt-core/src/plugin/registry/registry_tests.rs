use super::super::testing::temp_config;
use super::*;
use crate::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A local git repo that stands in for a registry: `registry.toml` at the
/// root plus a payload subtree, committed so refs resolve like a remote.
struct Fixture {
    dir: PathBuf,
    head: String,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = temp_config().join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture dir");
        run(&dir, &["init", "--quiet", "--initial-branch=main"]);
        run(&dir, &["config", "user.email", "registry@example.invalid"]);
        run(&dir, &["config", "user.name", "registry test"]);
        run(&dir, &["config", "commit.gpgsign", "false"]);
        Fixture {
            dir,
            head: String::new(),
        }
    }

    /// Write a payload file, then commit and return the new commit.
    fn commit_payload(&mut self, rel: &str, body: &str) -> String {
        let path = self.dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("payload dir");
        fs::write(&path, body).expect("payload write");
        run(&self.dir, &["add", "-A"]);
        run(
            &self.dir,
            &["commit", "--quiet", "-m", "payload", "--no-gpg-sign"],
        );
        let head = run(&self.dir, &["rev-parse", "HEAD"]);
        self.head = head.clone();
        head
    }

    fn write_manifest(&mut self, body: &str) {
        fs::write(self.dir.join("registry.toml"), body).expect("manifest write");
        run(&self.dir, &["add", "-A"]);
        run(
            &self.dir,
            &["commit", "--quiet", "-m", "manifest", "--no-gpg-sign"],
        );
        self.head = run(&self.dir, &["rev-parse", "HEAD"]);
    }

    fn url(&self) -> String {
        self.dir.to_string_lossy().into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn store() -> (PathBuf, RegistryStore) {
    let config = temp_config().join("config");
    let _ = fs::remove_dir_all(&config);
    fs::create_dir_all(&config).expect("config dir");
    let store = RegistryStore::load_in(&config).expect("store");
    (config, store)
}

/// Manifest whose digest is computed from the committed payload, so a
/// digest mismatch test can only come from tampering, not drift.
fn manifest_for(sha: &str) -> String {
    format!(
        "version = 1\nname = \"test\"\n\n[[plugins]]\nid = \"git.example/team/hello\"\nlabel = \"Hello\"\npath = \"payload/hello\"\nsha256 = \"{sha}\"\nabi_major = 1\n"
    )
}

fn tree_digest_of(fixture: &Fixture, rel: &str) -> String {
    let (digest, _) = payload_digest_in_repo_at(&fixture.dir, &fixture.head, rel)
        .expect("digest of committed payload");
    digest
}

fn payload_digest_in_repo_at(repo: &Path, pin: &str, path: &str) -> Result<(String, u64)> {
    let stage = temp_config().join("digest");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)?;
    archive(repo, pin, path, &stage.join("p.tar"))?;
    let dest = stage.join("payload");
    fs::create_dir_all(&dest)?;
    untar(&stage.join("p.tar"), &dest)?;
    let out = tree_digest(&dest);
    let _ = fs::remove_dir_all(&stage);
    out
}

#[test]
fn manifest_rejects_unsupported_version_and_abi() {
    let mut m = RegistryManifest {
        version: 2,
        name: "test".into(),
        plugins: Vec::new(),
    };
    assert!(validate_manifest(&m).is_err());
    m.version = REGISTRY_MANIFEST_VERSION;
    validate_manifest(&m).expect("version 1");

    m.plugins.push(RegistryPlugin {
        id: "git.example/team/hello".into(),
        label: "Hello".into(),
        description: String::new(),
        author: String::new(),
        path: "payload/hello".into(),
        sha256: "a".repeat(64),
        abi_major: REGISTRY_ABI_MAJOR + 1,
    });
    let err = validate_manifest(&m).expect_err("abi mismatch").to_string();
    assert!(err.contains("abi_major"), "{err}");
}

#[test]
fn manifest_rejects_duplicate_ids_bad_paths_and_bad_digests() {
    let plugin = RegistryPlugin {
        id: "git.example/team/hello".into(),
        label: "Hello".into(),
        description: String::new(),
        author: String::new(),
        path: "payload/hello".into(),
        sha256: "a".repeat(64),
        abi_major: REGISTRY_ABI_MAJOR,
    };
    let mut manifest = RegistryManifest {
        version: REGISTRY_MANIFEST_VERSION,
        name: "test".into(),
        plugins: vec![plugin.clone(), plugin.clone()],
    };
    let err = validate_manifest(&manifest)
        .expect_err("duplicate")
        .to_string();
    assert!(err.contains("duplicate"), "{err}");

    for (path, sha, why) in [
        ("../escape", "a".repeat(64), "relative"),
        ("/absolute", "a".repeat(64), "relative"),
        ("payload/hello", "nothex".to_string(), "sha256"),
        ("payload/hello", "A".repeat(64), "sha256"),
    ] {
        manifest.plugins = vec![RegistryPlugin {
            path: path.into(),
            sha256: sha.clone(),
            ..plugin.clone()
        }];
        let err = validate_manifest(&manifest).expect_err(why).to_string();
        assert!(
            err.contains(why) || err.contains("sha256"),
            "{path}/{sha}: {err}"
        );
    }

    // A tagged id is not a registry entry: the commit is the version.
    manifest.plugins = vec![RegistryPlugin {
        id: "git.example/team/hello:v1".into(),
        ..plugin
    }];
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn manifest_rejects_unknown_fields() {
    let text = "version = 1\nname = \"test\"\nsurprise = true\n";
    let err = toml::from_str::<RegistryManifest>(text).expect_err("unknown field");
    assert!(err.to_string().contains("surprise"), "{err}");
}

#[test]
fn url_must_be_https_unless_local_is_explicit() {
    assert!(check_url("https://example.invalid/reg", false).is_ok());
    let err = check_url("http://example.invalid/reg", false)
        .expect_err("plain http")
        .to_string();
    assert!(err.contains("https://"), "{err}");
    assert!(check_url("/srv/registry", false).is_err());
    assert!(check_url("file:///srv/registry", false).is_err());
    // Explicit dev opt-in is the only way a non-HTTPS url is accepted.
    assert!(check_url("/srv/registry", true).is_ok());
    assert!(check_url("file:///srv/registry", true).is_ok());
    assert!(check_url("https:// example.invalid", true).is_err());
    assert!(check_url("", true).is_err());
}

#[test]
fn add_pins_commit_lists_and_installs() {
    let mut fixture = Fixture::new("add");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));

    let (config, mut store) = store();
    let entry = store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    assert_eq!(entry.pinned, fixture.head, "pin is the resolved commit");
    assert!(entry.local);

    let view = store.views();
    assert_eq!(view.len(), 1);
    assert!(view[0].error.is_none());
    assert_eq!(view[0].plugins.len(), 1);
    assert_eq!(view[0].plugins[0].id, "git.example/team/hello");
    assert!(view[0].plugins[0].description.is_empty());

    let installed = store.install("git.example/team/hello").expect("install");
    assert_eq!(installed.pinned, fixture.head);
    assert_eq!(installed.sha256, sha);
    let landed = config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"))
        .join(&fixture.head[..12]);
    assert_eq!(
        fs::read_to_string(landed.join("hello.txt")).expect("payload landed"),
        "hello v1\n"
    );

    // Re-installing the same commit is idempotent, not a second copy.
    let again = store.install("git.example/team/hello").expect("re-install");
    assert_eq!(again.pinned, installed.pinned);
    assert_eq!(store.installed().len(), 1);
}

#[test]
fn nested_payload_installs_from_a_git_archive_with_directory_members() {
    let mut fixture = Fixture::new("nested");
    fixture.commit_payload("payload/hello/sub/a.txt", "a v1\n");
    fixture.commit_payload("payload/hello/sub/deep/b.txt", "b v1\n");
    // `git archive` writes every directory as its own member named
    // `sub/` (typeflag '5'), so a payload with a nested file only
    // installs — and only digests — if those members are accepted.
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));

    let (config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    let installed = store
        .install("git.example/team/hello")
        .expect("install nested payload");
    assert_eq!(installed.sha256, sha, "verified digest of the nested tree");
    let version = config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"))
        .join(&fixture.head[..12]);
    assert_eq!(
        fs::read_to_string(version.join("sub/a.txt")).expect("nested file landed"),
        "a v1\n"
    );
    assert_eq!(
        fs::read_to_string(version.join("sub/deep/b.txt")).expect("deep file landed"),
        "b v1\n"
    );
}

#[test]
fn install_refuses_tampered_payload_and_writes_nothing() {
    let mut fixture = Fixture::new("tamper");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    // Manifest advertises a digest the payload does not have.
    fixture.write_manifest(&manifest_for(&"b".repeat(64)));

    let (config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    let err = store
        .install("git.example/team/hello")
        .expect_err("digest mismatch")
        .to_string();
    assert!(err.contains("sha256 mismatch"), "{err}");
    assert!(store.installed().is_empty());
    assert!(!config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"))
        .exists());
}

#[test]
fn failed_update_keeps_previous_version_and_lock() {
    let mut fixture = Fixture::new("update");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha_v1 = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha_v1));

    let (config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    store.install("git.example/team/hello").expect("install v1");
    let before = store.installed().to_vec();

    // Upstream moves, but v2's manifest digest does not match its bytes.
    fixture.commit_payload("payload/hello/hello.txt", "hello v2\n");
    let bad = "c".repeat(64);
    fixture.write_manifest(&manifest_for(&bad));

    let updated = store.update_registry(&fixture.url()).expect("re-pin");
    assert_eq!(updated.pinned, fixture.head);
    let err = store
        .update_plugin("git.example/team/hello")
        .expect_err("tampered v2")
        .to_string();
    assert!(err.contains("sha256 mismatch"), "{err}");

    // The v1 install and its lock entry are untouched.
    assert_eq!(store.installed(), before.as_slice());
    let v1 = config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"))
        .join(&before[0].pinned[..12]);
    assert_eq!(
        fs::read_to_string(v1.join("hello.txt")).expect("v1 intact"),
        "hello v1\n"
    );
    // A clean v2 does land, under its own pin.
    let sha_v2 = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha_v2));
    let updated = store.update_registry(&fixture.url()).expect("re-pin v2");
    let plugin = store
        .update_plugin("git.example/team/hello")
        .expect("update v2");
    assert_eq!(plugin.pinned, updated.pinned);
    let v2 = config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"))
        .join(&fixture.head[..12]);
    assert_eq!(
        fs::read_to_string(v2.join("hello.txt")).expect("v2 landed"),
        "hello v2\n"
    );
}

#[test]
fn update_at_same_pin_is_a_no_op() {
    let mut fixture = Fixture::new("noop");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));
    let (_config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    let installed = store.install("git.example/team/hello").expect("install");
    let again = store
        .update_plugin("git.example/team/hello")
        .expect("update at same pin");
    assert_eq!(again.pinned, installed.pinned);
    assert_eq!(again.installed_at, installed.installed_at);
}

#[test]
fn remote_enable_requires_an_install_and_survives_registry_removal() {
    let mut fixture = Fixture::new("enable");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));

    let (config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    assert!(matches!(
        store.set_remote_enabled("git.example/team/hello", true),
        Err(Error::UnknownPlugin(_))
    ));
    assert!(!store.remote_enabled("git.example/team/hello"));
    store.install("git.example/team/hello").expect("install");
    store
        .set_remote_enabled("git.example/team/hello", false)
        .expect("disable");
    assert!(!store.remote_enabled("git.example/team/hello"));
    store
        .set_remote_enabled("git.example/team/hello", true)
        .expect("enable");
    assert!(store.remote_enabled("git.example/team/hello"));

    // First-party state is a different store and must survive untouched.
    let plugins_toml = config.join("plugins.toml");
    fs::write(&plugins_toml, "disabled = [\"steam\"]\n").expect("first-party file");
    store
        .remove_registry(&fixture.url())
        .expect("remove registry");
    assert_eq!(
        fs::read_to_string(&plugins_toml).expect("first-party file"),
        "disabled = [\"steam\"]\n"
    );
    assert!(store.installed().is_empty());
    assert!(!store.views().first().map(|v| v.entry.url.clone()).is_some());
    assert!(!store.remote_enabled("git.example/team/hello"));
    let reloaded = RegistryStore::load_in(&config).expect("reload");
    assert!(reloaded.installed().is_empty());
    assert!(reloaded.views().is_empty());
}

#[test]
fn remove_plugin_drops_state_and_bytes() {
    let mut fixture = Fixture::new("remove");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));
    let (config, mut store) = store();
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    store.install("git.example/team/hello").expect("install");
    let dir = config
        .join("plugins/installed")
        .join(slug_for("git.example/team/hello"));
    assert!(dir.exists());
    store
        .remove_plugin("git.example/team/hello")
        .expect("remove");
    assert!(!dir.exists());
    assert!(store.installed().is_empty());
    assert!(matches!(
        store.remove_plugin("git.example/team/hello"),
        Err(Error::UnknownPlugin(_))
    ));
}

#[test]
fn unknown_plugin_and_unreadable_registry_are_distinct_errors() {
    let mut fixture = Fixture::new("unknown");
    fixture.commit_payload("payload/hello/hello.txt", "hello v1\n");
    let sha = tree_digest_of(&fixture, "payload/hello");
    fixture.write_manifest(&manifest_for(&sha));
    let (_config, mut store) = store();
    assert!(matches!(
        store.install("git.example/team/missing"),
        Err(Error::UnknownPlugin(_))
    ));
    store
        .add(&fixture.url(), "main", "registry.toml", true)
        .expect("add registry");
    assert!(matches!(
        store.install("git.example/team/missing"),
        Err(Error::UnknownPlugin(_))
    ));
    // Wiping the cache is an unreadable registry, not an empty catalog.
    let cache = fs::read_dir(_config.join("plugins/cache"))
        .expect("cache entries")
        .next()
        .expect("one cache entry")
        .expect("cache entry")
        .path();
    fs::remove_dir_all(&cache).expect("drop cache");
    let err = store
        .install("git.example/team/hello")
        .expect_err("cache gone")
        .to_string();
    assert!(err.contains("unreadable"), "{err}");
    let view = store.views();
    assert_eq!(view[0].plugins.len(), 0);
    assert!(view[0].error.is_some());
}

#[test]
fn in_repo_official_registry_manifest_is_valid() {
    // The seeded registry must stay parseable by this build's schema, or
    // adding it from Settings fails on first use.
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../registries/official.toml"),
    )
    .expect("registries/official.toml");
    let manifest: RegistryManifest = toml::from_str(&text).expect("seed manifest parses");
    validate_manifest(&manifest).expect("seed manifest is valid");
    assert_eq!(manifest.version, REGISTRY_MANIFEST_VERSION);
}

#[test]
fn untar_refuses_paths_that_escape_and_symlink_entries() {
    let dir = temp_config().join("untar");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("untar dir");
    let dest = dir.join("dest");
    fs::create_dir_all(&dest).expect("dest");

    let escape = dir.join("escape.tar");
    fs::write(
        &escape,
        tar_with(&[TarEntry::Regular("../outside.txt", b"x")]),
    )
    .expect("tar");
    let err = untar(&escape, &dest)
        .expect_err("escaping path")
        .to_string();
    assert!(err.contains("unsafe archive path"), "{err}");
    assert!(!dir.join("outside.txt").exists());

    let link = dir.join("link.tar");
    fs::write(
        &link,
        tar_with(&[TarEntry::Symlink("hello", "/etc/passwd")]),
    )
    .expect("tar");
    let err = untar(&link, &dest).expect_err("symlink").to_string();
    assert!(err.contains("unsupported entry"), "{err}");
    assert!(
        fs::read_dir(&dest).expect("dest readable").next().is_none(),
        "a rejected symlink entry must leave nothing behind"
    );
}

#[test]
fn untar_accepts_directory_members_and_still_refuses_traversal() {
    let dir = temp_config().join("untar-nested");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("untar dir");
    let dest = dir.join("dest");
    fs::create_dir_all(&dest).expect("dest");

    // A directory member carries a trailing slash, like `git archive`
    // writes it; the payload still lands under dest.
    let nested = dir.join("nested.tar");
    fs::write(
        &nested,
        tar_with(&[
            TarEntry::Dir("sub/"),
            TarEntry::Regular("sub/a.txt", b"a\n"),
            TarEntry::Dir("sub/deep/"),
            TarEntry::Regular("sub/deep/b.txt", b"b\n"),
        ]),
    )
    .expect("tar");
    untar(&nested, &dest).expect("directory members");
    assert_eq!(
        fs::read_to_string(dest.join("sub/deep/b.txt")).expect("nested file"),
        "b\n"
    );

    // The trailing slash is a marker, not a way past the checks: `..`,
    // an absolute root, and a doubled separator are still refused.
    for name in ["../outside/", "/etc/", "sub//deep/", "./sub/"] {
        let bad = dir.join("bad.tar");
        fs::write(&bad, tar_with(&[TarEntry::Dir(name)])).expect("tar");
        let err = untar(&bad, &dest)
            .expect_err("traversal via directory member")
            .to_string();
        assert!(err.contains("unsafe archive path"), "{name}: {err}");
    }
    assert!(!dir.join("outside").exists());
    assert!(fs::read_dir(&dest).expect("dest readable").next().is_some());
}

#[test]
fn digest_temp_dir_is_exclusive_private_and_skips_a_planted_name() {
    use std::os::unix::fs::PermissionsExt as _;
    // Another local user who guesses the name and pre-creates it must
    // not be able to redirect what gets written there.
    let prefix = "tuxgt-registry-test-planted";
    let planted = std::env::temp_dir().join(format!("{prefix}-{}-0", std::process::id()));
    let victim = temp_config().join("victim");
    fs::create_dir_all(&victim).expect("victim dir");
    let _ = fs::remove_file(&planted);
    std::os::unix::fs::symlink(&victim, &planted).expect("plant symlink");

    let dir = private_temp_dir(prefix).expect("private temp dir");
    let mode = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "temp dir must be owner-only");
    assert_ne!(dir, planted, "a planted name is never handed out");
    assert!(
        fs::symlink_metadata(&planted)
            .expect("plant still there")
            .file_type()
            .is_symlink(),
        "the planted symlink is left as it was"
    );
    assert!(
        fs::read_dir(&victim)
            .expect("victim readable")
            .next()
            .is_none(),
        "nothing is written through the planted symlink"
    );

    fs::remove_file(&planted).expect("drop planted symlink");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&victim);
}

enum TarEntry<'a> {
    Regular(&'a str, &'a [u8]),
    /// A directory member, named the way `git archive` names one.
    Dir(&'a str),
    Symlink(&'a str, &'a str),
}

fn tar_with(entries: &[TarEntry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        let (name, kind, body) = match entry {
            TarEntry::Regular(name, body) => (*name, b'0', body.to_vec()),
            TarEntry::Dir(name) => {
                // A directory member carries no data block; writing one
                // would read back as the end-of-archive marker.
                out.extend_from_slice(&tar_header(name, b'5', 0));
                continue;
            }
            TarEntry::Symlink(name, target) => {
                let mut header = tar_header(name, b'2', 0);
                header[100..100 + target.len()].copy_from_slice(target.as_bytes());
                out.extend_from_slice(&header);
                out.extend_from_slice(&[0u8; 1024]);
                continue;
            }
        };
        out.extend_from_slice(&tar_header(name, kind, body.len() as u64));
        out.extend_from_slice(&body);
        let pad = (512 - body.len() % 512) % 512;
        out.extend_from_slice(&vec![0u8; pad]);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

fn tar_header(name: &str, kind: u8, size: u64) -> [u8; 512] {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..107].copy_from_slice(b"0000644");
    header[108..115].copy_from_slice(b"0000000");
    header[116..123].copy_from_slice(b"0000000");
    let size_field = format!("{size:011o}");
    header[124..124 + size_field.len()].copy_from_slice(size_field.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].copy_from_slice(b"        ");
    header[156] = kind;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header
}
