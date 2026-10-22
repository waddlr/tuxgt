use super::tests_support::fix;
use super::{
    activate_default_for_write, ensure_profile_write, ensure_profiles, ADD_TO_DEFAULT, DEFAULT_ID,
    UNMODDED_ID,
};
use crate::{set_knob, Error};

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
    activate_default_for_write(&fx.pool, &fx.dir, &fx.id)
        .await
        .unwrap();
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    assert_eq!(crate::knob_rows(&fx.pool, &fx.id).await.unwrap().len(), 1);
    fx.drop().await;
}
