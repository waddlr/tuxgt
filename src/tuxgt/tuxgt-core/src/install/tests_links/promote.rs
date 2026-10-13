//! Copy/Links contested-dest promotion on winner removal.
use super::super::testing::*;
use super::super::*;
use super::{links_manifest, stage_payload, staged};
use crate::FileManifest;
use std::collections::BTreeSet;
use std::fs;

struct Contest {
    data: std::path::PathBuf,
    root: std::path::PathBuf,
    game: String,
}

/// Two enabled claimants on one dest: `aaa` (load 0) then `bbb` (load 1),
/// applied in load order over a foreign original, in the given materials.
fn contested(game: &str, material_a: InstallMaterial, material_b: InstallMaterial) -> Contest {
    let (data, _stage, root) = setup(game);
    stage_payload(&data, game, "aaa", "dxgi.dll", b"AAA");
    stage_payload(&data, game, "bbb", "dxgi.dll", b"BBB");
    fs::write(root.join("dxgi.dll"), b"FOREIGN").unwrap();
    let ha = crate::sha256_file(&staged((&data, game), "aaa", "dxgi.dll")).unwrap();
    let hb = crate::sha256_file(&staged((&data, game), "bbb", "dxgi.dll")).unwrap();
    let mut a = test_manifest(game, "aaa", "dxgi.dll", &ha);
    a.adapter = if material_a.is_links() {
        "install-links"
    } else {
        "install"
    }
    .into();
    a.load_order = 0;
    let mut b = test_manifest(game, "bbb", "dxgi.dll", &hb);
    b.adapter = if material_b.is_links() {
        "install-links"
    } else {
        "install"
    }
    .into();
    b.load_order = 1;
    for m in [&mut a, &mut b] {
        crate::write_manifest(&data, m).unwrap();
        let claims = claim_inventory(&data, game).unwrap();
        let material = InstallMaterial::of(&m.adapter);
        let ops = plan_copies(
            &data,
            game,
            &m.instance.clone(),
            m.files.iter(),
            &root,
            None,
            material,
            &claims,
        )
        .unwrap();
        apply_copies(&data, game, m, &root, None, &ops, &claims).unwrap();
    }
    Contest {
        data,
        root,
        game: game.into(),
    }
}

fn live_bytes(fx: &Contest) -> Vec<u8> {
    fs::read(fx.root.join("dxgi.dll")).unwrap()
}

fn live_link(fx: &Contest) -> Option<std::path::PathBuf> {
    fs::read_link(fx.root.join("dxgi.dll")).ok()
}

fn manifest_of(fx: &Contest, instance: &str) -> FileManifest {
    crate::need_manifest(&fx.data, &fx.game, instance).unwrap()
}

/// Uninstall semantics: run the promotion-aware removal, then drop the
/// departing manifest so later removals see the true remaining claims.
fn remove_instance(fx: &Contest, instance: &str) -> PromotionOutcome {
    let departing = manifest_of(fx, instance);
    let excluded = departing
        .files
        .iter()
        .map(|f| (departing.instance.clone(), f.dest.clone()))
        .collect::<BTreeSet<_>>();
    let dests = departing
        .files
        .iter()
        .map(|f| f.dest.clone())
        .collect::<Vec<_>>();
    let mut backups = departing.backups.clone();
    let outcome = remove_with_promotion(
        &fx.data,
        &fx.game,
        departing,
        &excluded,
        &mut backups,
        &dests,
        &fx.root,
        None,
    )
    .unwrap();
    fs::remove_file(crate::manifest_path(&fx.data, &fx.game, instance)).unwrap();
    outcome
}

#[test]
fn copy_winner_removal_promotes_loser_bytes() {
    let fx = contested(
        "manual:standalone:links010",
        InstallMaterial::Copy,
        InstallMaterial::Copy,
    );
    assert_eq!(live_bytes(&fx), b"BBB");
    // Removing the winner promotes the loser; removing the last claimant
    // restores the foreign original the first claimant backed up.
    let outcome = remove_instance(&fx, "bbb");
    assert_eq!(live_bytes(&fx), b"AAA");
    assert!(outcome.promoted.contains(&"dxgi.dll".to_string()));
    let _ = remove_instance(&fx, "aaa");
    assert_eq!(live_bytes(&fx), b"FOREIGN");
    let _ = fs::remove_dir_all(&fx.data);
}

#[test]
fn copy_loser_removal_preserves_winner_and_hands_over_the_backup() {
    let fx = contested(
        "manual:standalone:links014",
        InstallMaterial::Copy,
        InstallMaterial::Copy,
    );
    // Removing the loser leaves the winner and its dest untouched, and the
    // foreign backup record moves to the surviving winner.
    let _ = remove_instance(&fx, "aaa");
    assert_eq!(live_bytes(&fx), b"BBB");
    assert!(manifest_of(&fx, "bbb").backups.contains_key("dxgi.dll"));
    // The winner is now the last claimant: its removal restores the
    // foreign original through the transferred record.
    let _ = remove_instance(&fx, "bbb");
    assert_eq!(live_bytes(&fx), b"FOREIGN");
    let _ = fs::remove_dir_all(&fx.data);
}

