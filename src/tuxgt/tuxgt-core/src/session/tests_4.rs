//! Session file gets the proxy-slot override the hook and Apply export.
use super::testing::*;
use super::*;
use crate::download::PlannedFile;
use crate::game::GameId;
use crate::{set_handle, write_manifest, FileManifest, PlannedEnv};
use std::fs;

fn manifest(game: &str, dest: &str, enabled: bool) -> FileManifest {
    manifest_files(game, &[dest], "preload", enabled)
}

fn manifest_files(game: &str, dests: &[&str], adapter: &str, enabled: bool) -> FileManifest {
    FileManifest {
        game: game.into(),
        instance: "optiscaler".into(),
        mod_type: "optiscaler".into(),
        adapter: adapter.into(),
        enabled,
        load_order: 0,
        include: Box::default(),
        files: dests
            .iter()
            .map(|dest| PlannedFile {
                source: format!("cache/k/{dest}"),
                dest: (*dest).into(),
                sha256: "aa".into(),
                enabled: true,
                load: None,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        backups: Default::default(),
        generated_globs: Box::default(),
        harvested: Default::default(),
        provenance: crate::ModProvenance::default(),
        env: Box::default(),
    }
}

async fn session_text(dest: &str, enabled: bool) -> String {
    session_case(dest, enabled, None, None).await
}

async fn session_case(
    dest: &str,
    enabled: bool,
    env: Option<&str>,
    launch_options: Option<&str>,
) -> String {
    let (pool, dir, host, _) = setup().await;
    let id = "steam::slotenv";
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id,
            manager: "steam",
            game_id: "slotenv",
            name: Some("slot"),
            exe_path: Some("/games/slot/slot.exe"),
            prefix_path: Some("/pfx/slot"),
            env,
            launch_options,
            ..Default::default()
        },
    )
    .await;
    write_manifest(&dir, &manifest(id, dest, enabled)).unwrap();
    set_handle(&pool, &dir, &host, id, true).await.unwrap();
    let gid = GameId::parse(id).unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    let _ = tokio::fs::remove_dir_all(&dir).await;
    text
}

#[tokio::test]
async fn session_sets_proxy_slot_override() {
    let text = session_text("dxgi.dll", true).await;
    assert!(
        text.lines().any(|l| l == "WINEDLLOVERRIDES=\"dxgi=n,b\""),
        "{text}"
    );
}

#[tokio::test]
async fn session_skips_self_slot_and_disabled() {
    let self_slot = session_text("OptiScaler.dll", true).await;
    assert!(!self_slot.contains("WINEDLLOVERRIDES"), "{self_slot}");
    let off = session_text("dxgi.dll", false).await;
    assert!(!off.contains("WINEDLLOVERRIDES"), "{off}");
}

