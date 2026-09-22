use super::*;

fn r38_pkg(name: &str, url: Option<&str>, url32: Option<&str>) -> ReshadePackage {
    ReshadePackage {
        kind: ReshadePackageKind::Effect,
        name: name.into(),
        description: String::new(),
        url: url.map(str::to_string),
        url32: url32.map(str::to_string),
        repository_url: None,
        shader_dir: None,
        texture_dir: None,
        deny_files: Box::default(),
        effect_files: Box::default(),
        in_catalog: false,
        in_catalog_32: false,
    }
}

fn listed_mod(id: &str, source: crate::instance::SourceRef) -> crate::instance::Mod {
    crate::instance::Mod {
        id: id.into(),
        mod_type: "effect".into(),
        label: id.into(),
        description: String::new(),
        source,
        plans_allowed: Box::default(),
        sha256: None,
        payload: Box::default(),
        games: Box::default(),
        appids: Box::default(),
        requires: Box::default(),
        dests: Default::default(),
        slot: None,
        include: Box::default(),
        env: Default::default(),
        shader_dir: None,
        texture_dir: None,
        effect_files: Box::default(),
        official: false,
        registry: None,
        enabled: true,
    }
}

fn manual(url: &str) -> crate::instance::SourceRef {
    crate::instance::SourceRef::ManualUrl { url: url.into() }
}

/// R38: a 32-bit recipe locks only the 32-bit row; the 64-bit row of the
/// same package stays mintable, and vice versa.
#[test]
fn arch_catalog_lock_is_per_arch() {
    let pkg = r38_pkg(
        "X",
        Some("https://example.com/x64.zip"),
        Some("https://example.com/x32.zip"),
    );
    let m32 = [listed_mod("x-x32", manual("https://example.com/x32.zip"))];
    assert!(package_in_catalog_for_arch(&pkg, &m32, "32"));
    assert!(!package_in_catalog_for_arch(&pkg, &m32, "64"));
    let m64 = [listed_mod("x-x64", manual("https://example.com/x64.zip"))];
    assert!(package_in_catalog_for_arch(&pkg, &m64, "64"));
    assert!(!package_in_catalog_for_arch(&pkg, &m64, "32"));
    // Both variants listed: both rows lock, still independently.
    let both = [
        listed_mod("x-x32", manual("https://example.com/x32.zip")),
        listed_mod("x-x64", manual("https://example.com/x64.zip")),
    ];
    assert!(package_in_catalog_for_arch(&pkg, &both, "32"));
    assert!(package_in_catalog_for_arch(&pkg, &both, "64"));
}

/// R38: github per-arch assets lock only their own arch, even inside one
/// repo that ships both (the cot6 same-repo shape).
#[test]
fn arch_catalog_lock_github_asset_per_arch() {
    let mut pkg = r38_pkg(
        "A",
        Some("https://github.com/o/r/releases/download/t/a-x64.zip"),
        Some("https://github.com/o/r/releases/download/t/a-x32.zip"),
    );
    pkg.repository_url = Some("https://github.com/o/r".into());
    let gh = |asset: &str| crate::instance::SourceRef::Github {
        owner: "o".into(),
        repo: "r".into(),
        asset_glob: asset.into(),
        tag: Some("t".into()),
        prerelease: false,
    };
    let m64 = [listed_mod("a-x64", gh("a-x64.zip"))];
    assert!(package_in_catalog_for_arch(&pkg, &m64, "64"));
    assert!(!package_in_catalog_for_arch(&pkg, &m64, "32"));
    let m32 = [listed_mod("a-x32", gh("a-x32.zip"))];
    assert!(package_in_catalog_for_arch(&pkg, &m32, "32"));
    assert!(!package_in_catalog_for_arch(&pkg, &m32, "64"));
}

/// R38: no `DownloadUrl32` → the 32-bit row can never read locked, even
/// when the repo matches a listed mod (there is no 32-bit payload).
#[test]
fn arch_catalog_lock_no_url32_never_locks_32() {
    let mut pkg = r38_pkg("X", Some("https://example.com/x64.zip"), None);
    pkg.repository_url = Some("https://github.com/example/ExampleShaders".into());
    let listed = [listed_mod(
        "my-effect",
        crate::instance::SourceRef::Github {
            owner: "example".into(),
            repo: "ExampleShaders".into(),
            asset_glob: "Shaders.zip".into(),
            tag: None,
            prerelease: false,
        },
    )];
    assert!(!package_in_catalog_for_arch(&pkg, &listed, "32"));
}

/// R38: an un-suffixed legacy recipe (base id, 64-bit URL) locks
/// neither row — minting migrates it to `B-x64` first, which then
/// locks the 64-bit row. Without the skip the locked row could never
/// be selected and migration would never trigger.
#[test]
fn arch_catalog_lock_skips_legacy_base_id() {
    let pkg = r38_pkg(
        "X",
        Some("https://example.com/x64.zip"),
        Some("https://example.com/x32.zip"),
    );
    let legacy = [listed_mod("x", manual("https://example.com/x64.zip"))];
    assert!(!package_in_catalog_for_arch(&pkg, &legacy, "64"));
    assert!(!package_in_catalog_for_arch(&pkg, &legacy, "32"));
    let migrated = [listed_mod("x-x64", manual("https://example.com/x64.zip"))];
    assert!(package_in_catalog_for_arch(&pkg, &migrated, "64"));
    assert!(!package_in_catalog_for_arch(&pkg, &migrated, "32"));
}

/// R38: `mintable_for_arch` is the row gate the GUI paints per target.
#[test]
fn mintable_for_arch_matrix() {
    let pkg = r38_pkg(
        "X",
        Some("https://example.com/x64.zip"),
        Some("https://example.com/x32.zip"),
    );
    assert!(pkg.mintable_for_arch("64"));
    assert!(pkg.mintable_for_arch("32"));
    let mut locked64 = pkg.clone();
    locked64.in_catalog = true;
    assert!(!locked64.mintable_for_arch("64"));
    assert!(locked64.mintable_for_arch("32"));
    let mut locked32 = pkg.clone();
    locked32.in_catalog_32 = true;
    assert!(locked32.mintable_for_arch("64"));
    assert!(!locked32.mintable_for_arch("32"));
    let no32 = r38_pkg("Y", Some("https://example.com/y64.zip"), None);
    assert!(no32.mintable_for_arch("64"));
    assert!(!no32.mintable_for_arch("32"));
    let no_url = r38_pkg("Z", None, None);
    assert!(!no_url.mintable_for_arch("64"));
    assert!(!no_url.mintable_for_arch("32"));
}
