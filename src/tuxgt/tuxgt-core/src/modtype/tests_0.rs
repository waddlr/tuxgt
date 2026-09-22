use super::*;
use crate::Error;

#[test]
fn registry_has_six_types() {
    let names: Vec<&str> = MOD_TYPES.iter().map(|t| t.mod_type()).collect();
    assert_eq!(
        names,
        [
            "reshade",
            "optiscaler",
            "reshade_addon",
            "custom",
            "effect",
            "texture"
        ]
    );
    for t in MOD_TYPES {
        if let Some(reqs) = t.requires() {
            assert!(!reqs.is_empty());
        }
    }
}

#[test]
fn effect_texture_are_reshade_shaders() {
    for (name, root) in [
        ("effect", "reshade-shaders/Shaders"),
        ("texture", "reshade-shaders/Textures"),
    ] {
        let t = parse_mod_type(name).unwrap();
        assert_eq!(t.requires(), Some(RESHADE_REQUIRES), "{name}");
        assert_eq!(t.default_slot(), None, "{name}");
        assert_eq!(t.dest_root(), Some(root), "{name}");
    }
    for name in ["reshade", "optiscaler", "reshade_addon", "custom"] {
        assert_eq!(parse_mod_type(name).unwrap().dest_root(), None, "{name}");
    }
}

#[test]
fn dest_for_prefixes_and_strips_once() {
    let shaders = Some("reshade-shaders/Shaders");
    let textures = Some("reshade-shaders/Textures");
    assert_eq!(dest_for(None, "Shaders/a.fx", None, None), "Shaders/a.fx");
    assert_eq!(
        dest_for(shaders, "Shaders/a.fx", None, None),
        "reshade-shaders/Shaders/a.fx"
    );
    assert_eq!(
        dest_for(shaders, "shaders/lilium__include/cas.fxh", None, None),
        "reshade-shaders/Shaders/lilium__include/cas.fxh"
    );
    assert_eq!(
        dest_for(shaders, "reshade-shaders/Shaders/a.fx", None, None),
        "reshade-shaders/Shaders/a.fx"
    );
    assert_eq!(
        dest_for(shaders, "reshade-shaders/a.fx", None, None),
        "reshade-shaders/Shaders/a.fx"
    );
    assert_eq!(
        dest_for(shaders, "a.fx", None, None),
        "reshade-shaders/Shaders/a.fx"
    );
    // Both ReShade roots land in their own shared dir, whatever the type.
    assert_eq!(
        dest_for(shaders, "Textures/noise.png", None, None),
        "reshade-shaders/Textures/noise.png"
    );
    assert_eq!(
        dest_for(shaders, "reshade-shaders/Textures/noise.png", None, None),
        "reshade-shaders/Textures/noise.png"
    );
    assert_eq!(
        dest_for(textures, "Textures/noise.png", None, None),
        "reshade-shaders/Textures/noise.png"
    );
    assert_eq!(
        dest_for(textures, "noise.png", None, None),
        "reshade-shaders/Textures/noise.png"
    );
    assert_eq!(
        dest_for(textures, "Shaders/a.fx", None, None),
        "reshade-shaders/Shaders/a.fx"
    );
    assert_eq!(
        dest_for(shaders, "Shaders/foo.fx", Some("OtisFX"), Some("OtisFX")),
        "reshade-shaders/Shaders/OtisFX/foo.fx"
    );
    assert_eq!(
        dest_for(shaders, "Textures/noise.png", Some("qUINT"), None),
        "reshade-shaders/Textures/noise.png"
    );
    assert_eq!(
        dest_for(shaders, "a.fx", Some("OtisFX"), None),
        "reshade-shaders/Shaders/OtisFX/a.fx"
    );
    assert_eq!(
        dest_for(None, "Shaders/a.fx", Some("OtisFX"), None),
        "Shaders/a.fx"
    );
}

#[test]
fn dest_for_keeps_ini_at_root() {
    let shaders = Some("reshade-shaders/Shaders");
    assert_eq!(dest_for(shaders, "foo.ini", None, None), "foo.ini");
    assert_eq!(dest_for(shaders, "Shaders/x.ini", None, None), "x.ini");
    assert_eq!(
        dest_for(shaders, "Docs/readme.txt", None, None),
        "reshade-shaders/Shaders/Docs/readme.txt"
    );
}

