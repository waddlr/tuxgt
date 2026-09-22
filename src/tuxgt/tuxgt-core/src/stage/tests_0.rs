use super::testing::*;
use super::*;
use crate::Error;
use std::fs;

#[test]
fn sync_and_status_roundtrip() {
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "dxgi.dll", b"v1");
    let ha = sha256_file(&a).unwrap();
    sync_staging(
        &data,
        "manual:standalone:aaaaaaaa",
        "optiscaler",
        &[StageInput {
            rel: "dxgi.dll",
            src: &a,
            sha: &ha,
        }],
        false,
    )
    .unwrap();
    assert!(stage_dir(&data, "manual:standalone:aaaaaaaa", "optiscaler")
        .join("dxgi.dll")
        .is_file());
    // Re-sync is a no-op (in sync).
    sync_staging(
        &data,
        "manual:standalone:aaaaaaaa",
        "optiscaler",
        &[StageInput {
            rel: "dxgi.dll",
            src: &a,
            sha: &ha,
        }],
        false,
    )
    .unwrap();
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn user_modified_blocks_without_force() {
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "dxgi.dll", b"v1");
    let ha = sha256_file(&a).unwrap();
    sync_staging(
        &data,
        "steam::1",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &a,
            sha: &ha,
        }],
        false,
    )
    .unwrap();
    // User edits the staged file.
    fs::write(
        stage_dir(&data, "steam::1", "reshade").join("dxgi.dll"),
        b"user",
    )
    .unwrap();
    let b = src_file(&up, "dxgi2.dll", b"v2");
    let hb = sha256_file(&b).unwrap();
    let err = sync_staging(
        &data,
        "steam::1",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &b,
            sha: &hb,
        }],
        false,
    )
    .unwrap_err();
    assert!(matches!(err, Error::StagedModified(_)));
    // Force overwrites.
    sync_staging(
        &data,
        "steam::1",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &b,
            sha: &hb,
        }],
        true,
    )
    .unwrap();
    assert_eq!(
        fs::read(stage_dir(&data, "steam::1", "reshade").join("dxgi.dll")).unwrap(),
        b"v2"
    );
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn bad_rel_rejected() {
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "x", b"x");
    let ha = sha256_file(&a).unwrap();
    for rel in ["../evil.dll", "/abs.dll", "a\\b.dll", ""] {
        let err = sync_staging(
            &data,
            "manual:standalone:bbbbbbbb",
            "reshade",
            &[StageInput {
                rel,
                src: &a,
                sha: &ha,
            }],
            false,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Manifest(_)), "{rel}");
    }
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn depot_moved_recopies_when_in_sync() {
    use crate::{write_manifest, FileManifest};
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "dxgi.dll", b"v1");
    let ha = sha256_file(&a).unwrap();
    sync_staging(
        &data,
        "manual:standalone:cccccccc",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &a,
            sha: &ha,
        }],
        false,
    )
    .unwrap();
    // Depot moved on (new download, same rel): in-sync staging re-copies.
    let b = src_file(&up, "dxgi-new.dll", b"v2");
    let hb = sha256_file(&b).unwrap();
    sync_staging(
        &data,
        "manual:standalone:cccccccc",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &b,
            sha: &hb,
        }],
        false,
    )
    .unwrap();
    assert_eq!(
        fs::read(stage_dir(&data, "manual:standalone:cccccccc", "reshade").join("dxgi.dll"))
            .unwrap(),
        b"v2"
    );
    // Status still in-sync once the manifest records the new hash.
    write_manifest(
        &data,
        &FileManifest {
            game: "manual:standalone:cccccccc".into(),
            instance: "reshade".into(),
            mod_type: "reshade".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![crate::download::PlannedFile {
                source: "cache/k/f#dxgi.dll".into(),
                dest: "dxgi.dll".into(),
                sha256: hb.clone(),
                enabled: true,
            }]
            .into_boxed_slice(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
            env: Box::default(),
        },
    )
    .unwrap();
    let lines = stage_status(&data, "manual:standalone:cccccccc").unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].state, StageState::InSync);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn status_depot_newer_and_prune() {
    use crate::{write_manifest, FileManifest};
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "a.dll", b"a1");
    let ha = sha256_file(&a).unwrap();
    let b = src_file(&up, "b.txt", b"b1");
    let hb = sha256_file(&b).unwrap();
    sync_staging(
        &data,
        "manual:standalone:dddddddd",
        "reshade",
        &[
            StageInput {
                rel: "a.dll",
                src: &a,
                sha: &ha,
            },
            StageInput {
                rel: "b.txt",
                src: &b,
                sha: &hb,
            },
        ],
        false,
    )
    .unwrap();
    // Manifest moved on for a.dll only: depot-newer there, in-sync else.
    write_manifest(
        &data,
        &FileManifest {
            game: "manual:standalone:dddddddd".into(),
            instance: "reshade".into(),
            mod_type: "reshade".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![
                crate::download::PlannedFile {
                    source: "cache/k/f#a.dll".into(),
                    dest: "a.dll".into(),
                    sha256: "newhash".into(),
                    enabled: true,
                },
                crate::download::PlannedFile {
                    source: "cache/k/f#b.txt".into(),
                    dest: "b.txt".into(),
                    sha256: hb.clone(),
                    enabled: true,
                },
            ]
            .into_boxed_slice(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
            env: Box::default(),
        },
    )
    .unwrap();
    let lines = stage_status(&data, "manual:standalone:dddddddd").unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].file, "a.dll");
    assert_eq!(lines[0].state, StageState::DepotNewer);
    assert_eq!(lines[1].state, StageState::InSync);
    // Dropping b.txt from the plan prunes its staged copy.
    sync_staging(
        &data,
        "manual:standalone:dddddddd",
        "reshade",
        &[StageInput {
            rel: "a.dll",
            src: &a,
            sha: "newhash",
        }],
        true,
    )
    .unwrap();
    assert!(!stage_dir(&data, "manual:standalone:dddddddd", "reshade")
        .join("b.txt")
        .exists());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn uninstall_drops_runtime_dests_keeps_others_and_generated() {
    let data = data();
    let gid = "steam::1";
    let rt = runtime_dir(&data, gid);
    fs::create_dir_all(rt.join("reshade-shaders").join("Shaders")).unwrap();
    fs::write(rt.join("ShaderToggler.addon64"), b"addon").unwrap();
    fs::write(
        rt.join("reshade-shaders")
            .join("Shaders")
            .join("ArcaneBloom.fx"),
        b"fx",
    )
    .unwrap();
    fs::write(
        rt.join("reshade-shaders")
            .join("Shaders")
            .join("ReShade.fxh"),
        b"hdr",
    )
    .unwrap();
    fs::write(rt.join("ReShade.ini"), b"generated").unwrap();
    let mut keep = BTreeSet::new();
    keep.insert("reshade-shaders/Shaders/ReShade.fxh".into());
    remove_runtime_dests(
        &data,
        gid,
        [
            "ShaderToggler.addon64",
            "reshade-shaders/Shaders/ArcaneBloom.fx",
        ],
        &keep,
    )
    .unwrap();
    assert!(!rt.join("ShaderToggler.addon64").exists());
    assert!(!rt
        .join("reshade-shaders")
        .join("Shaders")
        .join("ArcaneBloom.fx")
        .exists());
    assert!(rt
        .join("reshade-shaders")
        .join("Shaders")
        .join("ReShade.fxh")
        .is_file());
    assert_eq!(fs::read(rt.join("ReShade.ini")).unwrap(), b"generated");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn status_reports_hand_dropped_file_as_unmanaged() {
    use crate::{write_manifest, FileManifest};
    let data = data();
    let up = data.join("up");
    let a = src_file(&up, "dxgi.dll", b"v1");
    let ha = sha256_file(&a).unwrap();
    sync_staging(
        &data,
        "manual:standalone:eeeeeeee",
        "reshade",
        &[StageInput {
            rel: "dxgi.dll",
            src: &a,
            sha: &ha,
        }],
        false,
    )
    .unwrap();
    write_manifest(
        &data,
        &FileManifest {
            game: "manual:standalone:eeeeeeee".into(),
            instance: "reshade".into(),
            mod_type: "reshade".into(),
            adapter: "preload".into(),
            enabled: true,
            load_order: 0,
            include: Box::default(),
            files: vec![crate::download::PlannedFile {
                source: "cache/k/f#dxgi.dll".into(),
                dest: "dxgi.dll".into(),
                sha256: ha.clone(),
                enabled: true,
            }]
            .into_boxed_slice(),
            backups: Default::default(),
            generated_globs: Box::default(),
            harvested: Default::default(),
            provenance: crate::ModProvenance::default(),
            env: Box::default(),
        },
    )
    .unwrap();
    // Hand-dropped file: present in staging, claimed by nothing.
    fs::write(
        stage_dir(&data, "manual:standalone:eeeeeeee", "reshade").join("test.fx"),
        b"drop-in",
    )
    .unwrap();
    let lines = stage_status(&data, "manual:standalone:eeeeeeee").unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].file, "test.fx");
    assert_eq!(lines[1].state, StageState::Unmanaged);
    // Lost bookkeeping (deleted toml) must not double-report the managed
    // file as both InSync and Unmanaged.
    fs::remove_file(staging_toml_path(
        &data,
        "manual:standalone:eeeeeeee",
        "reshade",
    ))
    .unwrap();
    let lines = stage_status(&data, "manual:standalone:eeeeeeee").unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].file, "dxgi.dll");
    assert_eq!(lines[0].state, StageState::InSync);
    let _ = fs::remove_dir_all(&data);
}
