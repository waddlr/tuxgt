use super::*;

#[test]
fn mod_id_requires_match_names_not_types() {
    let pkgs = [ModPackage {
        name: "mypack",
        type_: "custom",
        slot: None,
        requires: &[],
        requires_mods: &["reshade"],
    }];
    let d = diagnose(&pkgs).unwrap();
    assert_eq!(d.missing_requires, vec!["mypack requires missing reshade"]);
    // A same-named package of any type satisfies the Mod-id require.
    let pkgs = [
        ModPackage {
            name: "reshade-custom",
            type_: "custom",
            slot: None,
            requires: &[],
            requires_mods: &[],
        },
        ModPackage {
            name: "mypack",
            type_: "custom",
            slot: None,
            requires: &[],
            requires_mods: &["reshade-custom"],
        },
    ];
    let d = diagnose(&pkgs).unwrap();
    assert!(d.missing_requires.is_empty());
}

/// The `.addon32` ReShade addon (R38) is a payload exactly like the
/// 64-bit one: one predicate, so Add-form inference, classification,
/// the required-dest rule and harvest globs cannot disagree about it.
#[test]
fn is_addon_covers_32_64_and_neutral_forms() {
    for yes in [
        "ShaderToggler.addon32",
        "ShaderToggler.addon64",
        "ShaderToggler.addon",
        "shaderToggler.AddOn32",
        "Luma/CrimsonDeset.addon32",
    ] {
        assert!(is_addon(yes), "{yes} must count as an addon payload");
    }
    for no in [
        "ReShade32.dll",
        "OptiScaler.dll",
        "notes.txt",
        "x.addon32.zip",
        "addons.txt",
    ] {
        assert!(!is_addon(no), "{no} must not count as an addon payload");
    }
}
