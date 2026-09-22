use super::testing::*;
use super::*;
use crate::game::GameId;
use crate::testing::{seed_game, SeedGame};
use crate::{set_custom, set_knob, set_wrapper, Error};
use std::fs;

#[tokio::test]
async fn handle_off_session_carries_trampoline_env() {
    // E80: `inject=` is hook-only. A handle-off session still carries
    // env + wrappers so an applied trampoline self-arms from argv.
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_wrapper(&pool, &id, "gamescope").await.unwrap();
    assert!(!game_handle(&pool, &id).await.unwrap());
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_none());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    assert!(text.contains("TUXGT_GAME_DIR="), "{text}");
    assert!(text.contains("MANGOHUD=1"), "{text}");
    assert!(text.contains("WRAPPERS=gamescope"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn handle_on_writes_knobs_and_wrappers() {
    let (pool, dir, host, id) = setup().await;
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/libtuxgt-launcher.so"), b"so").unwrap();
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_custom(&pool, &id, "FOO", "bar").await.unwrap();
    set_wrapper(&pool, &id, "gamescope").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=1"), "{text}");
    assert!(text.contains("MANGOHUD=1"), "{text}");
    assert!(text.contains("FOO=bar"), "{text}");
    assert!(text.contains("WRAPPERS=gamescope"), "{text}");
    assert!(text.contains("TUXGT_LAUNCHER_SO="), "{text}");
    assert!(!text.contains("LD_PRELOAD="), "{text}");
    assert!(text.contains("TUXGT_LAUNCHER_INI="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
#[tokio::test]
async fn handle_on_restores_apply_record() {
    // E80 never-both (handle direction): handle-on while applied
    // restores the trampoline first, then arms `inject=1`.
    let (pool, dir, host, id) = setup().await;
    // E94: an arm with nothing to inject auto-restores, so the fixture
    // needs a live need for the Hook arm to hold.
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_none());
    assert!(game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=1"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_on_end_state_is_trampoline_only() {
    // E80 never-both (apply direction): Apply ⇒ handle off, then the
    // trampoline record. Exactly one channel: record present,
    // `inject=0`, session still carrying env the trampoline would load.
    let (pool, dir, host, id) = setup().await;
    // E94: keep the Hook arm legal through the sequence.
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    assert!(game_handle(&pool, &id).await.unwrap());
    // Same two steps `apply_launch` performs (minus the store write,
    // which needs a live Steam/Heroic config): handle off, then record.
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    assert!(!game_handle(&pool, &id).await.unwrap());
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_some());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    assert!(text.contains("TUXGT_GAME_DIR="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn sync_session_disarms_hook_when_needs_are_gone() {
    // E94: clearing the last need drops the armed channel at the same
    // write, so the radio shows Not hooked without a click.
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    assert!(game_handle(&pool, &id).await.unwrap());
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(!game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    assert!(!text.contains("MANGOHUD="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn sync_session_restores_apply_when_needs_are_gone() {
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    crate::apply::write_record(
        &dir,
        &crate::apply::ApplyRecord {
            game: id.clone(),
            manager: "steam".into(),
            launcher: "/tmp/tuxgt-launcher".into(),
            applied_at: 0,
            files: vec![],
        },
    )
    .unwrap();
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(crate::apply::read_record(&dir, &id).unwrap().is_none());
    assert!(!game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=0"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn sync_session_keeps_armed_mode_while_a_need_remains() {
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_wrapper(&pool, &id, "gamescope").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(game_handle(&pool, &id).await.unwrap());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn corrupt_apply_record_never_blocks_the_session_write() {
    // E94: the auto-restore is best-effort. A record we cannot parse
    // leaves both arms alone (never half-disarmed) but must not stop the
    // render the caller asked for: the cleared env still lands.
    let (pool, dir, host, id) = setup().await;
    let gid = GameId::parse(&id).unwrap();
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    fs::write(
        crate::game::game_dir(&dir, &gid).join("apply.toml"),
        b"not = toml = at = all",
    )
    .unwrap();
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    assert!(game_handle(&pool, &id).await.unwrap());
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("inject=1"), "{text}");
    assert!(!text.contains("MANGOHUD="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn apply_unsupported_leaves_handle_untouched() {
    // Manual rows have no trampoline channel: `apply_launch` fails via
    // the provider without flipping the hook channel.
    let (pool, dir, host, _) = setup().await;
    let mid = "manual:standalone:abcdef12";
    seed_game(
        &pool,
        SeedGame {
            id: "manual:standalone:abcdef12",
            manager: "manual",
            store: "standalone",
            game_id: "abcdef12",
            name: Some("m"),
            exe_path: Some(""),
            prefix_path: Some(""),
            ..Default::default()
        },
    )
    .await;
    // E94: a knob keeps the arm legal so the row stays armed across the
    // failed Apply (which must not flip the hook channel either way).
    set_knob(&pool, mid, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, mid, true).await.unwrap();
    let err = crate::apply::apply_launch(&pool, &dir, &host, mid)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ApplyUnsupported(_)), "{err:?}");
    assert!(game_handle(&pool, mid).await.unwrap());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn trampoline_self_arms_at_inject_zero() {
    // E80: a correlated session self-arms the trampoline (session env +
    // `.so` preload, append-only over the existing `LD_PRELOAD`) even at
    // `inject=0`; a correlator miss keeps the owned/harness preload path.
    use crate::set_override;
    let (pool, dir, host, id) = setup().await;
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let so = dir.join("lib/libtuxgt-launcher.so");
    std::fs::write(&so, b"so").unwrap();
    let probe = dir.join("probe.exe");
    std::fs::write(
        &probe,
        "#!/bin/sh\necho \"PROBE_LD_PRELOAD=$LD_PRELOAD\"\necho \"PROBE_FOO=$FOO\"\n",
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&probe)
        .status()
        .unwrap();
    set_custom(&pool, &id, "FOO", "bar").await.unwrap();
    let exe = probe.to_string_lossy().into_owned();
    set_override(&pool, &id, "exe", Some(exe.as_str()))
        .await
        .unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    let launcher =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../launcher/tuxgt-launcher");
    let run = |prefix: &str| {
        std::process::Command::new("sh")
            .arg(&launcher)
            .arg(&probe)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &dir)
            .env("TUXGT_DATA", &dir)
            .env("TUXGT_LAUNCHER_SO", &so)
            .env("STEAM_COMPAT_DATA_PATH", prefix)
            .env("LD_PRELOAD", "/sentinel/keep.so")
            .output()
            .unwrap()
    };
    let hit = run("/pfx/sekiro");
    assert!(hit.status.success());
    let out = String::from_utf8(hit.stdout).unwrap();
    assert!(out.contains("PROBE_FOO=bar"), "{out}");
    assert!(out.contains("/sentinel/keep.so"), "{out}");
    assert!(out.contains("libtuxgt-launcher.so"), "{out}");
    let miss = run("/pfx/unknown");
    assert!(miss.status.success());
    let out = String::from_utf8(miss.stdout).unwrap();
    assert!(out.contains("libtuxgt-launcher.so"), "{out}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn correlator_requires_prefix_and_exe() {
    let (pool, dir, host, id) = setup().await;
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    set_handle(&pool, &dir, &host, "heroic:gog:abc", true)
        .await
        .unwrap();
    let text = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(text.contains("[/pfx/sekiro]"), "{text}");
    assert!(
        text.contains("/games/sekiro/sekiro.exe=steam/814380"),
        "{text}"
    );
    assert!(text.contains("[/pfx/flag]"), "{text}");
    assert!(
        text.contains("/games/flag/flag.exe=heroic_gog/abc"),
        "{text}"
    );
    assert!(!text.contains("[exe]"), "{text}");
    assert!(!dir.join("run").exists());
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

fn opti_manifest(game: &str, instance: &str, mod_type: &str) -> crate::FileManifest {
    crate::FileManifest {
        game: game.into(),
        instance: instance.into(),
        mod_type: mod_type.into(),
        adapter: "preload".into(),
        enabled: true,
        load_order: 0,
        include: Box::default(),
        files: Box::default(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}

async fn proton_row(
    pool: &sqlx::SqlitePool,
    id: &str,
    detected_proton: &str,
    override_platform: Option<&str>,
) {
    seed_game(
        pool,
        SeedGame {
            id,
            manager: "steam",
            game_id: "r59opti",
            name: Some("opti"),
            exe_path: Some("/games/opti/opti.exe"),
            prefix_path: Some("/pfx/opti"),
            detected_proton: Some(detected_proton),
            override_platform,
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn proton_env_granted_on_ge_with_official_plan() {
    let (pool, dir, host, _) = setup().await;
    proton_row(&pool, "steam::r59opti", "GE-Proton10-4", None).await;
    crate::write_manifest(
        &dir,
        &opti_manifest("steam::r59opti", "optiscaler", "optiscaler"),
    )
    .unwrap();
    set_handle(&pool, &dir, &host, "steam::r59opti", true)
        .await
        .unwrap();
    let gid = GameId::parse("steam::r59opti").unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(text.contains("PROTON_USE_OPTISCALER=1"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn no_proton_env_without_flavor() {
    let (pool, dir, host, _) = setup().await;
    proton_row(&pool, "steam::r59opti", "Proton 9.0 (Valve)", None).await;
    crate::write_manifest(
        &dir,
        &opti_manifest("steam::r59opti", "optiscaler", "optiscaler"),
    )
    .unwrap();
    set_handle(&pool, &dir, &host, "steam::r59opti", true)
        .await
        .unwrap();
    let gid = GameId::parse("steam::r59opti").unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(!text.contains("PROTON_USE_OPTISCALER"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn no_proton_env_without_plan_or_native_override() {
    let (pool, dir, host, _) = setup().await;
    proton_row(&pool, "steam::r59opti", "GE-Proton10-4", None).await;
    crate::write_manifest(&dir, &opti_manifest("steam::r59opti", "reshade", "reshade")).unwrap();
    set_handle(&pool, &dir, &host, "steam::r59opti", true)
        .await
        .unwrap();
    let gid = GameId::parse("steam::r59opti").unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(!text.contains("PROTON_USE_OPTISCALER"), "{text}");
    proton_row(&pool, "steam::r59native", "GE-Proton10-4", Some("native")).await;
    crate::write_manifest(
        &dir,
        &opti_manifest("steam::r59native", "optiscaler", "optiscaler"),
    )
    .unwrap();
    set_handle(&pool, &dir, &host, "steam::r59native", true)
        .await
        .unwrap();
    let gid = GameId::parse("steam::r59native").unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(!text.contains("PROTON_USE_OPTISCALER"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn trampoline_falls_back_to_single_rel_prefix() {
    // Prefix fallback, trampoline side: an argv exe with no key of its
    // own still arms the session when its prefix section names one rel.
    let (pool, dir, host, id) = setup().await;
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let so = dir.join("lib/libtuxgt-launcher.so");
    std::fs::write(&so, b"so").unwrap();
    let probe = dir.join("probe-fallback.exe");
    std::fs::write(&probe, "#!/bin/sh\necho \"PROBE_FOO=$FOO\"\n").unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&probe)
        .status()
        .unwrap();
    set_custom(&pool, &id, "FOO", "bar").await.unwrap();
    set_handle(&pool, &dir, &host, &id, false).await.unwrap();
    let launcher =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../launcher/tuxgt-launcher");
    let out = std::process::Command::new("sh")
        .arg(&launcher)
        .arg(&probe)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &dir)
        .env("TUXGT_DATA", &dir)
        .env("TUXGT_LAUNCHER_SO", &so)
        .env("STEAM_COMPAT_DATA_PATH", "/pfx/sekiro")
        .env("LD_PRELOAD", "/sentinel/keep.so")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("PROBE_FOO=bar"), "{stdout}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