#[test]
fn optiscaler_dest_keeps_injector_name() {
    let mut dests = vec![
        "OptiScaler.dll".into(),
        "OptiScaler.ini".into(),
        "amd_fidelityfx_dx12.dll".into(),
        "Licenses/DirectX_LICENSE.txt".into(),
    ];
    apply_type_dests("optiscaler", &mut dests).unwrap();
    assert_eq!(
        dests,
        [
            "OptiScaler.dll",
            "OptiScaler.ini",
            "amd_fidelityfx_dx12.dll",
            "Licenses/DirectX_LICENSE.txt"
        ]
    );
}

#[test]
fn optiscaler_dest_is_case_insensitive_basename() {
    let mut dests = vec!["lib/optiscaler.DLL".into(), "OptiScaler.ini".into()];
    apply_type_dests("optiscaler", &mut dests).unwrap();
    assert_eq!(dests[0], "lib/optiscaler.DLL");
    assert_eq!(dests[1], "OptiScaler.ini");
}

#[test]
fn optiscaler_dest_zero_or_two_errors() {
    let mut none = vec!["OptiScaler.ini".into()];
    let err = apply_type_dests("optiscaler", &mut none).unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let mut two = vec!["OptiScaler.dll".into(), "extra/OptiScaler.dll".into()];
    let err = apply_type_dests("optiscaler", &mut two).unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
}

#[test]
fn custom_never_gets_optiscaler_dest() {
    let mut dests = vec!["OptiScaler.dll".into()];
    apply_type_dests("custom", &mut dests).unwrap();
    apply_type_dests("reshade", &mut dests).unwrap();
    assert_eq!(dests[0], "OptiScaler.dll");
}

#[test]
fn explicit_dests_win_over_type_dests() {
    let srcs = vec!["OptiScaler.dll".into(), "extra.ini".into()];
    let skip = vec![true, false];
    let mut dests = vec!["custom.dll".into(), "extra.ini".into()];
    apply_type_dests_except("optiscaler", &mut dests, &skip, &srcs).unwrap();
    assert_eq!(dests[0], "custom.dll");
    assert_eq!(dests[1], "extra.ini");
    let mut mapped = vec!["dxgi.dll".into(), "extra.ini".into()];
    apply_type_dests_except("optiscaler", &mut mapped, &skip, &srcs).unwrap();
    assert_eq!(mapped[0], "dxgi.dll");
}

#[test]
fn two_optiscaler_dlls_error_unless_dests_differ() {
    let srcs = vec!["a/OptiScaler.dll".into(), "b/OptiScaler.dll".into()];
    let skip = vec![true, true];
    let mut same = vec!["dxgi.dll".into(), "dxgi.dll".into()];
    let err = apply_type_dests_except("optiscaler", &mut same, &skip, &srcs).unwrap_err();
    assert!(matches!(err, Error::InvalidInstance(_)), "{err}");
    let mut differ = vec!["dxgi.dll".into(), "custom.dll".into()];
    apply_type_dests_except("optiscaler", &mut differ, &skip, &srcs).unwrap();
    assert_eq!(differ[0], "dxgi.dll");
    assert_eq!(differ[1], "custom.dll");
}

#[test]
fn addon_requires_reshade_others_none() {
    assert_eq!(parse_mod_type("reshade").unwrap().requires(), None);
    assert_eq!(parse_mod_type("optiscaler").unwrap().requires(), None);
    assert_eq!(
        parse_mod_type("reshade_addon").unwrap().requires(),
        Some(RESHADE_REQUIRES)
    );
    assert_eq!(parse_mod_type("custom").unwrap().requires(), None);
    assert_eq!(
        parse_mod_type("effect").unwrap().requires(),
        Some(RESHADE_REQUIRES)
    );
    assert_eq!(
        parse_mod_type("texture").unwrap().requires(),
        Some(RESHADE_REQUIRES)
    );
}

#[test]
fn reserved_rejected_as_unknown() {
    for name in ["env_tool", "injector", "custom_dll", ""] {
        let err = parse_mod_type(name).err().expect("must err");
        assert!(matches!(err, Error::InvalidModType(_)), "{name}");
    }
}

#[test]
fn slots_parse_closed() {
    assert_eq!(parse_slot("dxgi").unwrap(), ProxySlot::Dxgi);
    assert_eq!(parse_slot("DXGI.dll").unwrap(), ProxySlot::Dxgi);
    assert_eq!(parse_slot("d3d9").unwrap(), ProxySlot::D3d9);
    assert_eq!(parse_slot("d3d10.dll").unwrap(), ProxySlot::D3d10);
    assert_eq!(parse_slot("d3d12").unwrap(), ProxySlot::D3d12);
    assert!(parse_slot("dinput8").is_err());
    assert!(parse_slot("").is_err());
}

