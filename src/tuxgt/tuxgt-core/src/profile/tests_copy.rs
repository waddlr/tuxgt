use super::tests_support::{fix, write_live_manifest};
use super::{create_copy, create_empty, ensure_profiles, DEFAULT_ID, UNMODDED_ID};
use crate::set_knob;

#[tokio::test]
async fn empty_create_does_not_clone_the_active_profile() {
    let fx = fix("fresh").await;
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    write_live_manifest(&fx, "shade", crate::ADAPTER_PRELOAD, true);
    let made = create_empty(&fx.pool, &fx.dir, &fx.id, "Fresh")
        .await
        .unwrap();
    let slot = fx.game_dir.join("profiles").join(&made.id);
    assert!(!slot.join("manifests").exists());
    let knobs = std::fs::read_to_string(slot.join("knobs.toml")).unwrap();
    assert!(!knobs.contains("mangohud"), "{knobs}");
    assert!(crate::manifest_path(&fx.dir, &fx.id, "shade").is_file());
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, DEFAULT_ID);
    fx.drop().await;
}

#[tokio::test]
async fn copy_of_active_clones_live_when_the_slot_is_missing() {
    let fx = fix("copylive").await;
    set_knob(&fx.pool, &fx.id, "mangohud", "1").await.unwrap();
    write_live_manifest(&fx, "shade", crate::ADAPTER_PRELOAD, true);
    ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert!(!fx.game_dir.join("profiles/default").exists());
    let made = create_copy(&fx.pool, &fx.dir, &fx.id, "Clone", "Default")
        .await
        .unwrap();
    let slot = fx.game_dir.join("profiles").join(&made.id);
    assert!(slot.join("manifests").join("shade.toml").is_file() || dir_has_manifest(&slot));
    let knobs = std::fs::read_to_string(slot.join("knobs.toml")).unwrap();
    assert!(knobs.contains("mangohud"), "{knobs}");
    assert!(crate::manifest_path(&fx.dir, &fx.id, "shade").is_file());
    assert!(!fx.game_dir.join("profiles/default").exists());
    let toml = std::fs::read_to_string(slot.join("profile.toml")).unwrap();
    assert!(toml.contains(&made.id), "{toml}");
    assert!(!toml.contains("parent"), "{toml}");
    fx.drop().await;
}

#[tokio::test]
async fn copy_of_inactive_clones_its_slot_not_the_live_tree() {
    let fx = fix("copyoff").await;
    let set = ensure_profiles(&fx.pool, &fx.dir, &fx.id).await.unwrap();
    assert_eq!(set.active, UNMODDED_ID);
    let parked = fx.game_dir.join("profiles/default/manifests");
    std::fs::create_dir_all(&parked).unwrap();
    std::fs::write(parked.join("only-slot.toml"), b"instance = \"only-slot\"\n").unwrap();
    write_live_manifest(&fx, "live-only", crate::ADAPTER_PRELOAD, true);
    let made = create_copy(&fx.pool, &fx.dir, &fx.id, "FromDefault", "Default")
        .await
        .unwrap();
    let slot = fx.game_dir.join("profiles").join(&made.id);
    assert!(slot.join("manifests/only-slot.toml").is_file());
    assert!(!slot.join("manifests/live-only.toml").exists());
    assert!(crate::manifest_path(&fx.dir, &fx.id, "live-only").is_file());
    fx.drop().await;
}

fn dir_has_manifest(slot: &std::path::Path) -> bool {
    let dir = slot.join("manifests");
    dir.is_dir()
        && std::fs::read_dir(&dir).unwrap().any(|ent| {
            ent.unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "toml")
        })
}
