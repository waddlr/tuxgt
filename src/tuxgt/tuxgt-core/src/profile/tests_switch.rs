use super::tests_support::{
    fix, fix_steam, plant_install, plant_slot_install, write_live_manifest,
};
use super::{create_empty, ensure_profiles, switch_profile, DEFAULT_ID, UNMODDED_ID};
use crate::client::StoreClient;
use crate::{set_custom, Error};

#[tokio::test]
async fn edits_stay_isolated_and_conf_follows_the_active_profile() {
    let fx = fix("iso").await;
    set_custom(&fx.pool, &fx.id, "PROFILE_MARK", "alpha")
        .await
        .unwrap();
    std::fs::create_dir_all(fx.game_dir.join("runtime")).unwrap();
    std::fs::write(fx.game_dir.join("runtime/from-active.txt"), b"alpha").unwrap();
    std::fs::create_dir_all(fx.game_dir.join("disabled/shade")).unwrap();
    std::fs::write(fx.game_dir.join("disabled/shade/note.txt"), b"alpha").unwrap();
    write_live_manifest(&fx, "shade", crate::ADAPTER_PRELOAD, true);
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    let beta = create_empty(&fx.pool, &fx.dir, &fx.id, "Beta")
        .await
        .unwrap();
    let slot = fx.game_dir.join("profiles").join(&beta.id);
    std::fs::create_dir_all(slot.join("runtime")).unwrap();
    std::fs::write(slot.join("runtime/from-beta.txt"), b"beta").unwrap();
    std::fs::write(
        slot.join("knobs.toml"),
        "[[custom]]\nkey = \"PROFILE_MARK\"\nvalue = \"beta\"\n",
    )
    .unwrap();
    switch_profile(&fx.pool, &fx.dir, &fx.id, &beta.id, true, true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(fx.game_dir.join("runtime/from-beta.txt")).unwrap(),
        b"beta"
    );
    assert!(!fx.game_dir.join("runtime/from-active.txt").exists());
    assert!(!fx.game_dir.join("disabled").exists());
    let conf = std::fs::read_to_string(fx.game_dir.join("tux-protonfixes.conf")).unwrap();
    assert!(conf.contains("PROFILE_MARK=beta"), "{conf}");
    assert!(!conf.contains("PROFILE_MARK=alpha"), "{conf}");
    assert!(conf.contains("TUXGT_GAME_DIR="), "{conf}");
    let parked = fx.game_dir.join("profiles/default");
    assert!(parked.join("manifests").join("shade.toml").is_file() || manifest_in(&parked));
    assert_eq!(
        std::fs::read(parked.join("runtime/from-active.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(
        std::fs::read(parked.join("disabled/shade/note.txt")).unwrap(),
        b"alpha"
    );
    switch_profile(&fx.pool, &fx.dir, &fx.id, DEFAULT_ID, true, true)
        .await
        .unwrap();
    let conf = std::fs::read_to_string(fx.game_dir.join("tux-protonfixes.conf")).unwrap();
    assert!(conf.contains("PROFILE_MARK=alpha"), "{conf}");
    assert_eq!(
        std::fs::read(fx.game_dir.join("runtime/from-active.txt")).unwrap(),
        b"alpha"
    );
    assert!(slot.join("runtime/from-beta.txt").is_file());
    fx.drop().await;
}

#[tokio::test]
async fn install_switch_confirms_before_any_write_and_does_not_stop_the_store() {
    let fx = fix("files").await;
    plant_install(&fx, "old", "payload.bin", b"old-bytes");
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    let alt = create_empty(&fx.pool, &fx.dir, &fx.id, "Alt")
        .await
        .unwrap();
    plant_slot_install(&fx, &alt.id, "alt", "other.bin", b"new-bytes");
    let err = switch_profile(&fx.pool, &fx.dir, &fx.id, &alt.id, false, false)
        .await
        .unwrap_err();
    match err {
        Error::NeedConfirm(msg) => {
            assert!(msg.starts_with("profile-files:"), "{msg}");
            assert!(msg.contains("payload.bin"), "{msg}");
            assert!(msg.contains("other.bin"), "{msg}");
        }
        other => panic!("{other}"),
    }
    assert_eq!(
        std::fs::read(fx.install.join("payload.bin")).unwrap(),
        b"old-bytes"
    );
    assert!(!fx.install.join("other.bin").exists());
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    switch_profile(&fx.pool, &fx.dir, &fx.id, &alt.id, true, true)
        .await
        .unwrap();
    assert!(!fx.install.join("payload.bin").exists());
    assert_eq!(
        std::fs::read(fx.install.join("other.bin")).unwrap(),
        b"new-bytes"
    );
    assert!(fx.game_dir.join("profiles/default/manifests").exists());
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, alt.id);
    fx.drop().await;
}

#[tokio::test]
async fn unmodded_apply_restore_confirms_and_defers_while_the_client_runs() {
    let fx = fix_steam("814381").await;
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
    let err = switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, false)
        .await
        .unwrap_err();
    match err {
        Error::NeedConfirm(msg) => assert!(msg.starts_with("profile-restore:"), "{msg}"),
        other => panic!("{other}"),
    }
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_some());
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    assert!(!crate::custom_env(&fx.pool, &fx.id)
        .await
        .unwrap()
        .is_empty());
    let done = switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true)
        .await
        .unwrap();
    assert_eq!(done.active, UNMODDED_ID);
    assert!(!done.restore_deferred);
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_none());
    fx.drop().await;
}

#[tokio::test]
async fn apply_restore_defers_when_the_store_client_is_running() {
    let fx = fix_steam("814382").await;
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
    let done = switch_profile(&fx.pool, &fx.dir, &fx.id, UNMODDED_ID, true, true)
        .await
        .unwrap();
    assert_eq!(done.active, UNMODDED_ID);
    assert!(done.restore_deferred);
    assert!(crate::read_record(&fx.dir, &fx.id).unwrap().is_some());
    fx.drop().await;
}

#[tokio::test]
async fn resume_finishes_a_parking_journal() {
    let fx = fix("resume").await;
    write_live_manifest(&fx, "shade", crate::ADAPTER_PRELOAD, true);
    std::fs::create_dir_all(fx.game_dir.join("runtime")).unwrap();
    std::fs::write(fx.game_dir.join("runtime/live.txt"), b"live").unwrap();
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    std::fs::create_dir_all(fx.game_dir.join("profiles/default/manifests")).unwrap();
    std::fs::write(
        fx.game_dir.join("profiles/default/manifests/junk.toml"),
        b"junk",
    )
    .unwrap();
    std::fs::write(
        fx.game_dir.join("profile-switch.toml"),
        "phase = \"parking\"\nfrom = \"default\"\nto = \"unmodded\"\nyes_files = true\nyes_restore = true\ndrops_needs = false\n",
    )
    .unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    assert!(!fx.game_dir.join("profile-switch.toml").exists());
    assert!(!crate::manifest_path(&fx.dir, &fx.id, "shade").exists());
    assert!(fx
        .game_dir
        .join("profiles/default/runtime/live.txt")
        .is_file());
    assert!(!fx
        .game_dir
        .join("profiles/default/manifests/junk.toml")
        .exists());
    fx.drop().await;
}

fn manifest_in(slot: &std::path::Path) -> bool {
    let dir = slot.join("manifests");
    dir.is_dir()
        && std::fs::read_dir(dir).unwrap().any(|ent| {
            ent.unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "toml")
        })
}
