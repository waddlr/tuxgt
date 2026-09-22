use super::testing::*;
use super::*;
use crate::game::GameId;
use crate::testing::{seed_game, SeedGame};
use crate::{disable_knob, set_custom, set_knob, Error};
use sqlx::SqlitePool;
use std::fs;

#[tokio::test]
async fn empty_game_knob_writes_empty_assign() {
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(text.contains("MANGOHUD=\n"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn unset_knob_omitted() {
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    crate::unset_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(!text.contains("MANGOHUD="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn disabled_knob_omitted() {
    let (pool, dir, host, id) = setup().await;
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    disable_knob(&pool, &id, "mangohud").await.unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    let text = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(!text.contains("MANGOHUD="), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn override_exe_prefix_refreshes_session_and_correlator() {
    // Site 1 (doctor --set) + Site 2 (GUI Launch-tab editors): the wired
    // sequence is `set_override` then `sync_session` (which also rewrites
    // the correlator). Proves both files refresh after the write.
    use crate::set_override;
    let (pool, dir, host, id) = setup().await;
    // E94: a knob keeps the Hook arm legal for the `inject=1` assertion.
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    set_override(&pool, &id, "exe", Some("/games/sekiro/dlc.exe"))
        .await
        .unwrap();
    set_override(&pool, &id, "prefix", Some("/pfx/sekiro2"))
        .await
        .unwrap();
    sync_session(&pool, &dir, &host, &id).await.unwrap();
    let corr = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(corr.contains("[/pfx/sekiro2]"), "{corr}");
    assert!(
        corr.contains("/games/sekiro/dlc.exe=steam/814380"),
        "{corr}"
    );
    let conf = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(conf.contains("inject=1"), "{conf}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn platform_override_flips_session_proton_grant() {
    // Site 3 (redetect/scan path): detected/platform writes change the
    // session proton grant. Forcing `native` must drop it; clearing back
    // must restore it after `sync_session`.
    use crate::set_override;
    let (pool, dir, host, _) = setup().await;
    seed_game(
        &pool,
        SeedGame {
            id: "steam::r59opti",
            manager: "steam",
            game_id: "r59opti",
            name: Some("opti"),
            exe_path: Some("/games/opti/opti.exe"),
            prefix_path: Some("/pfx/opti"),
            detected_proton: Some("GE-Proton10-4"),
            ..Default::default()
        },
    )
    .await;
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
    set_override(&pool, "steam::r59opti", "platform", Some("native"))
        .await
        .unwrap();
    sync_session(&pool, &dir, &host, "steam::r59opti")
        .await
        .unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(!text.contains("PROTON_USE_OPTISCALER"), "{text}");
    set_override(&pool, "steam::r59opti", "platform", None)
        .await
        .unwrap();
    sync_session(&pool, &dir, &host, "steam::r59opti")
        .await
        .unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    assert!(text.contains("PROTON_USE_OPTISCALER=1"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn scan_batch_refreshes_handle_sessions_and_correlator() {
    // Site 3 (scan path): `sync_handle_sessions` refreshes every
    // handle-on game plus the correlator after store/detect writes.
    use crate::set_override;
    let (pool, dir, host, id) = setup().await;
    // E94: knobs keep both arms legal across `sync_handle_sessions`.
    set_knob(&pool, &id, "mangohud", "1").await.unwrap();
    set_knob(&pool, "heroic:gog:abc", "mangohud", "1")
        .await
        .unwrap();
    set_handle(&pool, &dir, &host, &id, true).await.unwrap();
    set_handle(&pool, &dir, &host, "heroic:gog:abc", true)
        .await
        .unwrap();
    set_override(&pool, &id, "exe", Some("/games/sekiro/scan.exe"))
        .await
        .unwrap();
    sync_handle_sessions(&pool, &dir, &host).await.unwrap();
    let corr = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(
        corr.contains("/games/sekiro/scan.exe=steam/814380"),
        "{corr}"
    );
    assert!(
        corr.contains("/games/flag/flag.exe=heroic_gog/abc"),
        "{corr}"
    );
    let conf = fs::read_to_string(steam_conf(&dir)).unwrap();
    assert!(conf.contains("inject=1"), "{conf}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
// R55 fixtures: extras, refuse-vs-net, canonical lookup parity.

#[tokio::test]
async fn canonical_key_folds_separators_case_and_trim() {
    // SLOP T03 §1.4 golden exe-key vectors: the shared contract for
    // `canonical_exe_key` (Rust), `_norm` (hook Python), and `norm_path`
    // (trampoline shell). Mirrored in `src/protonfixes-hook/test_norm.py`
    // and `src/launcher/test/test_norm_path.sh` — keep the three in sync
    // so drift fails loudly. Vectors stay ASCII: the shell fold is
    // ASCII-only (`tr A-Z a-z`). Leading slashes are preserved; only
    // trailing slashes trim.
    let vectors = [
        // Pre-existing cases (kept verbatim).
        ("C:\\Games\\SkyrimSE.EXE ", "c:/games/skyrimse.exe"),
        ("/PFX/Skyrim//", "/pfx/skyrim"),
        ("  /a / ", "/a"),
        ("", ""),
        // Backslash fold, lowercase, whitespace trim.
        ("a\\b\\c", "a/b/c"),
        ("ABC.EXE", "abc.exe"),
        ("\t /X/ ", "/x"),
        // Trailing slashes trim; leading slashes stay.
        ("foo///", "foo"),
        ("///a", "///a"),
        ("/", ""),
        ("   ", ""),
        // Combined: all folds at once.
        ("  C:\\GAMES\\Foo.EXE//  ", "c:/games/foo.exe"),
    ];
    for (input, want) in vectors {
        assert_eq!(canonical_exe_key(input), want, "input: {input:?}");
    }
}

async fn mo2_row(pool: &SqlitePool) {
    // Steam MO2 shape: the shortcut target (store exe) is the loader
    // while the detector resolves the inner game exe.
    seed_game(
        pool,
        SeedGame {
            id: "steam::mo2test",
            manager: "steam",
            game_id: "mo2test",
            name: Some("mo2game"),
            exe_path: Some("C:\\Games\\ModOrganizer.exe"),
            prefix_path: Some("/pfx/mo2"),
            detected_exe_path: Some("C:\\Games\\Stock Game\\SkyrimSE.exe"),
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn extra_exe_keys_correlate_mo2_shape() {
    // Effective exe is the inner game exe; the user adds the loader
    // side as an extra. Both keys must render under one section.
    let (pool, dir, _host, _) = setup().await;
    mo2_row(&pool).await;
    add_extra_exe(&pool, "steam::mo2test", "C:\\Games\\ModOrganizer.exe")
        .await
        .unwrap();
    rewrite_correlator(&pool, &dir).await.unwrap();
    let text = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(text.contains("[/pfx/mo2]"), "{text}");
    assert!(
        text.contains("c:/games/stock game/skyrimse.exe=steam/mo2test"),
        "{text}"
    );
    assert!(
        text.contains("c:/games/modorganizer.exe=steam/mo2test"),
        "{text}"
    );
    // Extra equal to the primary after normalization: skipped, and a
    // re-add of the stored extra is a no-op.
    add_extra_exe(&pool, "steam::mo2test", "c:/games/stock game/skyrimse.exe")
        .await
        .unwrap();
    add_extra_exe(&pool, "steam::mo2test", "C:\\Games\\ModOrganizer.exe")
        .await
        .unwrap();
    assert_eq!(
        list_extra_exes(&pool, "steam::mo2test").await.unwrap(),
        vec!["c:/games/modorganizer.exe".to_string()]
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn prefix_override_collision_refuses_naming_owner() {
    // B parks on A's prefix with a different exe (no collision), then
    // aims B's exe at A's exe: the post-write render would collide, so
    // the write is refused naming A and persists nothing.
    use crate::set_override;
    let (pool, dir, _host, _) = setup().await;
    set_override(&pool, "heroic:gog:abc", "prefix", Some("/pfx/sekiro"))
        .await
        .unwrap();
    let err = set_override(
        &pool,
        "heroic:gog:abc",
        "exe",
        Some("/games/sekiro/sekiro.exe"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Error::CorrelatorCollision(_)), "{err:?}");
    let msg = err.to_string();
    assert!(msg.contains("steam::814380"), "{msg}");
    assert!(msg.contains("/games/sekiro/sekiro.exe"), "{msg}");
    let raw: Option<(Option<String>,)> =
        sqlx::query_as("SELECT override_exe_path FROM games WHERE id = 'heroic:gog:abc'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(raw, Some((None,)), "refused write left state behind");
    rewrite_correlator(&pool, &dir).await.unwrap();
    let text = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(
        text.contains("/games/sekiro/sekiro.exe=steam/814380"),
        "{text}"
    );
    // B keeps its own non-colliding key; only the refused key is absent.
    assert!(
        text.contains("/games/flag/flag.exe=heroic_gog/abc"),
        "{text}"
    );
    assert!(
        !text.contains("/games/sekiro/sekiro.exe=heroic_gog/abc"),
        "{text}"
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn batch_collision_renders_sorted_first() {
    // Batch paths (scan/resync) bypass the refuse guard and keep
    // writing: the render net gives the contested key to the
    // sorted-first rel (`heroic_gog/abc` < `steam/814380`) and drops
    // the loser with a warn (never silent last-wins).
    let (pool, dir, _host, _) = setup().await;
    sqlx::query(
        "UPDATE games SET exe_path = '/games/sekiro/sekiro.exe', prefix_path = '/pfx/sekiro'
             WHERE id = 'heroic:gog:abc'",
    )
    .execute(&pool)
    .await
    .unwrap();
    rewrite_correlator(&pool, &dir).await.unwrap();
    let text = fs::read_to_string(correlator_path(&dir)).unwrap();
    assert!(
        text.contains("/games/sekiro/sekiro.exe=heroic_gog/abc"),
        "{text}"
    );
    assert!(!text.contains("steam/814380"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn hook_correlates_mixed_case_exe() {
    // Lookup parity, hook side: the real `tuxgt_apply.py` resolves a
    // mixed-case Proton-reported exe against lowercased ini keys, on
    // both the primary and the extra key.
    let (pool, dir, host, _) = setup().await;
    mo2_row(&pool).await;
    add_extra_exe(&pool, "steam::mo2test", "C:\\Games\\ModOrganizer.exe")
        .await
        .unwrap();
    set_handle(&pool, &dir, &host, "steam::mo2test", true)
        .await
        .unwrap();
    let hooks = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protonfixes-hook");
    let run = |exe: &str| {
        std::process::Command::new("python3")
            .arg("-c")
            .arg(
                "import os, sys; sys.path.insert(0, sys.argv[1]); \
                     from pathlib import Path; import tuxgt_apply; \
                     conf = tuxgt_apply.correlate(Path(os.environ['TUXGT_DATA'])); \
                     print(conf if conf else 'MISS')",
            )
            .arg(&hooks)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TUXGT_DATA", &dir)
            .env("STEAM_COMPAT_DATA_PATH", "/pfx/mo2")
            .env("EXE", exe)
            .output()
            .unwrap()
    };
    for exe in [
        "C:\\Games\\Stock Game\\SkyrimSE.exe",
        "c:\\games\\modorganizer.EXE",
    ] {
        let out = run(exe);
        assert!(out.status.success(), "{out:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert!(stdout.contains("tux-protonfixes.conf"), "{exe}: {stdout}");
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn trampoline_correlates_mixed_case_exe() {
    // Lookup parity, trampoline side: the real `src/launcher/tuxgt-launcher`
    // self-arms from a mixed-case argv exe against a lowercased key.
    use crate::set_override;
    let (pool, dir, host, id) = setup().await;
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    let so = dir.join("lib/libtuxgt-launcher.so");
    std::fs::write(&so, b"so").unwrap();
    let probe = dir.join("ProbeMixed.EXE");
    std::fs::write(&probe, "#!/bin/sh\necho \"PROBE_MIXED=$FOO\"\n").unwrap();
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
    assert!(stdout.contains("PROBE_MIXED=bar"), "{stdout}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn hook_falls_back_to_single_rel_prefix() {
    // Prefix fallback, hook side: an exe with no key of its own (the
    // SKSE middle link) resolves when its prefix section names one
    // rel; a contested prefix still misses.
    let (pool, dir, host, _) = setup().await;
    mo2_row(&pool).await;
    set_handle(&pool, &dir, &host, "steam::mo2test", true)
        .await
        .unwrap();
    let hooks = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protonfixes-hook");
    let run = |prefix: &str, exe: &str| {
        std::process::Command::new("python3")
            .arg("-c")
            .arg(
                "import os, sys; sys.path.insert(0, sys.argv[1]); \
                     from pathlib import Path; import tuxgt_apply; \
                     conf = tuxgt_apply.correlate(Path(os.environ['TUXGT_DATA'])); \
                     print(conf if conf else 'MISS')",
            )
            .arg(&hooks)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TUXGT_DATA", &dir)
            .env("STEAM_COMPAT_DATA_PATH", prefix)
            .env("EXE", exe)
            .output()
            .unwrap()
    };
    let out = run("/pfx/mo2", "C:\\Games\\skse64_loader.exe");
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("tux-protonfixes.conf"), "{stdout}");
    // Contest the prefix with a second rel: the same exe now misses.
    sqlx::query(
        "UPDATE games SET exe_path = '/games/tool/tool.exe', prefix_path = '/pfx/mo2'
             WHERE id = 'heroic:gog:abc'",
    )
    .execute(&pool)
    .await
    .unwrap();
    rewrite_correlator(&pool, &dir).await.unwrap();
    let out = run("/pfx/mo2", "C:\\Games\\skse64_loader.exe");
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("MISS"), "{stdout}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
