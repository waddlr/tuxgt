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
