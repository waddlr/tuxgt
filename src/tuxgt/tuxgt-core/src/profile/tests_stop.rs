use super::tests_support::{fix, fix_steam, Fix};
use super::{
    activate_default_for_write, create_copy, create_empty, delete_profile, ensure_profiles,
    preview_switch, switch_profile, DEFAULT_ID, UNMODDED_ID,
};
use crate::client::StoreClient;
use crate::{set_custom, Error};

async fn armed_apply(fx: &Fix) {
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
}

fn assert_client_stop(err: Error) {
    match err {
        Error::NeedConfirm(msg) => assert!(msg.starts_with("profile-client-stop: "), "{msg}"),
        other => panic!("{other}"),
    }
}

#[tokio::test]
async fn delete_active_profile_asks_for_client_stop_before_any_write() {
    let fx = fix_steam("814387").await;
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    armed_apply(&fx).await;
    let beta = create_copy(&fx.pool, &fx.dir, &fx.id, "Beta", DEFAULT_ID)
        .await
        .unwrap();
    switch_profile(&fx.pool, &fx.dir, &fx.id, &beta.id, true, true, true)
        .await
        .unwrap();
    // Empty the parked Default: deleting Beta now lands on a need-free
    // profile, so the delete's switch would restore launch options.
    std::fs::write(fx.game_dir.join("profiles/default/knobs.toml"), "").unwrap();
    drop(_stopped);
    let _running = StoreClient::set_running_for_test(StoreClient::Steam, true);
    let err = match delete_profile(&fx.pool, &fx.dir, &fx.id, "Beta", true, true, false).await {
        Ok(_) => panic!("delete must ask for the client stop first"),
        Err(e) => e,
    };
    assert_client_stop(err);
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, beta.id);
    assert!(set.by_id(&beta.id).is_some());
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_some());
    fx.drop().await;
}

#[tokio::test]
async fn activate_default_asks_for_client_stop_before_any_write() {
    let fx = fix_steam("814388").await;
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    armed_apply(&fx).await;
    switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true, true)
        .await
        .unwrap();
    assert!(fx.game_dir.join("launch-memory.toml").is_file());
    drop(_stopped);
    let _running = StoreClient::set_running_for_test(StoreClient::Steam, true);
    let err = activate_default_for_write(&fx.pool, &fx.dir, &fx.id, false)
        .await
        .unwrap_err();
    assert_client_stop(err);
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_none());
    assert!(
        std::fs::read_to_string(fx.game_dir.join("launch-memory.toml"))
            .unwrap()
            .contains("apply")
    );
    fx.drop().await;
}

#[tokio::test]
async fn hook_memory_and_a_live_arm_do_not_predict_a_store_write() {
    let fx = fix("hookpred").await;
    set_custom(&fx.pool, &fx.id, "PROFILE_MARK", "armed")
        .await
        .unwrap();
    let host =
        crate::PluginHost::load_with(crate::plugin::FIRST_PARTY, fx.dir.join("config")).unwrap();
    crate::set_handle(&fx.pool, &fx.dir, &host, &fx.id, true)
        .await
        .unwrap();
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true, true)
        .await
        .unwrap();
    let preview = preview_switch(&fx.pool, &fx.dir, &fx.id, DEFAULT_ID)
        .await
        .unwrap();
    assert!(!preview.drops_needs);
    assert!(!preview.writes_store);
    fx.drop().await;

    let fx = fix_steam("814390").await;
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    armed_apply(&fx).await;
    std::fs::write(fx.game_dir.join("launch-memory.toml"), "mode = \"apply\"\n").unwrap();
    let beta = create_copy(&fx.pool, &fx.dir, &fx.id, "Beta", DEFAULT_ID)
        .await
        .unwrap();
    let preview = preview_switch(&fx.pool, &fx.dir, &fx.id, &beta.id)
        .await
        .unwrap();
    assert!(!preview.drops_needs);
    assert!(!preview.writes_store);
    fx.drop().await;
}

#[tokio::test]
async fn non_catalog_arg_in_target_still_drops_needs_and_defers_truthfully() {
    let fx = fix_steam("814389").await;
    let _stopped = StoreClient::set_running_for_test(StoreClient::Steam, false);
    armed_apply(&fx).await;
    let beta = create_empty(&fx.pool, &fx.dir, &fx.id, "Beta")
        .await
        .unwrap();
    // An enabled stored arg the game catalog does not carry never
    // launches, so the target needs no channel despite the raw row.
    std::fs::write(
        fx.game_dir
            .join("profiles")
            .join(&beta.id)
            .join("knobs.toml"),
        "[[args]]\narg = \"-not-a-catalog-arg\"\nvalue = \"\"\nenabled = true\n",
    )
    .unwrap();
    let preview = preview_switch(&fx.pool, &fx.dir, &fx.id, &beta.id)
        .await
        .unwrap();
    assert!(preview.drops_needs);
    assert!(preview.writes_store);
    drop(_stopped);
    let _running = StoreClient::set_running_for_test(StoreClient::Steam, true);
    let err = switch_profile(&fx.pool, &fx.dir, &fx.id, &beta.id, true, true, false)
        .await
        .unwrap_err();
    assert_client_stop(err);
    let done = switch_profile(&fx.pool, &fx.dir, &fx.id, &beta.id, true, true, true)
        .await
        .unwrap();
    assert_eq!(done.active, beta.id);
    assert!(done.restore_deferred);
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_some());
    fx.drop().await;
}
