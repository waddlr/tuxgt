use super::slot::KnobFile;
use super::tests_support::{fix, write_live_manifest};
use super::{create_empty, ensure_profiles, switch_profile, DEFAULT_ID, UNMODDED_ID};
use crate::game_args::store::{self, Snapshot, StoredArg};
use crate::set_knob;

#[tokio::test]
async fn empty_game_starts_unmodded() {
    let fx = fix("empty").await;
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    assert!(fx.game_dir.join("profiles/default/profile.toml").is_file());
    assert!(fx.game_dir.join("profiles/default/knobs.toml").is_file());
    assert!(!fx.game_dir.join("profiles/unmodded").exists());
    let again = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(again.active, UNMODDED_ID);
    let marker = fx.game_dir.join("runtime/stay.txt");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"stay").unwrap();
    let third = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(third.active, UNMODDED_ID);
    assert_eq!(std::fs::read(&marker).unwrap(), b"stay");
    fx.drop().await;
}

#[tokio::test]
async fn disabled_manifest_starts_on_default() {
    let fx = fix("disabled").await;
    write_live_manifest(&fx, "shade", crate::ADAPTER_PRELOAD, false);
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert!(!fx.game_dir.join("profiles/default").exists());
    let live = crate::manifest_path(&fx.dir, &fx.id, "shade");
    let before = std::fs::read(&live).unwrap();
    let again = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(again.active, DEFAULT_ID);
    assert_eq!(std::fs::read(&live).unwrap(), before);
    assert!(!fx.game_dir.join("profiles/default").exists());
    fx.drop().await;
}

#[tokio::test]
async fn knob_starts_on_default() {
    let fx = fix("knob").await;
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert!(!fx.game_dir.join("profiles/default").exists());
    let rows = crate::knob_rows(&fx.pool, &fx.id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].knob, "mangohud");
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(crate::knob_rows(&fx.pool, &fx.id).await.unwrap().len(), 1);
    assert!(!fx.game_dir.join("profiles/default").exists());
    fx.drop().await;
}

#[tokio::test]
async fn arg_row_migrates_to_default_and_parks_on_switch() {
    let fx = fix("args").await;
    store::upsert_arg(&fx.pool, &fx.id, "-windowed", "", false)
        .await
        .unwrap();
    store::upsert_published(
        &fx.pool,
        &fx.id,
        "-windowed",
        &Snapshot {
            args: vec![],
            custom: String::new(),
        },
    )
    .await
    .unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert!(!fx.game_dir.join("profiles/default").exists());
    let other = create_empty(&fx.pool, &fx.dir, &fx.id, "Other")
        .await
        .unwrap();
    switch_profile(&fx.pool, &fx.dir, &fx.id, &other.id, true, true, true)
        .await
        .unwrap();
    assert!(store::stored_args(&fx.pool, &fx.id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store::published(&fx.pool, &fx.id)
            .await
            .unwrap()
            .unwrap()
            .tail,
        "-windowed"
    );
    switch_profile(&fx.pool, &fx.dir, &fx.id, DEFAULT_ID, true, true, true)
        .await
        .unwrap();
    let rows = store::stored_args(&fx.pool, &fx.id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].arg, "-windowed");
    assert!(!rows[0].enabled);
    assert_eq!(
        store::published(&fx.pool, &fx.id)
            .await
            .unwrap()
            .unwrap()
            .tail,
        "-windowed"
    );
    fx.drop().await;
}

#[tokio::test]
async fn blank_custom_args_stay_unmodded() {
    let fx = fix("blank-args").await;
    store::set_custom_text(&fx.pool, &fx.id, "   ")
        .await
        .unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    fx.drop().await;
}

#[tokio::test]
async fn custom_args_migrate_to_default() {
    let fx = fix("custom-args").await;
    store::set_custom_text(&fx.pool, &fx.id, "-novid")
        .await
        .unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert_eq!(
        store::custom_text(&fx.pool, &fx.id).await.unwrap(),
        "-novid"
    );
    fx.drop().await;
}

#[test]
fn parked_args_need_a_channel_only_when_they_would_launch() {
    let on = KnobFile {
        args: vec![StoredArg {
            arg: "-windowed".into(),
            value: String::new(),
            enabled: true,
        }],
        ..KnobFile::default()
    };
    assert!(super::files::launch_needs(&[], &on).channel_needed());
    let off = KnobFile {
        args: vec![StoredArg {
            arg: "-windowed".into(),
            value: String::new(),
            enabled: false,
        }],
        ..KnobFile::default()
    };
    assert!(!super::files::launch_needs(&[], &off).channel_needed());
    let custom = KnobFile {
        args_custom: " -novid ".into(),
        ..KnobFile::default()
    };
    assert!(super::files::launch_needs(&[], &custom).channel_needed());
    let blank = KnobFile {
        args_custom: "  ".into(),
        ..KnobFile::default()
    };
    assert!(!super::files::launch_needs(&[], &blank).channel_needed());
}