#[tokio::test]
async fn session_keeps_store_stem_and_companion() {
    let (pool, dir, host, _) = setup().await;
    let id = "steam::slotenv";
    crate::testing::seed_game(
        &pool,
        crate::testing::SeedGame {
            id,
            manager: "steam",
            game_id: "slotenv",
            name: Some("slot"),
            exe_path: Some("/games/slot/slot.exe"),
            prefix_path: Some("/pfx/slot"),
            env: Some(r#"{"WINEDLLOVERRIDES":"dxgi=b"}"#),
            ..Default::default()
        },
    )
    .await;
    let mut manifest = manifest_files(id, &["dxgi.dll", "nvngx.dll"], "install", true);
    manifest.env = vec![PlannedEnv {
        key: "WINEDLLOVERRIDES".into(),
        value: "d3dcompiler_47=n".into(),
        enabled: true,
    }]
    .into_boxed_slice();
    write_manifest(&dir, &manifest).unwrap();
    set_handle(&pool, &dir, &host, id, true).await.unwrap();
    let gid = GameId::parse(id).unwrap();
    let text = fs::read_to_string(protonfixes_conf(&dir, &gid)).unwrap();
    let line = text
        .lines()
        .find(|l| l.starts_with("WINEDLLOVERRIDES="))
        .unwrap_or("");
    let value = line
        .split_once('=')
        .map(|(_, v)| v.trim_matches('"'))
        .unwrap_or("");
    let stems: Vec<&str> = value.split(';').collect();
    assert!(stems.iter().any(|e| *e == "dxgi=b"), "{text}");
    assert!(stems.iter().any(|e| *e == "nvngx=n,b"), "{text}");
    assert!(stems.iter().any(|e| *e == "d3dcompiler_47=n"), "{text}");
    assert!(!stems.iter().any(|e| *e == "dxgi=n,b"), "{text}");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn trampoline_decodes_quoted_session_values() {
    let dir = std::env::temp_dir().join(format!("tuxgt-q-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("games/steam/1")).unwrap();
    fs::create_dir_all(dir.join("lib")).unwrap();
    let so = dir.join("lib/libtuxgt-launcher.so");
    fs::write(&so, b"so").unwrap();
    let pfx = dir.join("pfx");
    fs::create_dir_all(&pfx).unwrap();
    fs::write(
        dir.join("games/load-correlator.ini"),
        format!("[{}]\nprobe.exe=steam/1\n", pfx.display()),
    )
    .unwrap();
    let mut pairs = std::collections::BTreeMap::new();
    pairs.insert("inject".into(), "1".into());
    pairs.insert("WINEDLLOVERRIDES".into(), "dxgi=n,b;d3d12=n,b".into());
    pairs.insert("FOO".into(), "hello world".into());
    pairs.insert("NL".into(), "a\nb".into());
    pairs.insert("BAZ".into(), "a$b`c\"d\\e".into());
    pairs.insert("PLAIN".into(), "bar".into());
    pairs.insert("s".into(), "keep-s".into());
    pairs.insert("buf".into(), "keep-buf".into());
    pairs.insert("fc".into(), "keep-fc".into());
    let text = render_session(&pairs);
    assert!(
        text.contains("WINEDLLOVERRIDES=\"dxgi=n,b;d3d12=n,b\"\n"),
        "{text}"
    );
    assert!(text.contains("PLAIN=bar\n"), "{text}");
    assert!(text.contains("inject=1\n"), "{text}");
    let conf = dir.join("games/steam/1/tux-protonfixes.conf");
    fs::write(&conf, &text).unwrap();
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    let probe = dir.join("probe.exe");
    fs::write(
        &probe,
        "#!/bin/sh\n\
         printf '%s' \"$WINEDLLOVERRIDES\" > \"$OUT/wine\"\n\
         printf '%s' \"$FOO\" > \"$OUT/foo\"\n\
         printf '%s' \"$NL\" > \"$OUT/nl\"\n\
         printf '%s' \"$BAZ\" > \"$OUT/baz\"\n\
         printf '%s' \"$PLAIN\" > \"$OUT/plain\"\n\
         printf '%s' \"$s\" > \"$OUT/s\"\n\
         printf '%s' \"$buf\" > \"$OUT/buf\"\n\
         printf '%s' \"$fc\" > \"$OUT/fc\"\n\
         [ -z \"${inject:-}\" ] && printf ok > \"$OUT/inject\"\n",
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&probe)
        .status()
        .unwrap();
    let launcher =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../launcher/tuxgt-launcher");
    let run = |conf_body: &str| {
        fs::write(&conf, conf_body).unwrap();
        std::process::Command::new("sh")
            .arg(&launcher)
            .arg(&probe)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &dir)
            .env("TUXGT_DATA", &dir)
            .env("TUXGT_LAUNCHER_SO", &so)
            .env("STEAM_COMPAT_DATA_PATH", &pfx)
            .env("EXE", "probe.exe")
            .env("OUT", &out)
            .output()
            .unwrap()
    };
    let hit = run(&text);
    assert!(
        hit.status.success(),
        "{}",
        String::from_utf8_lossy(&hit.stderr)
    );
    assert_eq!(
        fs::read_to_string(out.join("wine")).unwrap(),
        "dxgi=n,b;d3d12=n,b"
    );
    assert_eq!(fs::read_to_string(out.join("foo")).unwrap(), "hello world");
    assert_eq!(fs::read_to_string(out.join("nl")).unwrap(), "a\nb");
    assert_eq!(fs::read_to_string(out.join("baz")).unwrap(), "a$b`c\"d\\e");
    assert_eq!(fs::read_to_string(out.join("plain")).unwrap(), "bar");
    assert_eq!(fs::read_to_string(out.join("s")).unwrap(), "keep-s");
    assert_eq!(fs::read_to_string(out.join("buf")).unwrap(), "keep-buf");
    assert_eq!(fs::read_to_string(out.join("fc")).unwrap(), "keep-fc");
    assert_eq!(fs::read_to_string(out.join("inject")).unwrap(), "ok");
    let legacy = run("WINEDLLOVERRIDES=d3dcompiler_47=n;dxgi=n,b\n");
    assert!(
        legacy.status.success(),
        "{}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    assert_eq!(
        fs::read_to_string(out.join("wine")).unwrap(),
        "d3dcompiler_47=n;dxgi=n,b"
    );
    let _ = fs::remove_dir_all(&dir);
}
