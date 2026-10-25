use std::fs;

use super::*;
use crate::apply::{write_record, ApplyFile, ApplyRecord};
use crate::launch::testing::{pool_dir, stub_paths, test_host};
use crate::testing::{seed_game, SeedGame};

pub(super) const VDF: &str = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"814380\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"%command% -windowed\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";

pub(super) const ENGINE: &str = r#"
[engine]
name = "unreal"

[[arg]]
id = "dx11"
template = "-dx11"
group = "api"
help = "DirectX 11."

[[arg]]
id = "dx12"
template = "-dx12"
group = "api"
help = "DirectX 12."

[[arg]]
id = "log"
template = "-log"
help = "Write a log."

[[arg]]
id = "resx"
kind = "freeform"
template = "-ResX={value}"
help = "Width."
"#;

pub(super) async fn steam_ready() -> (sqlx::SqlitePool, std::path::PathBuf, crate::PluginHost) {
    let (pool, dir) = pool_dir().await;
    let eng = dir.join("share/args/curated/engines");
    fs::create_dir_all(&eng).unwrap();
    fs::write(eng.join("unreal.toml"), ENGINE).unwrap();
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/tuxgt-launcher"), b"#!/bin/sh\nexec \"$@\"\n").unwrap();
    let id = "steam:steam:814380";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "steam",
            store: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let host = test_host(&dir);
    (pool, dir, host)
}

pub(super) fn plant(dir: &std::path::Path, previous: Option<&str>, body: &str) {
    let vdf = dir.join("localconfig.vdf");
    fs::write(&vdf, body).unwrap();
    write_record(
        dir,
        &ApplyRecord {
            game: "steam:steam:814380".into(),
            manager: "steam".into(),
            launcher: dir.join("bin/tuxgt-launcher").to_string_lossy().into(),
            applied_at: 1,
            files: vec![ApplyFile {
                path: vdf.to_string_lossy().into(),
                backup: None,
                previous: previous.map(|s| s.to_string()),
            }],
        },
    )
    .unwrap();
}

#[tokio::test]
async fn steam_save_keeps_previous_and_restore_is_byte_faithful() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let prev = "%command% -windowed";
    plant(&dir, Some(prev), VDF);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.dirty);
    assert_eq!(v.tail, "-log");
    let report = save_args(&pool, &dir, &host, id).await.unwrap();
    assert!(report.contains("saved"), "{report}");
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(text.contains("tuxgt-launcher"), "{text}");
    assert!(text.contains("%command%"), "{text}");
    assert!(text.contains("-windowed"), "{text}");
    assert!(text.contains("-log"), "{text}");
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous.as_deref(), Some(prev));

    set_arg(&pool, &dir, &host, id, "dx12", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    // log stays (it is not in the api group). The tail is rebuilt from
    // previous, so -log is not left in front of the new token.
    assert!(text.contains("%command% -windowed -dx12 -log"), "{text}");
    assert!(!text.contains("-dx11"), "{text}");
    assert!(!text.contains("-log -log"), "{text}");
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous.as_deref(), Some(prev));

    crate::apply::restore_launch(&dir, id).unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(text.contains(prev), "{text}");
    assert!(!text.contains("tuxgt-launcher"), "{text}");
    assert!(crate::game_args::store::published(&pool, id)
        .await
        .unwrap()
        .is_none());
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.rows.iter().any(|r| r.id == "dx12" && r.enabled));
    assert!(v.dirty);
}

#[tokio::test]
async fn steam_dedupes_user_token_and_skips_suppressed() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    plant(&dir, Some("PROTON_LOG=1 mangohud %command% -log"), VDF);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.tail.is_empty(), "{}", v.tail);
    assert!(!v.dirty);
    assert_eq!(
        save_args(&pool, &dir, &host, id).await.unwrap(),
        format!("{id} already saved")
    );

    plant(&dir, Some("%command%"), VDF);
    let py = dir.join("proton/protonfixes/gamefixes-steam/814380.py");
    fs::create_dir_all(py.parent().unwrap()).unwrap();
    fs::write(&py, "util.append_argument(\"-dx12\")\n").unwrap();
    let proton = dir.join("proton/proton");
    fs::write(&proton, b"#!/bin/sh\n").unwrap();
    sqlx::query("UPDATE games SET proton = ? WHERE id = ?")
        .bind(proton.to_str().unwrap())
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    crate::game_args::store::upsert_arg(&pool, id, "dx12", "", true)
        .await
        .unwrap();
    let v = view(&pool, &dir, id).await.unwrap();
    assert_eq!(
        v.rows.iter().find(|r| r.id == "dx12").unwrap().mark,
        ArgMark::Suppressed
    );
    assert!(!v.tokens.iter().any(|t| t == "-dx12"));
}