#[test]
fn slot_dll_normalizes_stem() {
    assert_eq!(slot_dll("dxgi").unwrap(), "dxgi.dll");
    assert_eq!(slot_dll("winmm.dll").unwrap(), "winmm.dll");
    assert_eq!(slot_dll("D3D12.DLL").unwrap(), "d3d12.dll");
    assert!(slot_dll("dinput8").is_err());
}

#[test]
fn custom_has_no_default_slot() {
    assert_eq!(parse_mod_type("custom").unwrap().default_slot(), None);
    assert_eq!(parse_mod_type("optiscaler").unwrap().default_slot(), None);
    assert_eq!(parse_mod_type("reshade").unwrap().default_slot(), None);
    assert_eq!(
        parse_mod_type("reshade_addon").unwrap().default_slot(),
        None
    );
}

#[test]
fn fixture_conflict_no_missing() {
    let pkgs = fixture_packages();
    let d = diagnose(&pkgs).unwrap();
    assert!(d.missing_requires.is_empty());
    assert_eq!(d.slot_conflicts.len(), 1);
    assert!(d.slot_conflicts[0].contains("dxgi"));
}

#[test]
fn missing_requires_reported_not_enabled() {
    let pkgs = [ModPackage {
        name: "example-addon",
        type_: "reshade_addon",
        slot: None,
        requires: RESHADE_REQUIRES,
        requires_mods: &[],
    }];
    let d = diagnose(&pkgs).unwrap();
    assert_eq!(d.missing_requires.len(), 1);
    let d2 = diagnose(&pkgs).unwrap();
    assert_eq!(d2.missing_requires, d.missing_requires);
}

#[test]
fn type_level_requires_apply_with_empty_list() {
    let pkgs = [ModPackage {
        name: "example-addon",
        type_: "reshade_addon",
        slot: None,
        requires: &[],
        requires_mods: &[],
    }];
    let d = diagnose(&pkgs).unwrap();
    assert_eq!(d.missing_requires.len(), 1);
}

#[test]
fn diagnose_rejects_unknown_type() {
    let pkgs = [ModPackage {
        name: "x",
        type_: "injector",
        slot: None,
        requires: &[],
        requires_mods: &[],
    }];
    assert!(diagnose(&pkgs).is_err());
}

#[test]
fn stock_and_asi_claim_no_slot() {
    let pkgs = [
        ModPackage {
            name: "reshade",
            type_: "reshade",
            slot: None,
            requires: &[],
            requires_mods: &[],
        },
        ModPackage {
            name: "example-addon",
            type_: "reshade_addon",
            slot: None,
            requires: RESHADE_REQUIRES,
            requires_mods: &[],
        },
    ];
    let d = diagnose(&pkgs).unwrap();
    assert!(d.slot_conflicts.is_empty());
    assert!(d.missing_requires.is_empty());
}

#[test]
fn custom_reshade_satisfies_addon_requires() {
    let pkgs = [
        ModPackage {
            name: "reshade-custom",
            type_: "reshade",
            slot: None,
            requires: &[],
            requires_mods: &[],
        },
        ModPackage {
            name: "example-addon",
            type_: "reshade_addon",
            slot: None,
            requires: &["reshade"],
            requires_mods: &[],
        },
    ];
    let d = diagnose(&pkgs).unwrap();
    assert!(d.missing_requires.is_empty());
}

fn rw_ctx(dir: &str) -> StagingRewriteContext {
    StagingRewriteContext {
        game_id: "steam::1".into(),
        instance_id: "opti".into(),
        runtime_dir: std::path::PathBuf::from(dir),
    }
}

const INI_WITH_TOKEN: &[u8] = b"\
; comment line
[Log]
LogToFile=auto
LogFileName=@TUXGT_RUNTIME@/logs/OptiScaler.log

[NvApi]
DisableReflexSync=true
";

#[test]
fn optiscaler_ini_token_becomes_absolute_runtime_path() {
    let t = parse_mod_type("optiscaler").unwrap();
    let ctx = rw_ctx("/home/u/tuxgt/games/steam/1/runtime");
    let out = t
        .staging_rewrite(&ctx, "OptiScaler.ini", INI_WITH_TOKEN)
        .unwrap()
        .expect("applicable");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("LogFileName=Z:/home/u/tuxgt/games/steam/1/runtime/logs/OptiScaler.log"),
        "{text}"
    );
    // Unrelated lines, comments, spacing, and section order survive.
    assert!(
        text.starts_with("; comment line\n[Log]\nLogToFile=auto\n"),
        "{text}"
    );
    assert!(
        text.ends_with("[NvApi]\nDisableReflexSync=true\n"),
        "{text}"
    );
}

