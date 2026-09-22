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
                load: None,
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
                    load: None,
                },
                crate::download::PlannedFile {
                    source: "cache/k/f#b.txt".into(),
                    dest: "b.txt".into(),
                    sha256: hb.clone(),
                    enabled: true,
                    load: None,
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
                load: None,
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

const R21_GAME: &str = "manual:standalone:ffffffffff";
const R21_INST: &str = "opti";
const R21_INI: &[u8] = b"[Log]\nLogToFile=auto\nLogFileName=@TUXGT_RUNTIME@/logs/OptiScaler.log\n";

fn r21_stage(data: &Path, rel: &str) -> PathBuf {
    stage_dir(data, R21_GAME, R21_INST).join(rel)
}

fn r21_sync(
    data: &Path,
    mod_type: &str,
    rel: &str,
    src: &Path,
    sha: &str,
    force: bool,
) -> Result<()> {
    sync_staging(
        data,
        R21_GAME,
        R21_INST,
        mod_type,
        &[StageInput { rel, src, sha }],
        force,
    )
}

#[test]
fn optiscaler_ini_is_rewritten_and_hashes_recorded() {
    let data = data();
    let src = src_file(&data.join("depot"), "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&src).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &sha, false).unwrap();
    // Staged bytes carry the absolute per-game runtime path; depot bytes
    // are untouched (the rewrite is a staging-copy concern only).
    let staged = r21_stage(&data, "OptiScaler.ini");
    let abs = format!(
        "Z:{}",
        runtime_dir(&data, R21_GAME)
            .to_string_lossy()
            .replace('\\', "/")
    );
    let text = String::from_utf8(fs::read(&staged).unwrap()).unwrap();
    assert!(
        text.contains(&format!("LogFileName={abs}/logs/OptiScaler.log")),
        "{text}"
    );
    // Depot bytes are byte-for-byte unchanged and differ from the staged
    // output TuxGT rendered.
    assert_eq!(sha256_file(&src).unwrap(), sha, "depot must not change");
    assert_ne!(
        sha256_file(&staged).unwrap(),
        sha,
        "staged output must differ"
    );
    let recorded = staged_shas(&data, R21_GAME, R21_INST).unwrap();
    assert_eq!(recorded["OptiScaler.ini"], sha256_file(&staged).unwrap());
    let raw = fs::read_to_string(staging_toml_path(&data, R21_GAME, R21_INST)).unwrap();
    assert!(raw.contains("tuxgt_modified = true"), "{raw}");
    // Repeat sync is a byte-level no-op (idempotent re-render).
    let before = fs::read(&staged).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &sha, false).unwrap();
    assert_eq!(fs::read(&staged).unwrap(), before);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn depot_update_refreshes_untouched_rewrite_and_depot_stays_immutable() {
    let data = data();
    let up = data.join("depot");
    let src = src_file(&up, "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&src).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &sha, false).unwrap();
    let staged = r21_stage(&data, "OptiScaler.ini");
    let first = fs::read(&staged).unwrap();
    // Depot moves on: the untouched staged copy is re-rendered.
    let v2 = src_file(
        &up,
        "OptiScaler.ini.new",
        b"[Log]\nLogToFile=true\nLogFileName=@TUXGT_RUNTIME@/second.log\n",
    );
    let sha2 = sha256_file(&v2).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &v2, &sha2, false).unwrap();
    let now = fs::read(&staged).unwrap();
    assert_ne!(now, first);
    let text = String::from_utf8(now.clone()).unwrap();
    assert!(
        text.contains("LogToFile=true") && text.contains("/second.log"),
        "{text}"
    );
    assert_eq!(
        staged_shas(&data, R21_GAME, R21_INST).unwrap()["OptiScaler.ini"],
        sha256_file(&staged).unwrap()
    );
    // Depot sources still hold exactly what was written to them.
    assert_eq!(sha256_file(&up.join("OptiScaler.ini")).unwrap(), sha);
    assert_eq!(sha256_file(&up.join("OptiScaler.ini.new")).unwrap(), sha2);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn user_edit_survives_depot_update_until_force() {
    let data = data();
    let up = data.join("depot");
    let src = src_file(&up, "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&src).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &sha, false).unwrap();
    let staged = r21_stage(&data, "OptiScaler.ini");
    fs::write(&staged, b"[Log]\nLogFileName=C:\\mine.log\n").unwrap();
    let user = fs::read(&staged).unwrap();
    let v2 = src_file(
        &up,
        "OptiScaler.ini.new",
        b"[Log]\nLogFileName=@TUXGT_RUNTIME@/v2.log\n",
    );
    let sha2 = sha256_file(&v2).unwrap();
    let err = r21_sync(&data, "optiscaler", "OptiScaler.ini", &v2, &sha2, false).unwrap_err();
    assert!(matches!(err, Error::StagedModified(_)), "{err}");
    assert_eq!(fs::read(&staged).unwrap(), user, "user edit must survive");
    assert_eq!(sha256_file(&up.join("OptiScaler.ini.new")).unwrap(), sha2);
    // Force is the explicit discard path and re-renders from the depot.
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &v2, &sha2, true).unwrap();
    let text = String::from_utf8(fs::read(&staged).unwrap()).unwrap();
    assert!(text.contains("/v2.log"), "{text}");
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn rewrite_failure_preserves_staged_bytes_and_toml() {
    let data = data();
    let up = data.join("depot");
    let src = src_file(&up, "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&src).unwrap();
    r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &sha, false).unwrap();
    let staged = r21_stage(&data, "OptiScaler.ini");
    let toml_path = staging_toml_path(&data, R21_GAME, R21_INST);
    let good_bytes = fs::read(&staged).unwrap();
    let good_toml = fs::read(&toml_path).unwrap();
    // Depot source turns into non-UTF-8 under the same rel: rewrite fails,
    // so the previous staged copy and bookkeeping stand.
    fs::write(&src, b"[Log]\nLogFileName=\xff\xfe\n").unwrap();
    let bad = sha256_file(&src).unwrap();
    let err = r21_sync(&data, "optiscaler", "OptiScaler.ini", &src, &bad, false).unwrap_err();
    assert!(matches!(err, Error::Manifest(_)), "{err}");
    assert_eq!(fs::read(&staged).unwrap(), good_bytes);
    assert_eq!(fs::read(&toml_path).unwrap(), good_toml);
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn non_optiscaler_types_and_files_stay_byte_identical() {
    let data = data();
    let up = data.join("depot");
    // A token-bearing ini under a non-OptiScaler type is copied verbatim.
    let ini = src_file(&up, "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&ini).unwrap();
    r21_sync(&data, "custom", "OptiScaler.ini", &ini, &sha, false).unwrap();
    assert_eq!(
        fs::read(r21_stage(&data, "OptiScaler.ini")).unwrap(),
        R21_INI
    );
    // An OptiScaler payload without the ini is copied verbatim too.
    let dll = src_file(&up, "dxgi.dll", &[0xff, 0xfe, 0x00, 0x42]);
    let dsha = sha256_file(&dll).unwrap();
    r21_sync(&data, "optiscaler", "dxgi.dll", &dll, &dsha, false).unwrap();
    assert_eq!(
        fs::read(r21_stage(&data, "dxgi.dll")).unwrap(),
        [0xff, 0xfe, 0x00, 0x42]
    );
    assert_eq!(
        staged_shas(&data, R21_GAME, R21_INST).unwrap()["dxgi.dll"],
        dsha
    );
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn unknown_mod_type_fails_closed_before_staging() {
    let data = data();
    let src = src_file(&data.join("depot"), "dxgi.dll", b"v1");
    let sha = sha256_file(&src).unwrap();
    let err = r21_sync(&data, "nope", "dxgi.dll", &src, &sha, false).unwrap_err();
    assert!(matches!(err, Error::InvalidModType(_)), "{err}");
    assert!(!r21_stage(&data, "dxgi.dll").exists());
    assert!(!staging_toml_path(&data, R21_GAME, R21_INST).exists());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn set_staged_re_renders_rewrite_on_keep_and_unknown_field_survives() {
    let data = data();
    let up = data.join("depot");
    let src = src_file(&up, "OptiScaler.ini", R21_INI);
    let sha = sha256_file(&src).unwrap();
    let toml_path = staging_toml_path(&data, R21_GAME, R21_INST);
    set_staged(
        &data,
        R21_GAME,
        R21_INST,
        "optiscaler",
        "OptiScaler.ini",
        Some(&src),
        &sha,
        true,
    )
    .unwrap();
    let abs = format!(
        "Z:{}",
        runtime_dir(&data, R21_GAME)
            .to_string_lossy()
            .replace('\\', "/")
    );
    let text = String::from_utf8(fs::read(r21_stage(&data, "OptiScaler.ini")).unwrap()).unwrap();
    assert!(text.contains(&abs), "{text}");
    // An unknown future root field round-trips through the rewrite pass.
    let raw = fs::read_to_string(&toml_path).unwrap();
    fs::write(&toml_path, format!("rewrite_version = 3\n{raw}")).unwrap();
    let v2 = src_file(
        &up,
        "OptiScaler.ini.new",
        b"[Log]\nLogFileName=@TUXGT_RUNTIME@/later.log\n",
    );
    let sha2 = sha256_file(&v2).unwrap();
    // Depot moved on: keep re-renders instead of only re-recording the hash.
    set_staged(
        &data,
        R21_GAME,
        R21_INST,
        "optiscaler",
        "OptiScaler.ini",
        Some(&v2),
        &sha2,
        true,
    )
    .unwrap();
    let text = String::from_utf8(fs::read(r21_stage(&data, "OptiScaler.ini")).unwrap()).unwrap();
    assert!(text.contains("/later.log"), "{text}");
    let raw = fs::read_to_string(&toml_path).unwrap();
    assert!(raw.contains("rewrite_version = 3"), "{raw}");
    // Omit drops the copy and its bookkeeping, rewriting nothing.
    set_staged(
        &data,
        R21_GAME,
        R21_INST,
        "optiscaler",
        "OptiScaler.ini",
        None,
        "",
        false,
    )
    .unwrap();
    assert!(!r21_stage(&data, "OptiScaler.ini").exists());
    assert!(!staged_shas(&data, R21_GAME, R21_INST)
        .unwrap()
        .contains_key("OptiScaler.ini"));
    let _ = fs::remove_dir_all(&data);
}
