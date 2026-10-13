//! Install: Links materialization over staged files.
use super::testing::*;
use super::*;
use crate::FileManifest;
use std::fs;

mod promote;

fn links_manifest(game: &str, instance: &str, dest: &str, sha: &str, load: i64) -> FileManifest {
    let mut m = test_manifest(game, instance, dest, sha);
    m.adapter = "install-links".into();
    m.load_order = load;
    m
}

fn stage_payload(data: &std::path::Path, game: &str, instance: &str, dest: &str, bytes: &[u8]) {
    let dir = crate::stage::stage_dir(data, game, instance);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(dest), bytes).unwrap();
}

fn staged(game_data: (&std::path::Path, &str), instance: &str, dest: &str) -> std::path::PathBuf {
    crate::stage::stage_dir(game_data.0, game_data.1, instance).join(dest)
}

fn claims_for(data: &std::path::Path, game: &str, manifests: &[FileManifest]) -> ClaimInventory {
    for m in manifests {
        crate::write_manifest(data, m).unwrap();
    }
    claim_inventory(data, game).unwrap()
}

#[test]
fn links_apply_creates_an_absolute_staged_symlink() {
    let game = "manual:standalone:links001";
    let (data, _stage, root) = setup(game);
    stage_payload(&data, game, "reshade", "dxgi.dll", b"v1");
    let h1 = crate::sha256_file(&staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let mut m = links_manifest(game, "reshade", "dxgi.dll", &h1, 0);
    let claims = claims_for(&data, game, &[m.clone()]);
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap();
    assert!(ops[0].material.is_links());
    apply_copies(&data, game, &mut m, &root, None, &ops, &claims).unwrap();
    let target = root.join("dxgi.dll");
    assert!(
        fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink()),
        "Links must land a symlink, not bytes"
    );
    assert_eq!(
        fs::read_link(&target).unwrap(),
        staged((&data, game), "reshade", "dxgi.dll")
    );
    assert_eq!(fs::read(&target).unwrap(), b"v1");
    assert!(m.backups.is_empty(), "fresh Links apply takes no backup");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn links_reapply_is_a_noop_and_never_writes_staging() {
    let game = "manual:standalone:links002";
    let (data, _stage, root) = setup(game);
    stage_payload(&data, game, "reshade", "dxgi.dll", b"v1");
    let before = fs::read(staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let h1 = crate::sha256_file(&staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let mut m = links_manifest(game, "reshade", "dxgi.dll", &h1, 0);
    let claims = claims_for(&data, game, &[m.clone()]);
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap();
    apply_copies(&data, game, &mut m, &root, None, &ops, &claims).unwrap();
    let claims = claim_inventory(&data, game).unwrap();
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap();
    apply_copies(&data, game, &mut m, &root, None, &ops, &claims).unwrap();
    assert_eq!(
        fs::read(staged((&data, game), "reshade", "dxgi.dll")).unwrap(),
        before,
        "re-apply must not write through the link into staging"
    );
    assert!(m.backups.is_empty());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn links_backs_up_a_foreign_symlink_as_resolved_bytes() {
    let game = "manual:standalone:links003";
    let (data, _stage, root) = setup(game);
    stage_payload(&data, game, "reshade", "dxgi.dll", b"v1");
    let h1 = crate::sha256_file(&staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    // A foreign symlink to unrelated bytes: not ours by target.
    fs::write(data.join("elsewhere.dll"), b"foreign-bytes").unwrap();
    std::os::unix::fs::symlink(data.join("elsewhere.dll"), root.join("dxgi.dll")).unwrap();
    let mut m = links_manifest(game, "reshade", "dxgi.dll", &h1, 0);
    let claims = claims_for(&data, game, &[m.clone()]);
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap();
    apply_copies(&data, game, &mut m, &root, None, &ops, &claims).unwrap();
    assert_eq!(m.backups.len(), 1);
    assert_eq!(
        fs::read(data.join(&m.backups["dxgi.dll"])).unwrap(),
        b"foreign-bytes",
        "a foreign symlink restores as its resolved bytes"
    );
    assert_eq!(
        fs::read_link(root.join("dxgi.dll")).unwrap(),
        staged((&data, game), "reshade", "dxgi.dll")
    );
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn links_removal_unlinks_owned_links_including_dangling_ones() {
    let game = "manual:standalone:links004";
    let (data, _stage, root) = setup(game);
    stage_payload(&data, game, "reshade", "dxgi.dll", b"v1");
    let h1 = crate::sha256_file(&staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let mut m = links_manifest(game, "reshade", "dxgi.dll", &h1, 0);
    let claims = claims_for(&data, game, &[m.clone()]);
    let ops = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap();
    apply_copies(&data, game, &mut m, &root, None, &ops, &claims).unwrap();
    // Dangle our own link by removing the staged file: removal must still
    // unlink it rather than leak it as missing content.
    fs::remove_file(staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let claims = claim_inventory(&data, game).unwrap();
    let out = remove_copies(
        &data,
        &mut m.backups,
        ["dxgi.dll"].into_iter(),
        game,
        &m.instance,
        &m.adapter,
        &m.files,
        &root,
        None,
        &claims,
    )
    .unwrap();
    assert_eq!(out.deleted, vec!["dxgi.dll".to_string()]);
    assert!(root.join("dxgi.dll").symlink_metadata().is_err());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn links_plan_refuses_before_mutation_when_symlinks_are_impossible() {
    let game = "manual:standalone:links005";
    let (data, _stage, _root) = setup(game);
    stage_payload(&data, game, "reshade", "dxgi.dll", b"v1");
    let h1 = crate::sha256_file(&staged((&data, game), "reshade", "dxgi.dll")).unwrap();
    let m = links_manifest(game, "reshade", "dxgi.dll", &h1, 0);
    // A regular file is not a directory: the capability probe cannot create
    // a probe link inside it, so planning fails before any dest mutates.
    let fake_root = data.join("not-a-dir");
    fs::write(&fake_root, b"nope").unwrap();
    let claims = ClaimInventory::default();
    let err = plan_copies(
        &data,
        game,
        "reshade",
        m.files.iter(),
        &fake_root,
        None,
        InstallMaterial::Links,
        &claims,
    )
    .unwrap_err();
    assert!(matches!(err, crate::Error::Install(_)), "{err}");
    let _ = fs::remove_dir_all(&data);
}