#[test]
fn optiscaler_rewrite_is_case_insensitive_and_crlf_safe() {
    let t = parse_mod_type("optiscaler").unwrap();
    let ctx = rw_ctx("/rt");
    let src = b"[log]\r\nLOGFILENAME = @TUXGT_RUNTIME@/x.log ; keep\r\n";
    let out = t
        .staging_rewrite(&ctx, "sub/dir/optiscaler.INI", src)
        .unwrap()
        .expect("applicable");
    assert_eq!(
        out,
        b"[log]\r\nLOGFILENAME = Z:/rt/x.log ; keep\r\n".to_vec()
    );
}

#[test]
fn optiscaler_rewrite_preserves_bom_and_substitutes_every_log_line() {
    let t = parse_mod_type("optiscaler").unwrap();
    let ctx = rw_ctx("/rt");
    let mut src = b"\xef\xbb\xbf[Log]\nLogFileName=@TUXGT_RUNTIME@\n".to_vec();
    src.extend_from_slice(b"[NvApi]\nX=1\n[Log]\nLogFileName=@TUXGT_RUNTIME@.log\n");
    let out = t
        .staging_rewrite(&ctx, "OptiScaler.ini", &src)
        .unwrap()
        .expect("applicable");
    assert_eq!(
        out,
        b"\xef\xbb\xbf[Log]\nLogFileName=Z:/rt\n[NvApi]\nX=1\n[Log]\nLogFileName=Z:/rt.log\n"
            .to_vec()
    );
}

#[test]
fn optiscaler_rewrite_not_applicable_without_token_or_in_log_section() {
    let t = parse_mod_type("optiscaler").unwrap();
    let ctx = rw_ctx("/rt");
    // Token outside [Log]: untouched.
    let other = b"[Menu]\nTTFFontPath=@TUXGT_RUNTIME@/f.ttf\n";
    assert_eq!(
        t.staging_rewrite(&ctx, "OptiScaler.ini", other).unwrap(),
        None
    );
    // Wrong token case is never substituted.
    let case = b"[Log]\nLogFileName=@tuxgt_runtime@/x\n";
    assert_eq!(
        t.staging_rewrite(&ctx, "OptiScaler.ini", case).unwrap(),
        None
    );
    // Stock OptiScaler.ini (LogFileName=auto) is a no-op.
    let stock = b"[Log]\nLogFileName=auto\n";
    assert_eq!(
        t.staging_rewrite(&ctx, "OptiScaler.ini", stock).unwrap(),
        None
    );
}

#[test]
fn rewrite_gate_only_claims_the_optiscaler_ini() {
    let opti = parse_mod_type("optiscaler").unwrap();
    for rel in ["OptiScaler.ini", "optiscaler.INI", "sub/OptiScaler.ini"] {
        assert!(opti.staging_rewrite_applies(rel), "{rel}");
    }
    for rel in ["dxgi.dll", "OptiScaler.dll", "ReShade.ini", "notes.txt"] {
        assert!(!opti.staging_rewrite_applies(rel), "{rel}");
    }
    for name in ["reshade", "reshade_addon", "custom", "effect", "texture"] {
        let t = parse_mod_type(name).unwrap();
        assert!(!t.staging_rewrite_applies("OptiScaler.ini"), "{name}");
    }
}

#[test]
fn other_types_never_rewrite_and_optiscaler_rejects_non_utf8() {
    let ctx = rw_ctx("/rt");
    for name in ["reshade", "custom", "effect", "texture", "reshade_addon"] {
        let t = parse_mod_type(name).unwrap();
        assert_eq!(
            t.staging_rewrite(&ctx, "OptiScaler.ini", INI_WITH_TOKEN)
                .unwrap(),
            None
        );
    }
    let opti = parse_mod_type("optiscaler").unwrap();
    // A payload claimed as OptiScaler.ini but not valid UTF-8 fails closed
    // instead of staging corrupted text.
    let err = opti
        .staging_rewrite(&ctx, "OptiScaler.ini", b"[Log]\nLogFileName=\xff\xfe")
        .unwrap_err();
    assert!(matches!(err, Error::Manifest(_)), "{err}");
}
