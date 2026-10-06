use super::wow64::{heroic_env_on, options_carry_wow64};
use super::*;
use crate::provider::heroic::launch_snap;
use crate::testing::{seed_game, SeedGame};
use crate::{disable_knob, open_db, set_custom, set_global_knob, set_knob, Strings};

fn all_on() -> Wow64Sources {
    Wow64Sources {
        heroic_toggle: true,
        heroic_env: true,
        steam_options: true,
        knob: true,
        custom_env: true,
        global_env: true,
        process_env: true,
    }
}

#[test]
fn bitness_gates_the_warning() {
    let on = all_on();
    let off = Wow64Sources::default();
    assert_eq!(
        should_warn_wow64(Some("32"), &on),
        Some(Wow64Source::HeroicToggle)
    );
    assert_eq!(should_warn_wow64(Some("32"), &off), None);
    assert_eq!(should_warn_wow64(Some("64"), &on), None);
    assert_eq!(should_warn_wow64(None, &on), None);
    assert_eq!(should_warn_wow64(Some(""), &on), None);
}

#[test]
fn priority_order_names_first_match() {
    let mut s = all_on();
    for want in [
        Wow64Source::HeroicToggle,
        Wow64Source::HeroicEnv,
        Wow64Source::SteamOptions,
        Wow64Source::Knob,
        Wow64Source::CustomEnv,
        Wow64Source::GlobalEnv,
        Wow64Source::ProcessEnv,
    ] {
        assert_eq!(should_warn_wow64(Some("32"), &s), Some(want));
        match want {
            Wow64Source::HeroicToggle => s.heroic_toggle = false,
            Wow64Source::HeroicEnv => s.heroic_env = false,
            Wow64Source::SteamOptions => s.steam_options = false,
            Wow64Source::Knob => s.knob = false,
            Wow64Source::CustomEnv => s.custom_env = false,
            Wow64Source::GlobalEnv => s.global_env = false,
            Wow64Source::ProcessEnv => s.process_env = false,
        }
    }
    assert_eq!(should_warn_wow64(Some("32"), &s), None);
}

#[test]
fn steam_options_need_the_exact_token() {
    assert!(options_carry_wow64("PROTON_USE_WOW64=1 %command%"));
    assert!(options_carry_wow64(
        "PROTON_USE_WOW64=1 gamemoderun %command%"
    ));
    assert!(!options_carry_wow64("gamemoderun %command%"));
    assert!(!options_carry_wow64("PROTON_USE_WOW64=0 %command%"));
    assert!(!options_carry_wow64("PROTON_USE_WOW64=%command%"));
    assert!(!options_carry_wow64(""));
}

