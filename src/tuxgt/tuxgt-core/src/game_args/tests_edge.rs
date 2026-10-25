use std::fs;

use super::store::{self, Snapshot};
use super::tests_save::{plant, steam_ready, ENGINE, VDF};
use super::*;
use crate::apply::{write_record, ApplyFile, ApplyRecord};
use crate::launch::testing::pool_dir;

#[tokio::test]
async fn empty_save_does_not_rearm_apply() {
    // The disarm/restore path is the subject here (the running-client
    // deferral has its own test below), so pin the owner stopped rather
    // than inherit the host's Steam.
    let _stopped = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, false);
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let prev = "%command% -windowed";
    plant(&dir, Some(prev), VDF);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    // Skip mutate_game so the toggle does not restore before Save.
    sqlx::query("UPDATE game_args SET enabled = 0 WHERE game_id = ? AND arg = 'log'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.dirty, "{v:?}");
    assert!(v.tail.is_empty(), "{}", v.tail);
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(!text.contains("tuxgt-launcher"), "{text}");
    assert!(text.contains(prev), "{text}");
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(!v.dirty);
    assert!(v.tail.is_empty());
}

#[tokio::test]
async fn empty_save_while_client_runs_errors_and_stays_dirty() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let prev = "%command% -windowed";
    plant(&dir, Some(prev), VDF);
    // The first Save is the legitimate stopped-client write.
    let stopped = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, false);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    drop(stopped);
    // Skip mutate_game so the toggle does not restore before Save.
    sqlx::query("UPDATE game_args SET enabled = 0 WHERE game_id = ? AND arg = 'log'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    // Auto-restore defers while the owner runs. The explicit Save must
    // error instead of publishing the empty tail over the live record
    // and marking the row clean.
    let before = fs::read(dir.join("localconfig.vdf")).unwrap();
    let running = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, true);
    let err = save_args(&pool, &dir, &host, id).await.unwrap_err();
    assert!(err.to_string().contains("Steam is running"), "{err}");
    drop(running);
    assert_eq!(fs::read(dir.join("localconfig.vdf")).unwrap(), before);
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(text.contains("tuxgt-launcher"), "{text}");
    assert!(crate::apply::read_record(&dir, id).unwrap().is_some());
    assert!(view(&pool, &dir, id).await.unwrap().dirty);
    assert_eq!(
        store::published(&pool, id).await.unwrap().unwrap().tail,
        "-log"
    );
    // Once the client stops the same Save restores and lands clean.
    let _stopped = crate::StoreClient::set_running_for_test(crate::StoreClient::Steam, false);
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(!text.contains("tuxgt-launcher"), "{text}");
    assert!(text.contains(prev), "{text}");
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    assert!(!view(&pool, &dir, id).await.unwrap().dirty);
}

#[tokio::test]
async fn heroic_empty_save_restores_the_baseline() {
    use crate::launch::testing::test_host;
    use crate::testing::{seed_game, SeedGame};

    let _stopped = crate::StoreClient::set_running_for_test(crate::StoreClient::Heroic, false);
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
    // Live state is the applied one: Apply appended our launcher entry
    // to the user's wrapperOptions (snapshotted in `previous` below).
    fs::write(
        &cfg,
        format!(
            r#"{{"abc":{{"winePrefix":"/p","launcherArgs":"--user","wrapperOptions":[{{"exe":"/wrap"}},{{"exe":"{}","args":""}}]}}}}"#,
            dir.join("bin/tuxgt-launcher").display()
        ),
    )
    .unwrap();
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
                previous: Some(
                    r#"{"wrapperOptions":[{"exe":"/wrap"}],"launcherArgs":"--user"}"#.into(),
                ),
            }],
        },
    )
    .unwrap();
    let host = test_host(&dir);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    // Skip mutate_game so the toggle does not restore before Save.
    sqlx::query("UPDATE game_args SET enabled = 0 WHERE game_id = ? AND arg = 'log'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let doc: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(doc["abc"]["launcherArgs"].as_str(), Some("--user"));
    let wrappers = doc["abc"]["wrapperOptions"].as_array().unwrap();
    assert_eq!(wrappers.len(), 1, "{doc}");
    assert_eq!(wrappers[0]["exe"].as_str(), Some("/wrap"));
    assert!(crate::apply::read_record(&dir, id).unwrap().is_none());
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(!v.dirty);
    assert!(v.tail.is_empty());
}

