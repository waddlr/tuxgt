use super::*;
use crate::instance::{parse_recipe, SourceRef};

#[test]
fn parse_accepts_official_shapes_and_rejects_bad_keys() {
    let nv = parse_recipe(&official_text("nvngx-dlssnr"), true).unwrap();
    assert_eq!(nv.mod_type, "custom");
    assert_eq!(nv.label, "nvngx_dlssnr");
    assert_eq!(&nv.include[..], ["nvngx_dlssnr.dll"]);
    match &nv.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["nvngx_dlssnr.dll"]);
            assert_eq!(
                note,
                "NVIDIA DLSS neural-rendering library (nvngx_dlssnr.dll). TuxGT does not download it."
            );
        }
        other => panic!("{other:?}"),
    }
    let reno = parse_recipe(&official_text("renodx-dlss"), true).unwrap();
    assert_eq!(reno.mod_type, "reshade_addon");
    assert_eq!(
        &reno.requires[..],
        ["d3dcompiler-47", "nvidia-streamline", "nvngx-dlssnr-proxy"]
    );
    match &reno.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["renodx-dlss.addon64"]);
            assert!(note.contains("does not download"));
        }
        other => panic!("{other:?}"),
    }
    let sl = parse_recipe(&official_text("nvidia-streamline"), true).unwrap();
    assert_eq!(sl.mod_type, "custom");
    assert_eq!(&sl.include[..], STREAM);
    match &sl.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], STREAM);
            assert_eq!(
                note,
                "NVIDIA Streamline runtime. TuxGT does not download it."
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(nv.sha256.is_none() && sl.games.is_empty() && sl.appids.is_empty());
    assert!(nv.env.is_empty() && sl.plans_allowed.iter().all(|p| p.as_str() != "proton_env"));

    let one = r#"
id = "onepin"
type = "custom"
label = "one"
sha256 = "abc"
[source]
type = "provided"
files = ["a.dll"]
"#;
    let parsed = parse_recipe(one, false).unwrap();
    assert_eq!(parsed.sha256.as_deref(), Some("abc"));
    match parsed.source {
        SourceRef::Provided { note, .. } => assert!(note.is_empty()),
        other => panic!("{other:?}"),
    }

    let bad = [
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["a.dll"]
url = "https://example.invalid/a.dll"
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["a.dll"]
path = "/tmp/a.dll"
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = []
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["../a.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["foo?.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
[source]
type = "provided"
files = ["bin/a.dll"]
"#,
        r#"
id = "p"
type = "custom"
label = "p"
sha256 = "abc"
[source]
type = "provided"
files = ["a.dll", "b.dll"]
"#,
    ];
    for text in bad {
        assert!(parse_recipe(text, false).is_err(), "{text}");
    }
}

#[test]
fn parse_accepts_official_dfc_proxy_y4my4m() {
    let dfc64 = parse_recipe(&official_text("deep-fried-chicken-64bit"), true).unwrap();
    assert_eq!(dfc64.id, "deep-fried-chicken-64bit");
    assert_eq!(dfc64.mod_type, "reshade_addon");
    assert_eq!(dfc64.label, "Deep Fried Chicken v3 - 64bit");
    assert_eq!(
        &dfc64.include[..],
        [
            "deep-fried-chicken-nvngx.dll",
            "deep-fried-chicken-present-support.dll"
        ]
    );
    assert!(dfc64.dests.is_empty());
    assert_eq!(dfc64.slot, None);
    assert_eq!(
        &dfc64.requires[..],
        ["d3dcompiler-47", "nvidia-streamline", "nvngx-dlssnr-proxy"]
    );
    match &dfc64.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(
                &files[..],
                [
                    "deep-fried-chicken.addon64",
                    "deep-fried-chicken.cfg",
                    "deep-fried-chicken-nvngx.dll",
                    "deep-fried-chicken-present-support.dll"
                ]
            );
            assert!(note.contains("64-bit"));
        }
        other => panic!("{other:?}"),
    }
    let proxy = parse_recipe(&official_text("nvngx-dlssnr-proxy"), true).unwrap();
    assert_eq!(proxy.id, "nvngx-dlssnr-proxy");
    assert_eq!(proxy.mod_type, "custom");
    assert_eq!(proxy.label, "nvngx_dlssnr - workaround proxy");
    assert_eq!(
        &proxy.include[..],
        ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]
    );
    assert_eq!(proxy.slot, None);
    match &proxy.source {
        SourceRef::Provided { files, note } => {
            assert_eq!(&files[..], ["nvngx_dlssnr.dll", "nvngx_dlssnr.real.dll"]);
            assert_eq!(
                note,
                "Proxy with renamed original. Use with ReShade DLSS-NR mods. Do not also install nvngx-dlssnr: same dest, load order decides."
            );
        }
        other => panic!("{other:?}"),
    }
    let fork = parse_recipe(&official_text("optiscaler-y4my4m-v4"), true).unwrap();
    assert_eq!(fork.id, "optiscaler-y4my4m-v4");
    assert_eq!(fork.mod_type, "optiscaler");
    assert_eq!(fork.label, "OptiScaler - y4my4m - v4");
    assert_eq!(fork.slot.as_deref(), Some("dxgi"));
    assert_eq!(&fork.include[..], ["OptiScaler/", "nvngx.dll_dlssnr.dll"]);
    assert!(fork
        .plans_allowed
        .iter()
        .all(|p| p.as_str() != "proton_env"));
    assert!(fork
        .payload
        .iter()
        .flat_map(|r| r.drop.iter())
        .any(|g| g == "Licenses/*"));
    match &fork.source {
        SourceRef::Github {
            owner,
            repo,
            asset_glob,
            tag,
            prerelease,
        } => {
            assert_eq!(owner, "y4my4my4m");
            assert_eq!(repo, "OptiScaler_DLSSNR_Multipass_MFG");
            assert_eq!(
                asset_glob,
                "OptiScaler_v10.0.0-dev-fork-y4my4my4m-v4_20260905_with_DLSS.7z"
            );
            assert_eq!(tag.as_deref(), Some("v10.0.0-dev-fork-y4my4my4m-v4"));
            assert!(!prerelease);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn official_proxy_recipes_declare_slot() {
    for id in ["reshade", "optiscaler", "optiscaler-y4my4m-v4"] {
        let m = parse_recipe(&official_text(id), true).unwrap();
        assert_eq!(m.slot.as_deref(), Some("dxgi"), "{id}");
    }
    for id in [
        "d3dcompiler-47",
        "nvngx-dlssnr",
        "nvngx-dlssnr-proxy",
        "nvidia-streamline",
        "renodx-dlss",
        "deep-fried-chicken-64bit",
    ] {
        let m = parse_recipe(&official_text(id), true).unwrap();
        assert_eq!(m.slot, None, "{id}");
    }
}