#[tokio::test]
async fn heroic_second_save_keeps_launcher_args_baseline() {
    let (pool, dir) = pool_dir().await;
    let eng = dir.join("share/args/curated/engines");
    fs::create_dir_all(&eng).unwrap();
    fs::write(eng.join("unreal.toml"), ENGINE).unwrap();
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/tuxgt-launcher"), b"#!/bin/sh\n").unwrap();
    let id = "heroic:gog:abc";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "heroic",
            store: "gog",
            game_id: "abc",
            name: Some("h"),
            ..Default::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let cfg = dir.join("abc.json");
    fs::write(
        &cfg,
        r#"{"abc":{"winePrefix":"/p","launcherArgs":"--user","wrapperOptions":[{"exe":"/wrap"}]}}"#,
    )
    .unwrap();
    let baseline = r#"{"wrapperOptions":[{"exe":"/wrap"}],"launcherArgs":"--user"}"#;
    write_record(
        &dir,
        &ApplyRecord {
            game: id.into(),
            manager: "heroic".into(),
            launcher: dir.join("bin/tuxgt-launcher").to_string_lossy().into(),
            applied_at: 1,
            files: vec![ApplyFile {
                path: cfg.to_string_lossy().into(),
                backup: None,
                previous: Some(baseline.into()),
            }],
        },
    )
    .unwrap();
    let host = test_host(&dir);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(doc["abc"]["launcherArgs"].as_str(), Some("--user -log"));
    assert_eq!(
        doc["abc"]["wrapperOptions"][0]["exe"].as_str(),
        Some("/wrap")
    );
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous.as_deref(), Some(baseline));
    let kept = rec.files[0].previous.clone();

    set_arg(&pool, &dir, &host, id, "dx11", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(
        doc["abc"]["launcherArgs"].as_str(),
        Some("--user -dx11 -log")
    );
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous, kept);
}

#[test]
fn heroic_first_publish_records_the_live_launcher_args() {
    let dir = std::env::temp_dir().join(format!("tuxgt-heroic-args-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).unwrap();
    let launcher = dir.join("bin/tuxgt-launcher");
    fs::write(&launcher, b"#!/bin/sh\n").unwrap();
    let cfg = dir.join("abc.json");
    fs::write(
        &cfg,
        r#"{"abc":{"winePrefix":"/p","launcherArgs":"--user","wrapperOptions":[{"exe":"/wrap"}]}}"#,
    )
    .unwrap();
    let id = "heroic:gog:abc";
    write_record(
        &dir,
        &ApplyRecord {
            game: id.into(),
            manager: "heroic".into(),
            launcher: launcher.to_string_lossy().into(),
            applied_at: 1,
            files: vec![ApplyFile {
                path: cfg.to_string_lossy().into(),
                backup: None,
                previous: Some(r#"{"wrapperOptions":[{"exe":"/wrap"}]}"#.into()),
            }],
        },
    )
    .unwrap();
    let ctx = crate::apply::ApplyCtx {
        data_dir: dir.clone(),
        launcher,
        ini: dir.join("ini"),
        game_dir: dir.join("runtime"),
        depot: dir.join("stage"),
    };
    crate::provider::heroic::publish_launcher_args(&ctx, id, "abc", "-log").unwrap();
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    let snap: serde_json::Value =
        serde_json::from_str(rec.files[0].previous.as_deref().unwrap()).unwrap();
    assert_eq!(snap["launcherArgs"].as_str(), Some("--user"));
    let frozen = rec.files[0].previous.clone();
    crate::provider::heroic::publish_launcher_args(&ctx, id, "abc", "-dx11").unwrap();
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous, frozen);
    let doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(doc["abc"]["launcherArgs"].as_str(), Some("--user -dx11"));
    assert_eq!(
        doc["abc"]["wrapperOptions"][0]["exe"].as_str(),
        Some("/wrap")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn manual_print_shows_the_flag_and_steam_spec_does_not() {
    let true_bin = std::path::PathBuf::from("/usr/bin/true");
    if !true_bin.is_file() {
        return;
    }
    let (pool, dir) = pool_dir().await;
    let eng = dir.join("share/args/curated/engines");
    fs::create_dir_all(&eng).unwrap();
    fs::write(eng.join("unreal.toml"), ENGINE).unwrap();
    let host = test_host(&dir);
    let mut paths = stub_paths(&dir);
    let id = "manual:standalone:cccccccc";
    seed_game(
        &pool,
        SeedGame {
            id,
            manager: "manual",
            store: "standalone",
            game_id: "cccccccc",
            name: Some("m"),
            exe_path: Some(true_bin.to_str().unwrap()),
            detected_platform: Some("native"),
            ..Default::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    set_arg(&pool, &dir, &host, id, "dx11", None).await.unwrap();
    let spec = crate::launch::build_launch_spec(&pool, &host, id, &paths, &dir)
        .await
        .unwrap();
    assert!(spec.display().contains("-dx11"), "{}", spec.display());

    let sid = "steam:steam:814380";
    seed_game(
        &pool,
        SeedGame {
            id: sid,
            manager: "steam",
            store: "steam",
            game_id: "814380",
            name: Some("s"),
            ..Default::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();
    plant(&dir, Some("%command%"), VDF);
    set_arg(&pool, &dir, &host, sid, "dx11", None)
        .await
        .unwrap();
    paths.steam = Some(dir.join("steam"));
    fs::write(paths.steam.as_ref().unwrap(), b"steam").unwrap();
    let spec = crate::launch::build_launch_spec(&pool, &host, sid, &paths, &dir)
        .await
        .unwrap();
    assert!(!spec.display().contains("-dx11"), "{}", spec.display());
    assert!(spec.display().contains("steam://rungameid/814380"));
}
