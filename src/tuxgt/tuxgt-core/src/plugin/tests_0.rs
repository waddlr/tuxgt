use super::testing::*;
use super::*;
use crate::Error;
use std::fs;

#[test]
fn first_party_table_valid() {
    for d in FIRST_PARTY {
        validate_desc(d).expect("FIRST_PARTY");
    }
}

#[test]
fn parse_first_party_and_remote() {
    let steam = PluginId::parse("steam").unwrap();
    assert_eq!(steam.registry, None);
    assert_eq!(steam.name, "steam");
    assert_eq!(steam.tag, None);
    assert_eq!(steam.to_string(), "steam");

    let beta = PluginId::parse("steam:beta").unwrap();
    assert_eq!(beta.tag.as_deref(), Some("beta"));
    assert_eq!(beta.to_string(), "steam:beta");

    let remote = PluginId::parse("github.com/waddlr/lutris:v1.2.0").unwrap();
    assert_eq!(remote.registry.as_deref(), Some("github.com/waddlr"));
    assert_eq!(remote.name, "lutris");
    assert_eq!(remote.tag.as_deref(), Some("v1.2.0"));
    assert_eq!(remote.to_string(), "github.com/waddlr/lutris:v1.2.0");

    let branch = PluginId::parse("codeberg.org/u/p:feat/foo").unwrap();
    assert_eq!(branch.tag.as_deref(), Some("feat/foo"));

    assert!(PluginId::parse("core").is_err());
    assert!(PluginId::parse("github.com/waddlr").is_err());
    assert!(PluginId::parse("Steam").is_err());
    assert!(PluginId::parse("").is_err());
}

#[test]
fn empty_table_lists_nothing() {
    let dir = temp_config();
    let mut host = PluginHost::load_with(&[], &dir).unwrap();
    assert!(host.list().is_empty());
    assert!(matches!(
        host.set_enabled("steam", false).unwrap_err(),
        Error::UnknownPlugin(_)
    ));
}

#[test]
fn first_party_table() {
    let names: Vec<&str> = FIRST_PARTY.iter().map(|d| d.name).collect();
    assert_eq!(
        names,
        [
            "steam",
            "heroic",
            "manual",
            "env",
            "protondb",
            "steamgriddb",
            "awacy",
            "wrapper"
        ]
    );
    let dir = temp_config();
    let host = PluginHost::load_with(FIRST_PARTY, &dir).unwrap();
    assert!(host.is_enabled("steam"));
    assert!(host.is_enabled("heroic"));
    assert!(host.is_enabled("manual"));
    assert!(host.is_enabled("env"));
    assert!(host.is_enabled("protondb"));
    assert!(host.is_enabled("steamgriddb"));
    assert!(host.is_enabled("awacy"));
    assert!(host.is_enabled("wrapper"));
}

#[test]
fn disable_writes_toml_enable_clears() {
    let dir = temp_config();
    let descs = [
        fake("alpha", "plugin-alpha-label", None),
        fake("beta", "plugin-beta-label", Some(&["alpha"])),
    ];
    let mut host = PluginHost::load_with(&descs, &dir).unwrap();
    let listed = host.list();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|e| e.enabled));
    assert!(listed[1].desc.requires.is_some());

    host.set_enabled("alpha", false).unwrap();
    assert!(!host.is_enabled("alpha"));
    assert!(host.is_enabled("beta"));

    let text = fs::read_to_string(dir.join(PLUGINS_TOML)).unwrap();
    assert!(text.contains("alpha"));

    host.set_enabled("alpha", false).unwrap();
    host.set_enabled("alpha", true).unwrap();
    assert!(host.is_enabled("alpha"));
    let text = fs::read_to_string(dir.join(PLUGINS_TOML)).unwrap();
    let parsed: PluginsFile = toml::from_str(&text).unwrap();
    assert!(!parsed.disabled.iter().any(|id| id == "alpha"));
}

#[test]
fn rejects_empty_requires() {
    let dir = temp_config();
    let bad = [fake("alpha", "plugin-alpha-label", Some(&[]))];
    assert!(matches!(
        PluginHost::load_with(&bad, &dir).unwrap_err(),
        Error::InvalidPluginDesc(_)
    ));
}

#[test]
fn stale_disabled_kept_across_write() {
    let dir = temp_config();
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(PLUGINS_TOML),
        "disabled = [\"steam\", \"alpha\"]\n",
    )
    .unwrap();
    let descs = [fake("alpha", "plugin-alpha-label", None)];
    let mut host = PluginHost::load_with(&descs, &dir).unwrap();
    let listed = host.list();
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].enabled);

    host.set_enabled("alpha", false).unwrap();
    let parsed: PluginsFile =
        toml::from_str(&fs::read_to_string(dir.join(PLUGINS_TOML)).unwrap()).unwrap();
    assert!(parsed.disabled.iter().any(|id| id == "steam"));
    assert!(parsed.disabled.iter().any(|id| id == "alpha"));

    host.set_enabled("alpha", true).unwrap();
    let parsed: PluginsFile =
        toml::from_str(&fs::read_to_string(dir.join(PLUGINS_TOML)).unwrap()).unwrap();
    assert!(parsed.disabled.iter().any(|id| id == "steam"));
    assert!(!parsed.disabled.iter().any(|id| id == "alpha"));
    assert!(host.is_enabled("alpha"));
}

#[test]
fn duplicate_id_keeps_first() {
    let dir = temp_config();
    let descs = [
        fake("alpha", "plugin-alpha-label", None),
        fake("alpha", "plugin-alpha-label", Some(&["beta"])),
    ];
    let host = PluginHost::load_with(&descs, &dir).unwrap();
    let listed = host.list();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].desc.requires.is_none());
}

#[test]
fn first_party_rejects_registry() {
    let dir = temp_config();
    let mut d = fake("alpha", "plugin-alpha-label", None);
    d.registry = Some("github.com/waddlr");
    assert!(matches!(
        PluginHost::load_with(&[d], &dir).unwrap_err(),
        Error::InvalidPluginDesc(_)
    ));
}