#[test]
fn links_winner_removal_promotes_loser_link() {
    let fx = contested(
        "manual:standalone:links011",
        InstallMaterial::Links,
        InstallMaterial::Links,
    );
    assert_eq!(
        live_link(&fx).unwrap(),
        staged((&fx.data, &fx.game), "bbb", "dxgi.dll")
    );
    let _ = remove_instance(&fx, "bbb");
    assert_eq!(
        live_link(&fx).unwrap(),
        staged((&fx.data, &fx.game), "aaa", "dxgi.dll")
    );
    let _ = remove_instance(&fx, "aaa");
    assert_eq!(live_bytes(&fx), b"FOREIGN");
    assert!(
        fs::symlink_metadata(fx.root.join("dxgi.dll"))
            .is_ok_and(|meta| !meta.file_type().is_symlink()),
        "the restored foreign original is a regular file"
    );
    let _ = fs::remove_dir_all(&fx.data);
}

#[test]
fn links_loser_removal_preserves_winner_link_and_backup() {
    let fx = contested(
        "manual:standalone:links015",
        InstallMaterial::Links,
        InstallMaterial::Links,
    );
    let _ = remove_instance(&fx, "aaa");
    assert_eq!(
        live_link(&fx).unwrap(),
        staged((&fx.data, &fx.game), "bbb", "dxgi.dll"),
        "loser removal must not disturb the winner's link"
    );
    assert!(manifest_of(&fx, "bbb").backups.contains_key("dxgi.dll"));
    let _ = remove_instance(&fx, "bbb");
    assert_eq!(live_bytes(&fx), b"FOREIGN");
    let _ = fs::remove_dir_all(&fx.data);
}

/// Two-dest Links contest: `aaa` (load 0) then `bbb` (load 1) applied in
/// load order over foreign originals.
fn contested_two_dests(game: &str) -> Contest {
    let dests = ["dxgi.dll", "d3d11.dll"];
    let (data, _stage, root) = setup(game);
    for dest in dests {
        stage_payload(&data, game, "aaa", dest, b"AAA");
        stage_payload(&data, game, "bbb", dest, b"BBB");
        fs::write(root.join(dest), b"FOREIGN").unwrap();
    }
    let mut built = Vec::new();
    for (instance, load) in [("aaa", 0), ("bbb", 1)] {
        let first = staged((&data, game), instance, dests[0]);
        let hash = crate::sha256_file(&first).unwrap();
        let mut m = links_manifest(game, instance, dests[0], &hash, load);
        let mut files = m.files.to_vec();
        for dest in &dests[1..] {
            let hash = crate::sha256_file(&staged((&data, game), instance, dest)).unwrap();
            let mut file = files[0].clone();
            file.dest = (*dest).to_string();
            file.sha256 = hash;
            files.push(file);
        }
        m.files = files.into_boxed_slice();
        built.push(m);
    }
    for m in &mut built {
        crate::write_manifest(&data, m).unwrap();
        let claims = claim_inventory(&data, game).unwrap();
        let material = InstallMaterial::of(&m.adapter);
        let ops = plan_copies(
            &data,
            game,
            &m.instance.clone(),
            m.files.iter(),
            &root,
            None,
            material,
            &claims,
        )
        .unwrap();
        apply_copies(&data, game, m, &root, None, &ops, &claims).unwrap();
    }
    Contest {
        data,
        root,
        game: game.into(),
    }
}

#[test]
fn loser_removal_hands_every_backup_record_to_the_winner() {
    let fx = contested_two_dests("manual:standalone:links020");
    for dest in ["dxgi.dll", "d3d11.dll"] {
        assert_eq!(
            fs::read_link(fx.root.join(dest)).unwrap(),
            staged((&fx.data, &fx.game), "bbb", dest)
        );
    }
    // Removing the loser leaves both winners live and moves both
    // foreign-original records to the surviving manifest: persisting the
    // second handoff must not drop the first.
    let _ = remove_instance(&fx, "aaa");
    for dest in ["dxgi.dll", "d3d11.dll"] {
        assert_eq!(
            fs::read_link(fx.root.join(dest)).unwrap(),
            staged((&fx.data, &fx.game), "bbb", dest),
            "loser removal must not disturb either winner link"
        );
    }
    let survivor = manifest_of(&fx, "bbb");
    assert!(
        survivor.backups.contains_key("dxgi.dll") && survivor.backups.contains_key("d3d11.dll"),
        "both backup records survive: {:?}",
        survivor.backups.keys().collect::<Vec<_>>()
    );
    let _ = remove_instance(&fx, "bbb");
    for dest in ["dxgi.dll", "d3d11.dll"] {
        assert_eq!(fs::read(fx.root.join(dest)).unwrap(), b"FOREIGN");
    }
    let _ = fs::remove_dir_all(&fx.data);
}

#[test]
fn mixed_materials_promote_in_the_claimants_own_kind() {
    let fx = contested(
        "manual:standalone:links012",
        InstallMaterial::Links,
        InstallMaterial::Copy,
    );
    assert_eq!(live_bytes(&fx), b"BBB");
    // Copy winner departs: the Links loser is promoted as a symlink.
    let _ = remove_instance(&fx, "bbb");
    assert_eq!(
        live_link(&fx).unwrap(),
        staged((&fx.data, &fx.game), "aaa", "dxgi.dll")
    );
    let fx = contested(
        "manual:standalone:links013",
        InstallMaterial::Copy,
        InstallMaterial::Links,
    );
    assert_eq!(
        live_link(&fx).unwrap(),
        staged((&fx.data, &fx.game), "bbb", "dxgi.dll")
    );
    // Links winner departs: the Copy loser is promoted as a regular file.
    let _ = remove_instance(&fx, "bbb");
    assert_eq!(live_bytes(&fx), b"AAA");
    assert!(
        fs::symlink_metadata(fx.root.join("dxgi.dll"))
            .is_ok_and(|meta| !meta.file_type().is_symlink()),
        "a promoted Copy claim is a regular file"
    );
    let _ = fs::remove_dir_all(&fx.data);
}
