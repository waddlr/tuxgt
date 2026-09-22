use super::*;
use crate::testing::{seed_game, SeedGame};
use crate::Error;

#[tokio::test]
async fn platform_pick() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e17p-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();

    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:aaaaaaaa",
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        effective_platform(&pool, "manual:standalone:aaaaaaaa")
            .await
            .unwrap(),
        "native"
    );

    sqlx::query(
        "UPDATE games SET detected_platform = 'proton' WHERE id = 'manual:standalone:aaaaaaaa'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        effective_platform(&pool, "manual:standalone:aaaaaaaa")
            .await
            .unwrap(),
        "proton"
    );

    // no platform but a prefix: proton
    sqlx::query(
            "UPDATE games SET detected_platform = NULL, prefix_path = '/tmp/pfx' WHERE id = 'manual:standalone:aaaaaaaa'",
        )
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        effective_platform(&pool, "manual:standalone:aaaaaaaa")
            .await
            .unwrap(),
        "proton"
    );

    // no platform, no prefix, but a proton runner: proton
    sqlx::query(
            "UPDATE games SET prefix_path = NULL, proton = 'proton_9', detected_platform = NULL WHERE id = 'manual:standalone:aaaaaaaa'",
        )
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        effective_platform(&pool, "manual:standalone:aaaaaaaa")
            .await
            .unwrap(),
        "proton"
    );

    // override wins over detected
    sqlx::query(
            "UPDATE games SET detected_platform = 'proton', override_platform = 'native' WHERE id = 'manual:standalone:aaaaaaaa'",
        )
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        effective_platform(&pool, "manual:standalone:aaaaaaaa")
            .await
            .unwrap(),
        "native"
    );

    assert!(matches!(
        effective_platform(&pool, "manual:standalone:nope0000")
            .await
            .unwrap_err(),
        Error::UnknownGame(_)
    ));
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn enabled_column_defaults_and_disable() {
    let dir = std::env::temp_dir().join(format!("tuxgt-e51en-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let id = "manual:standalone:abcd1234";
    set_knob(&pool, id, "mangohud", "1").await.unwrap();
    let rows = knob_rows(&pool, id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].enabled);
    disable_knob(&pool, id, "mangohud").await.unwrap();
    let rows = knob_rows(&pool, id).await.unwrap();
    assert_eq!(rows[0].value, "1");
    assert!(!rows[0].enabled);
    enable_knob(&pool, id, "mangohud").await.unwrap();
    assert!(knob_rows(&pool, id).await.unwrap()[0].enabled);
    unset_knob(&pool, id, "mangohud").await.unwrap();
    assert!(matches!(
        disable_knob(&pool, id, "mangohud").await.unwrap_err(),
        Error::KnobNotSet(_)
    ));
    enable_knob(&pool, id, "mangohud").await.unwrap();
    assert!(knob_rows(&pool, id).await.unwrap().is_empty());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn global_roundtrip_no_environment_d() {
    let _g = TEST_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("tuxgt-e51g-{}", std::process::id()));
    let xdg = dir.join("xdg");
    let _ = tokio::fs::remove_dir_all(&dir).await;
    std::fs::create_dir_all(&xdg).unwrap();
    let prev = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", &xdg);
    let prev_hud = std::env::var_os("DXVK_HUD");
    let prev_mango = std::env::var_os("MANGOHUD");
    std::env::remove_var("DXVK_HUD");
    std::env::remove_var("MANGOHUD");
    let pool = crate::open_db(&dir).await.unwrap();
    set_global_knob(&pool, "dxvk-hud", "full").await.unwrap();
    set_global_knob(&pool, "mangohud", "1").await.unwrap();
    let path = super::global::environment_d_path();
    assert!(!path.is_file(), "no environment.d write");
    disable_global_knob(&pool, "dxvk-hud").await.unwrap();
    assert!(!path.is_file(), "no environment.d write");
    let rows = global_knobs(&pool).await.unwrap();
    let hud = rows.iter().find(|r| r.knob == "dxvk-hud").unwrap();
    assert_eq!(hud.value, "full");
    assert!(!hud.enabled);
    unset_global_knob(&pool, "mangohud").await.unwrap();
    assert!(!path.is_file(), "no environment.d write");
    match prev_hud {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
    match prev_mango {
        Some(v) => std::env::set_var("MANGOHUD", v),
        None => std::env::remove_var("MANGOHUD"),
    }
    match prev {
        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn retire_our_environment_d_leaves_foreign() {
    let _g = TEST_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("tuxgt-e51r-{}", std::process::id()));
    let xdg = dir.join("xdg");
    let _ = tokio::fs::remove_dir_all(&dir).await;
    std::fs::create_dir_all(&xdg).unwrap();
    let prev = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", &xdg);
    let path = super::global::environment_d_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "# Written by TuxGT. Do not edit; TuxGT rewrites this file.\nMANGOHUD=1\n",
    )
    .unwrap();
    let pool = crate::open_db(&dir).await.unwrap();
    assert!(!path.exists(), "our leftover is retired on open");
    std::fs::write(&path, "MANGOHUD=1\n").unwrap();
    crate::migrate_env(&pool).await.unwrap();
    assert!(path.is_file(), "foreign file stays");
    std::fs::write(
        &path,
        "# Written by TuxGT. Do not edit; TuxGT rewrites this file.\n",
    )
    .unwrap();
    super::global::retire_environment_d().unwrap();
    assert!(!path.exists(), "our header file is deleted");
    match prev {
        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn unmanaged_global_writes_refused() {
    let _g = TEST_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("tuxgt-e51u-{}", std::process::id()));
    let xdg = dir.join("xdg");
    let _ = tokio::fs::remove_dir_all(&dir).await;
    std::fs::create_dir_all(&xdg).unwrap();
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", &xdg);
    let pool = crate::open_db(&dir).await.unwrap();
    let prev = std::env::var_os("DXVK_HUD");
    std::env::set_var("DXVK_HUD", "full");
    assert!(matches!(
        set_global_knob(&pool, "dxvk-hud", "1").await.unwrap_err(),
        Error::KnobUnmanaged(_)
    ));
    assert!(matches!(
        enable_global_knob(&pool, "dxvk-hud").await.unwrap_err(),
        Error::KnobUnmanaged(_)
    ));
    assert!(matches!(
        disable_global_knob(&pool, "dxvk-hud").await.unwrap_err(),
        Error::KnobUnmanaged(_)
    ));
    assert!(matches!(
        unset_global_knob(&pool, "dxvk-hud").await.unwrap_err(),
        Error::KnobUnmanaged(_)
    ));
    match prev {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
    match prev_xdg {
        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn unmanaged_vs_ours() {
    let _g = TEST_ENV_LOCK.lock().unwrap();
    let k = find_knob("dxvk-hud").unwrap();
    let prev = std::env::var_os("DXVK_HUD");
    std::env::set_var("DXVK_HUD", "full");
    assert!(knob_is_unmanaged(k, None));
    assert!(!knob_is_unmanaged(k, Some("full")));
    assert!(!knob_is_unmanaged(k, Some("1")));
    match prev {
        Some(v) => std::env::set_var("DXVK_HUD", v),
        None => std::env::remove_var("DXVK_HUD"),
    }
}

#[tokio::test]
async fn env_keys_game_custom_global_order() {
    let _g = TEST_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join(format!("tuxgt-t15ek-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let prev_mango = std::env::var_os("MANGOHUD");
    std::env::remove_var("MANGOHUD");
    let pool = crate::open_db(&dir).await.unwrap();
    let id = "manual:standalone:abcd1234";

    // one key colliding across all three tables: game wins, then custom,
    // then global. A disabled game row still reports Game (storage, not
    // effective policy).
    set_global_knob(&pool, "mangohud", "1").await.unwrap();
    set_custom(&pool, id, "mangohud", "custom").await.unwrap();
    set_knob(&pool, id, "mangohud", "1").await.unwrap();
    let got = env_keys(&pool, id, "mangohud").await.unwrap().unwrap();
    assert_eq!(got.scope, EnvScope::Game);
    assert_eq!(got.value, "1");
    assert!(got.enabled);
    disable_knob(&pool, id, "mangohud").await.unwrap();
    let got = env_keys(&pool, id, "mangohud").await.unwrap().unwrap();
    assert_eq!(got.scope, EnvScope::Game);
    assert!(!got.enabled);
    unset_knob(&pool, id, "mangohud").await.unwrap();
    let got = env_keys(&pool, id, "mangohud").await.unwrap().unwrap();
    assert_eq!(got.scope, EnvScope::Custom);
    assert_eq!(got.value, "custom");
    assert!(got.enabled);
    remove_custom(&pool, id, "mangohud").await.unwrap();
    let got = env_keys(&pool, id, "mangohud").await.unwrap().unwrap();
    assert_eq!(got.scope, EnvScope::Global);
    assert_eq!(got.value, "1");
    unset_global_knob(&pool, "mangohud").await.unwrap();
    assert!(env_keys(&pool, id, "mangohud").await.unwrap().is_none());

    // disjoint keys resolve to their own scope; unknown is None.
    set_custom(&pool, id, "TZ", "UTC").await.unwrap();
    let got = env_keys(&pool, id, "TZ").await.unwrap().unwrap();
    assert_eq!(got.scope, EnvScope::Custom);
    assert_eq!(got.scope.as_str(), "custom");
    assert!(env_keys(&pool, id, "nope").await.unwrap().is_none());
    assert_eq!(EnvScope::Game.as_str(), "game");
    assert_eq!(EnvScope::Global.as_str(), "global");

    match prev_mango {
        Some(v) => std::env::set_var("MANGOHUD", v),
        None => std::env::remove_var("MANGOHUD"),
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn count_set_env_matches_loaded_rows() {
    let dir = std::env::temp_dir().join(format!("tuxgt-countenv-{}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&dir).await;
    let pool = crate::open_db(&dir).await.unwrap();
    let game = "steam::1";
    // Enabled + valued counts; disabled and empty-valued do not.
    set_knob(&pool, game, "a", "1").await.unwrap();
    set_knob(&pool, game, "b", "2").await.unwrap();
    disable_knob(&pool, game, "b").await.unwrap();
    set_knob(&pool, game, "c", "").await.unwrap();
    set_custom(&pool, game, "X", "1").await.unwrap();
    set_custom(&pool, game, "Y", "2").await.unwrap();
    assert_eq!(count_set_env(&pool, game).await.unwrap(), (1, 2));
    // Matches what the GUI hero counted from loaded rows.
    let rows = knob_rows(&pool, game).await.unwrap();
    let n = rows
        .iter()
        .filter(|r| r.enabled && !r.value.is_empty())
        .count();
    assert_eq!(n, 1);
    assert_eq!(custom_env(&pool, game).await.unwrap().len(), 2);
    // Unknown game: zeros, never an error.
    assert_eq!(count_set_env(&pool, "steam::nope").await.unwrap(), (0, 0));
    pool.close().await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