#[tokio::test]
async fn stripped_previous_keeps_user_tokens_after_command() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let frag = crate::apply::quote_fragment(&dir.join("bin/tuxgt-launcher"));
    assert!(
        !frag.chars().any(|c| c.is_whitespace() || c == '"'),
        "{frag}"
    );
    let live = format!("{frag} %command% -windowed -log");
    let body = VDF.replacen("%command% -windowed", &live, 1);
    plant(&dir, Some("-windowed -log"), &body);
    store::upsert_published(
        &pool,
        id,
        "-log",
        &Snapshot {
            args: Vec::new(),
            custom: String::new(),
        },
    )
    .await
    .unwrap();
    set_arg(&pool, &dir, &host, id, "dx12", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(text.contains("%command% -windowed -dx12"), "{text}");
    assert!(!text.contains("-log"), "{text}");
    assert!(!text.contains(&format!("-windowed {frag}")), "{text}");
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous.as_deref(), Some("-windowed -log"));

    set_arg(&pool, &dir, &host, id, "dx11", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    assert!(text.contains("%command% -windowed -dx11"), "{text}");
    assert!(!text.contains("-dx12"), "{text}");
    assert!(!text.contains(&format!("-windowed {frag}")), "{text}");
    let rec = crate::apply::read_record(&dir, id).unwrap().unwrap();
    assert_eq!(rec.files[0].previous.as_deref(), Some("-windowed -log"));
}

#[tokio::test]
async fn stripped_previous_keeps_quoted_user_tokens() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let frag = crate::apply::quote_fragment(&dir.join("bin/tuxgt-launcher"));
    let live = format!("{frag} %command% \\\"My Game\\\" -log");
    let body = VDF.replacen("%command% -windowed", &live, 1);
    plant(&dir, Some("\"My Game\" -log"), &body);
    store::upsert_published(
        &pool,
        id,
        "-log",
        &Snapshot {
            args: Vec::new(),
            custom: String::new(),
        },
    )
    .await
    .unwrap();
    set_arg(&pool, &dir, &host, id, "dx12", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    let opts = crate::provider::steam::steam_get_options(&text, "814380").unwrap();
    assert!(opts.contains("%command% \"My Game\" -dx12"), "{opts}");
    assert!(!opts.contains("-log"), "{opts}");
}

#[tokio::test]
async fn stripped_previous_strips_an_escaped_prior_tail() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let frag = crate::apply::quote_fragment(&dir.join("bin/tuxgt-launcher"));
    let prior = crate::apply::quote_token("say \"hi\"");
    let live = format!("{frag} %command% -windowed {prior}");
    let (body, _) = crate::provider::steam::steam_set_options(VDF, "814380", &live).unwrap();
    let previous = crate::provider::steam::strip_fragment(&live, &frag).unwrap();
    plant(&dir, Some(&previous), &body);
    store::upsert_published(
        &pool,
        id,
        &prior,
        &Snapshot {
            args: Vec::new(),
            custom: String::new(),
        },
    )
    .await
    .unwrap();
    set_arg(&pool, &dir, &host, id, "dx12", None).await.unwrap();
    save_args(&pool, &dir, &host, id).await.unwrap();
    let text = fs::read_to_string(dir.join("localconfig.vdf")).unwrap();
    let opts = crate::provider::steam::steam_get_options(&text, "814380").unwrap();
    assert!(opts.contains("%command% -windowed -dx12"), "{opts}");
    assert!(!opts.contains("say"), "{opts}");
    assert!(!opts.contains("hi"), "{opts}");
}

