use super::tests_support::{fix, fix_steam};
use super::{
    activate_default_for_write, ensure_profile_write, ensure_profiles, preview_switch,
    switch_profile, ADD_TO_DEFAULT, DEFAULT_ID, UNMODDED_ID,
};
use crate::client::StoreClient;
use crate::{set_custom, set_knob, Error};

#[tokio::test]
async fn unmodded_write_lands_on_empty_default() {
    let fx = fix("gate-empty").await;
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    ensure_profile_write(&fx.pool, &fx.dir, &fx.id)
        .await
        .unwrap();
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    let rows = crate::knob_rows(&fx.pool, &fx.id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, "1");
    fx.drop().await;
}

#[tokio::test]
async fn refused_need_profile_writes_nothing() {
    let fx = fix("gate-full").await;
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    let knobs = fx.game_dir.join("profiles/default/knobs.toml");
    std::fs::write(
        &knobs,
        "[[knobs]]\nknob = \"mangohud\"\nvalue = \"1\"\nenabled = true\n",
    )
    .unwrap();
    let err = ensure_profile_write(&fx.pool, &fx.dir, &fx.id)
        .await
        .unwrap_err();
    match err {
        Error::NeedProfile(msg) => assert_eq!(msg, ADD_TO_DEFAULT),
        other => panic!("{other}"),
    }
    assert!(crate::knob_rows(&fx.pool, &fx.id).await.unwrap().is_empty());
    assert!(std::fs::read_to_string(&knobs)
        .unwrap()
        .contains("mangohud"));
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    activate_default_for_write(&fx.pool, &fx.dir, &fx.id, false)
        .await
        .unwrap();
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert_eq!(crate::knob_rows(&fx.pool, &fx.id).await.unwrap().len(), 1);
    fx.drop().await;
}

#[tokio::test]
async fn apply_switch_asks_for_client_stop_before_any_write() {
    let fx = fix_steam("814385").await;
    let _running = StoreClient::set_running_for_test(StoreClient::Steam, true);
    set_custom(&fx.pool, &fx.id, "PROFILE_MARK", "armed")
        .await
        .unwrap();
    crate::apply::write_record(
        &fx.dir,
        &crate::ApplyRecord {
            game: fx.id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    let preview = preview_switch(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID)
        .await
        .unwrap();
    assert!(preview.drops_needs);
    assert!(preview.writes_store);
    let err = switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true, false)
        .await
        .unwrap_err();
    match err {
        Error::NeedConfirm(msg) => assert!(msg.starts_with("profile-client-stop: "), "{msg}"),
        other => panic!("{other}"),
    }
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    assert!(std::fs::read_to_string(fx.game_dir.join("profiles.toml"))
        .unwrap()
        .contains("active = \"default\""));
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_some());
    assert!(!fx.game_dir.join("launch-memory.toml").exists());
    fx.drop().await;
}

#[tokio::test]
async fn apply_rearm_asks_for_client_stop_before_any_write() {
    let fx = fix_steam("814386").await;
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    set_custom(&fx.pool, &fx.id, "PROFILE_MARK", "armed")
        .await
        .unwrap();
    crate::apply::write_record(
        &fx.dir,
        &crate::ApplyRecord {
            game: fx.id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true, false)
        .await
        .unwrap();
    assert!(fx.game_dir.join("launch-memory.toml").is_file());
    drop(_stopped);
    let _running = StoreClient::set_running_for_test(StoreClient::Steam, true);
    let preview = preview_switch(&fx.pool, &fx.dir, &fx.id, DEFAULT_ID)
        .await
        .unwrap();
    assert!(preview.writes_store);
    let err = switch_profile(&fx.pool, &fx.dir, &fx.id, DEFAULT_ID, true, true, false)
        .await
        .unwrap_err();
    match err {
        Error::NeedConfirm(msg) => assert!(msg.starts_with("profile-client-stop: "), "{msg}"),
        other => panic!("{other}"),
    }
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    assert!(std::fs::read_to_string(fx.game_dir.join("profiles.toml"))
        .unwrap()
        .contains("active = \"unmodded\""));
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_none());
    assert!(
        std::fs::read_to_string(fx.game_dir.join("launch-memory.toml"))
            .unwrap()
            .contains("apply")
    );
    fx.drop().await;
}