#[test]
fn heroic_env_needs_value_one() {
    assert!(heroic_env_on(Some(r#"{"PROTON_USE_WOW64":"1"}"#)));
    assert!(heroic_env_on(Some(
        r#"{"OTHER":"x","PROTON_USE_WOW64":" 1 "}"#
    )));
    assert!(!heroic_env_on(None));
    assert!(!heroic_env_on(Some("")));
    assert!(!heroic_env_on(Some(r#"{"PROTON_USE_WOW64":"0"}"#)));
    assert!(!heroic_env_on(Some(r#"{"OTHER":"x"}"#)));
    assert!(!heroic_env_on(Some("not json")));
}

#[test]
fn heroic_snap_reads_enable_wow64() {
    let root = std::env::temp_dir().join(format!("tuxgt-wow64-heroic-{}", std::process::id()));
    let cfg = root.join("GamesConfig");
    std::fs::create_dir_all(&cfg).unwrap();
    let write = |app: &str, body: &str| {
        std::fs::write(cfg.join(format!("{app}.json")), body).unwrap();
    };
    write("on", r#"{"on":{"enableWoW64":true}}"#);
    write("off", r#"{"off":{"enableWoW64":false}}"#);
    write("absent", r#"{"absent":{"winePrefix":"/pfx"}}"#);
    write("broken", r#"{"broken":{"enableWoW64":"#);

    assert!(launch_snap(&root, "on", None).unwrap().enable_wow64);
    assert!(!launch_snap(&root, "off", None).unwrap().enable_wow64);
    assert!(!launch_snap(&root, "absent", None).unwrap().enable_wow64);
    assert!(launch_snap(&root, "broken", None).is_none());
    assert!(launch_snap(&root, "missing", None).is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn heroic_snap_carries_env_options() {
    let root = std::env::temp_dir().join(format!("tuxgt-wow64-env-{}", std::process::id()));
    let cfg = root.join("GamesConfig");
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(
        cfg.join("app.json"),
        r#"{"app":{"enviromentOptions":[{"key":"PROTON_USE_WOW64","value":"1"}]}}"#,
    )
    .unwrap();
    let snap = launch_snap(&root, "app", None).unwrap();
    assert!(!snap.enable_wow64);
    assert!(heroic_env_on(snap.env.as_deref()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_source_id_resolves() {
    let s = Strings::en_us().expect("catalog");
    for src in [
        Wow64Source::HeroicToggle,
        Wow64Source::HeroicEnv,
        Wow64Source::SteamOptions,
        Wow64Source::Knob,
        Wow64Source::CustomEnv,
        Wow64Source::GlobalEnv,
        Wow64Source::ProcessEnv,
    ] {
        assert_ne!(s.get(src.fluent_id()), src.fluent_id());
    }
}

async fn tmp_pool(name: &str) -> (sqlx::SqlitePool, String) {
    let dir = std::env::temp_dir().join(format!(
        "tuxgt-wow64-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let pool = open_db(&dir).await.expect("open db");
    (pool, format!("manual:standalone:{name}"))
}

fn ambient_clean() {
    assert!(
        !std::env::var(WOW64_VAR).is_ok_and(|v| v.trim() == "1"),
        "test env sets {WOW64_VAR}=1; unset it to run wow64 tests",
    );
}

async fn seed_32(pool: &sqlx::SqlitePool, id: &str) {
    seed_game(
        pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: id.rsplit(':').next().unwrap_or(id),
            detected_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn gather_knob_custom_global() {
    ambient_clean();
    let (pool, id) = tmp_pool("gather").await;
    seed_32(&pool, &id).await;
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);

    set_knob(&pool, &id, "proton-wow64", "1").await.unwrap();
    assert_eq!(
        wow64_warn(&pool, &id, None).await.unwrap(),
        Some(Wow64Source::Knob)
    );
    disable_knob(&pool, &id, "proton-wow64").await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);

    set_custom(&pool, &id, WOW64_VAR, "1").await.unwrap();
    assert_eq!(
        wow64_warn(&pool, &id, None).await.unwrap(),
        Some(Wow64Source::CustomEnv)
    );
    crate::remove_custom(&pool, &id, WOW64_VAR).await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);

    set_global_knob(&pool, "proton-wow64", "1").await.unwrap();
    assert_eq!(
        wow64_warn(&pool, &id, None).await.unwrap(),
        Some(Wow64Source::GlobalEnv)
    );
}

#[tokio::test]
async fn gather_ignores_non_one_knob_values() {
    ambient_clean();
    let (pool, id) = tmp_pool("knobval").await;
    seed_32(&pool, &id).await;
    set_knob(&pool, &id, "proton-wow64", "").await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);
    set_knob(&pool, &id, "proton-wow64", "2").await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);
}

#[tokio::test]
async fn gather_64bit_and_unknown_stay_silent() {
    ambient_clean();
    let (pool, id) = tmp_pool("silent").await;
    seed_game(
        &pool,
        SeedGame {
            id: &id,
            manager: "manual",
            store: "standalone",
            game_id: "silent",
            detected_bitness: Some("64"),
            ..Default::default()
        },
    )
    .await;
    set_knob(&pool, &id, "proton-wow64", "1").await.unwrap();
    set_custom(&pool, &id, WOW64_VAR, "1").await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);

    assert_eq!(
        wow64_warn(&pool, "manual:standalone:nope", None)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn gather_override_bitness_wins() {
    ambient_clean();
    let (pool, id) = tmp_pool("override").await;
    seed_game(
        &pool,
        SeedGame {
            id: &id,
            manager: "manual",
            store: "standalone",
            game_id: "override",
            detected_bitness: Some("64"),
            override_bitness: Some("32"),
            ..Default::default()
        },
    )
    .await;
    set_custom(&pool, &id, WOW64_VAR, "1").await.unwrap();
    assert_eq!(
        wow64_warn(&pool, &id, None).await.unwrap(),
        Some(Wow64Source::CustomEnv)
    );
}

#[tokio::test]
async fn gather_manager_gating_falls_through_to_knob() {
    ambient_clean();
    let tag = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    let (pool, _) = tmp_pool("gating").await;
    // Fake app ids match no live store config, so the manager-gated file
    // reads miss and the knob channel decides.
    for id in [
        format!("heroic:gog:wow64fake{tag}"),
        format!("steam::wow64fake{tag}"),
    ] {
        let gid = GameId::parse(&id).unwrap();
        seed_game(
            &pool,
            SeedGame {
                id: id.as_str(),
                manager: gid.manager.as_str(),
                store: gid.store.as_str(),
                game_id: gid.game.as_str(),
                detected_bitness: Some("32"),
                ..Default::default()
            },
        )
        .await;
        set_knob(&pool, &id, "proton-wow64", "1").await.unwrap();
        assert_eq!(
            wow64_warn(&pool, &id, None).await.unwrap(),
            Some(Wow64Source::Knob),
            "{id}",
        );
    }
}

#[tokio::test]
async fn gather_empty_override_falls_back_to_detected() {
    ambient_clean();
    let (pool, id) = tmp_pool("fallback").await;
    seed_game(
        &pool,
        SeedGame {
            id: &id,
            manager: "manual",
            store: "standalone",
            game_id: "fallback",
            detected_bitness: Some("64"),
            override_bitness: Some(""),
            ..Default::default()
        },
    )
    .await;
    set_custom(&pool, &id, WOW64_VAR, "1").await.unwrap();
    assert_eq!(wow64_warn(&pool, &id, None).await.unwrap(), None);
}
