use super::*;
use crate::Error;

#[test]
fn env_key_grammar() {
    assert!(valid_env_key("PROTON_LOG"));
    assert!(valid_env_key("_x"));
    assert!(!valid_env_key("1X"));
    assert!(!valid_env_key("A-B"));
    assert!(!valid_env_key(""));
}

#[test]
fn knob_table_is_well_formed() {
    let mut seen = std::collections::BTreeSet::new();
    for k in KNOBS {
        assert!(!k.id.is_empty());
        assert!(k.id.len() <= 32);
        let first = k.id.chars().next().unwrap();
        assert!(first.is_ascii_lowercase());
        assert!(k
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert!(!k.help.is_empty(), "empty help: {}", k.id);
        if let Some(var) = k.freeform {
            assert!(valid_env_key(var), "bad freeform var: {var}");
            assert!(k.values.is_empty(), "freeform with values: {}", k.id);
        } else {
            assert!(!k.values.is_empty(), "no values: {}", k.id);
            for v in k.values {
                assert!(!v.value.is_empty());
                assert!(!v.help.is_empty(), "empty value help: {}", k.id);
                assert!(!v.env.is_empty(), "no env pairs: {}", k.id);
                for (var, val) in v.env {
                    assert!(valid_env_key(var), "bad var: {var}");
                    assert!(!val.is_empty(), "empty env value: {}", k.id);
                }
            }
        }
        assert!(seen.insert(k.id), "duplicate knob id: {}", k.id);
    }
    assert!(find_knob("mangohud").is_some());
    assert!(find_knob("nope-knob").is_none());
    assert!(find_knob("proton-log").is_some());
    assert!(find_knob("nvidia-shader-cache-size").is_some());
    assert_eq!(
        find_knob("nvidia-shader-cache-size").unwrap().freeform,
        Some("__GL_SHADER_DISK_CACHE_SIZE")
    );
    for id in [
        "proton-local-shader-cache",
        "proton-nvidia-libs",
        "proton-nvidia-libs-no-32bit",
        "proton-nvidia-nvoptix",
        "low-latency-layer-reflex",
    ] {
        let k = find_knob(id).unwrap();
        assert!(k.ge_cachy_only(), "{id}");
    }
    assert!(find_knob("proton-wayland").unwrap().ge_cachy_only());
    assert!(!find_knob("dxvk-hud").unwrap().ge_cachy_only());
    assert_eq!(find_knob("dxvk-hud").unwrap().env_label(), "DXVK_HUD");
    assert_eq!(
        find_knob("proton-fsr4").unwrap().env_label(),
        "PROTON_FSR4_UPGRADE"
    );
    let offload = find_knob("nvidia-prime-offload").unwrap().env_label();
    assert!(offload.contains(" · "), "{offload}");
    assert!(proton_ge_cachy(Some("GE-Proton10-4")));
    assert!(proton_ge_cachy(Some(
        "/usr/share/steam/compatibilitytools.d/proton-cachyos"
    )));
    assert!(!proton_ge_cachy(Some("proton_9")));
}

#[test]
fn disabled_plugin_hides_knobs() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e17d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut host = crate::plugin::PluginHost::load_with(crate::plugin::FIRST_PARTY, &dir).unwrap();
    assert!(!enabled_knobs(&host).is_empty());
    assert!(find_enabled_knob(&host, "mangohud").is_some());

    host.set_enabled("env", false).unwrap();
    assert!(enabled_knobs(&host).is_empty());
    assert!(find_enabled_knob(&host, "mangohud").is_none());
    // table lookup stays intact for E06 (launch never re-offers)
    assert!(find_knob("mangohud").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scope_rules() {
    let any: &[Scope] = &[];
    assert!(scope_applies(any, "native"));
    assert!(scope_applies(any, "proton"));
    let pw: &[Scope] = &[Scope::Proton, Scope::Wine];
    assert!(scope_applies(pw, "proton"));
    assert!(scope_applies(pw, "wine"));
    assert!(!scope_applies(pw, "native"));
    let n: &[Scope] = &[Scope::Native];
    assert!(scope_applies(n, "native"));
    assert!(!scope_applies(n, "wine"));
}

#[test]
fn env_pairs_freeform_and_listed() {
    let k = find_knob("dxvk-hud").unwrap();
    assert_eq!(
        k.env_pairs("full").unwrap(),
        vec![("DXVK_HUD".to_string(), "full".to_string())]
    );
    assert!(k.env_pairs("nope").is_none());
    let multi = find_knob("nvidia-prime-offload").unwrap();
    let pairs = multi.env_pairs("offload").unwrap();
    assert!(pairs.len() >= 3);
    let free = find_knob("dxvk-config").unwrap();
    assert_eq!(
        free.env_pairs("dxgi.hideAmdGpu = True").unwrap(),
        vec![(
            "DXVK_CONFIG".to_string(),
            "dxgi.hideAmdGpu = True".to_string()
        )]
    );
    assert_eq!(free.value_help("anything"), Some(free.help));
}

#[test]
fn wine_scopes_cover_proton() {
    for k in KNOBS {
        if k.id.starts_with("wine-") {
            assert!(
                k.scopes.contains(&Scope::Proton) && k.scopes.contains(&Scope::Wine),
                "{} must cover proton and wine",
                k.id
            );
        }
        if k.id.starts_with("proton-") && k.id != "steamdeck-spoof" {
            assert_eq!(k.scopes, &[Scope::Proton], "{}", k.id);
        }
    }
}

#[test]
fn resolve_value_rules() {
    // freeform: value required
    let free = find_knob("dxvk-config").unwrap();
    assert!(matches!(
        resolve_value(free, None).unwrap_err(),
        Error::InvalidKnobValue(_)
    ));
    assert_eq!(
        resolve_value(free, Some("dxgi.hideAmdGpu = True")).unwrap(),
        "dxgi.hideAmdGpu = True"
    );
    // single listed value: omitted ok
    let on = find_knob("proton-wined3d").unwrap();
    assert_eq!(resolve_value(on, None).unwrap(), "1");
    // multi listed: value required and must be listed
    let multi = find_knob("wine-sync").unwrap();
    assert!(resolve_value(multi, None).is_err());
    assert_eq!(resolve_value(multi, Some("fsync")).unwrap(), "fsync");
    assert!(resolve_value(multi, Some("ntsync2")).is_err());
    // listed knob with "0" value keeps explicit zero (not freeform)
    let hud = find_knob("dxvk-hud").unwrap();
    assert_eq!(resolve_value(hud, Some("0")).unwrap(), "0");
    // empty value is never accepted
    let log = find_knob("proton-log").unwrap();
    assert!(resolve_value(log, Some("")).is_err());
}

#[tokio::test]
async fn knob_rows_roundtrip() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e17-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();

    assert!(knob_values(&pool, "manual:standalone:abcd1234")
        .await
        .unwrap()
        .is_empty());
    set_knob(&pool, "manual:standalone:abcd1234", "mangohud", "1")
        .await
        .unwrap();
    set_knob(
        &pool,
        "manual:standalone:abcd1234",
        "dxvk-config",
        "dxgi.hideAmdGpu = True",
    )
    .await
    .unwrap();
    set_knob(&pool, "manual:standalone:abcd1234", "mangohud", "0")
        .await
        .unwrap();
    let rows = knob_values(&pool, "manual:standalone:abcd1234")
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "dxvk-config".to_string(),
                "dxgi.hideAmdGpu = True".to_string()
            ),
            ("mangohud".to_string(), "0".to_string()),
        ]
    );

    unset_knob(&pool, "manual:standalone:abcd1234", "mangohud")
        .await
        .unwrap();
    assert!(matches!(
        unset_knob(&pool, "manual:standalone:abcd1234", "mangohud")
            .await
            .unwrap_err(),
        Error::KnobNotSet(_)
    ));

    // rows are per game
    set_knob(&pool, "manual:standalone:zzzz9999", "mangohud", "1")
        .await
        .unwrap();
    assert_eq!(
        knob_values(&pool, "manual:standalone:zzzz9999")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        knob_values(&pool, "manual:standalone:abcd1234")
            .await
            .unwrap()
            .len(),
        1
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn custom_rows_roundtrip() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e17c-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();

    set_custom(&pool, "manual:standalone:abcd1234", "TZ", "UTC")
        .await
        .unwrap();
    set_custom(&pool, "manual:standalone:abcd1234", "TZ", "Europe/Berlin")
        .await
        .unwrap();
    set_custom(&pool, "manual:standalone:abcd1234", "EMPTY", "")
        .await
        .unwrap();
    let rows = custom_env(&pool, "manual:standalone:abcd1234")
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec![
            ("EMPTY".to_string(), "".to_string()),
            ("TZ".to_string(), "Europe/Berlin".to_string()),
        ]
    );

    remove_custom(&pool, "manual:standalone:abcd1234", "TZ")
        .await
        .unwrap();
    assert!(matches!(
        remove_custom(&pool, "manual:standalone:abcd1234", "TZ")
            .await
            .unwrap_err(),
        Error::CustomEnvNotSet(_)
    ));
    assert!(validate_custom_key("BAD-KEY").is_err());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