#[tokio::test]
async fn missing_apply_target_stays_dirty() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    let missing = dir.join("missing.vdf");
    write_record(
        &dir,
        &ApplyRecord {
            game: id.into(),
            manager: "steam".into(),
            launcher: dir.join("bin/tuxgt-launcher").to_string_lossy().into(),
            applied_at: 1,
            files: vec![ApplyFile {
                path: missing.to_string_lossy().into(),
                backup: None,
                previous: Some("%command% -windowed".into()),
            }],
        },
    )
    .unwrap();
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    let err = save_args(&pool, &dir, &host, id).await.unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.dirty);
    assert!(store::published(&pool, id).await.unwrap().is_none());
}

#[tokio::test]
async fn heroic_missing_target_errors() {
    let (_pool, dir) = pool_dir().await;
    let id = "heroic:gog:abc";
    let missing = dir.join("missing.json");
    write_record(
        &dir,
        &ApplyRecord {
            game: id.into(),
            manager: "heroic".into(),
            launcher: dir.join("bin/tuxgt-launcher").to_string_lossy().into(),
            applied_at: 1,
            files: vec![ApplyFile {
                path: missing.to_string_lossy().into(),
                backup: None,
                previous: Some(r#"{"launcherArgs":"--user"}"#.into()),
            }],
        },
    )
    .unwrap();
    let ctx = crate::apply::ApplyCtx {
        data_dir: dir.clone(),
        launcher: dir.join("bin/tuxgt-launcher"),
        ini: dir.join("ini"),
        game_dir: dir.join("runtime"),
        depot: dir.join("stage"),
    };
    let err = crate::provider::heroic::publish_launcher_args(&ctx, id, "abc", "-log").unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[tokio::test]
async fn blank_custom_is_rejected() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    plant(&dir, Some("%command%"), VDF);
    let err = set_custom(&pool, &dir, &host, id, "   ").await.unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
    assert!(store::custom_text(&pool, id).await.unwrap().is_empty());
}

#[tokio::test]
async fn suppressed_custom_does_not_fail_the_view() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    plant(&dir, Some("%command%"), VDF);
    let proton = dir.join("proton/proton");
    fs::create_dir_all(proton.parent().unwrap()).unwrap();
    fs::write(&proton, b"#!/bin/sh\n").unwrap();
    let py = proton
        .parent()
        .unwrap()
        .join("protonfixes/gamefixes-steam/814380.py");
    fs::create_dir_all(py.parent().unwrap()).unwrap();
    fs::write(&py, "util.append_argument(\"-log\")\n").unwrap();
    sqlx::query("UPDATE games SET proton = ? WHERE id = ?")
        .bind(proton.to_str().unwrap())
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    store::set_custom_text(&pool, id, "-log -extra")
        .await
        .unwrap();
    let v = view(&pool, &dir, id).await.unwrap();
    assert!(v.tokens.iter().any(|t| t == "-extra"), "{:?}", v.tokens);
    assert!(!v.tokens.iter().any(|t| t == "-log"), "{:?}", v.tokens);
    assert!(v.channel);
    let err = set_custom(&pool, &dir, &host, id, "-log")
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::ArgSuppressed(_)), "{err}");
}

#[tokio::test]
async fn arg_count_skips_an_unset_game() {
    let (pool, dir, host) = steam_ready().await;
    let id = "steam:steam:814380";
    plant(&dir, Some("%command%"), VDF);
    assert_eq!(arg_count(&pool, &dir, id).await.unwrap(), 0);
    set_arg(&pool, &dir, &host, id, "log", None).await.unwrap();
    assert_eq!(arg_count(&pool, &dir, id).await.unwrap(), 1);
}
