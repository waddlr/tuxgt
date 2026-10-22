use std::fs;
use std::path::Path;

use super::catalog::{matching_args, GameIdentity};
use super::schema::Trust;
use super::*;
use crate::launch::testing::{pool_dir, test_host};
use crate::testing::{seed_game, SeedGame};

const ENGINE: &str = r#"
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

fn game_toml(template: &str, help: &str) -> String {
    format!(
        r#"
[game]
steam_appid = "5"

[[arg]]
id = "dx11"
template = "{template}"
help = "{help}"
source_url = "https://example.invalid/dx"
"#
    )
}

#[test]
fn schema_rejects_forbidden_template_and_missing_source() {
    let bad = r#"
[game]
steam_appid = "1"
[[arg]]
id = "bad"
template = "-dx11;rm"
help = "no"
source_url = "https://example.invalid/a"
"#;
    let err = parse_catalog(bad, "/curated/games/a.toml", Trust::CuratedGame).unwrap_err();
    assert!(err.to_string().contains("a.toml"), "{err}");

    let bare = r#"
[game]
steam_appid = "1"
[[arg]]
id = "dx11"
template = "-dx11"
help = "Force."
"#;
    let err = parse_catalog(bare, "/x/curated/games/a.toml", Trust::CuratedGame).unwrap_err();
    assert!(err.to_string().contains("source_url"), "{err}");

    let grouped = r#"
[engine]
name = "unreal"
[[arg]]
id = "width"
kind = "freeform"
template = "-ResX={value}"
group = "size"
help = "Width."
"#;
    let err = parse_catalog(
        grouped,
        "/x/curated/engines/unreal.toml",
        Trust::CuratedEngine,
    )
    .unwrap_err();
    assert!(err.to_string().contains("group"), "{err}");
}

#[test]
fn same_rank_collides_and_per_game_overrides_engine() {
    let a = parse_catalog(
        &game_toml("-dx11", "Game."),
        "/x/curated/games/a.toml",
        Trust::CuratedGame,
    )
    .unwrap();
    let b = parse_catalog(
        &game_toml("-dx11", "Other."),
        "/x/curated/games/b.toml",
        Trust::CuratedGame,
    )
    .unwrap();
    assert!(check_same_rank(&[a.clone(), b]).is_err());

    let engine = parse_catalog(
        ENGINE,
        "/x/curated/engines/unreal.toml",
        Trust::CuratedEngine,
    )
    .unwrap();
    assert!(check_same_rank(&[a.clone(), engine.clone()]).is_ok());
    let id = GameIdentity {
        manager: "steam".into(),
        store: "steam".into(),
        game: "5".into(),
        steam_appid: Some("5".into()),
        exe_stem: None,
        engine: Some("unreal".into()),
    };
    let merged = matching_args(&[engine, a], &id);
    let dx = merged.iter().find(|r| r.stem == "dx11").unwrap();
    assert_eq!(dx.help, "Game.");
    assert!(merged.iter().any(|r| r.id == "log"));
}

#[test]
fn harvest_emits_literals_and_protondb_stops_without_a_field() {
    let dir = std::env::temp_dir().join(format!("tuxgt-harvest-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let fixes = dir.join("src/gamefixes-steam");
    fs::create_dir_all(&fixes).unwrap();
    fs::write(
        fixes.join("9.py"),
        "util.append_argument(\"-dx11\")\nutil.append_argument(f\"-no\")\n",
    )
    .unwrap();
    let root = dir.join("args");
    let n = harvest_protonfixes(&dir.join("src"), &root.join("protonfixes"), "abc").unwrap();
    assert_eq!(n, 1);
    let text = fs::read_to_string(root.join("protonfixes/games/9.toml")).unwrap();
    assert!(text.contains("protonfixes abc"));
    assert!(text.contains("appended_value = \"-dx11\""));
    assert!(!text.contains("-no"));
    check_tree(&root).unwrap();

    let dump = dir.join("dump.json");
    fs::write(&dump, r#"[{"notes":"try -dx11"}]"#).unwrap();
    assert!(!protondb_has_launch_field(&dump).unwrap());
    fs::write(&dump, r#"[{"arguments":"-dx11"}]"#).unwrap();
    assert!(protondb_has_launch_field(&dump).unwrap());
    let _ = fs::remove_dir_all(&dir);
}

fn write_engine(dir: &Path) {
    let p = dir.join("share/args/curated/engines");
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("unreal.toml"), ENGINE).unwrap();
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/tuxgt-launcher"), b"#!/bin/sh\n").unwrap();
}

async fn engine_game(
    manager: &str,
    store: &str,
    game: &str,
) -> (sqlx::SqlitePool, std::path::PathBuf, String) {
    let (pool, dir) = pool_dir().await;
    write_engine(&dir);
    let id = format!("{manager}:{store}:{game}");
    seed_game(
        &pool,
        SeedGame {
            id: &id,
            manager,
            store,
            game_id: game,
            name: Some("t"),
            ..Default::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    (pool, dir, id)
}

#[tokio::test]
async fn group_set_clears_siblings_and_custom_rejects_command() {
    let (pool, dir, id) = engine_game("manual", "standalone", "aaaaaaaa").await;
    let host = test_host(&dir);
    set_arg(&pool, &dir, &host, &id, "dx11", None)
        .await
        .unwrap();
    set_arg(&pool, &dir, &host, &id, "dx12", None)
        .await
        .unwrap();
    let v = view(&pool, &dir, &id).await.unwrap();
    let dx11 = v.rows.iter().find(|r| r.id == "dx11").unwrap();
    let dx12 = v.rows.iter().find(|r| r.id == "dx12").unwrap();
    assert!(!dx11.enabled);
    assert!(dx12.enabled);
    assert!(v.tokens.iter().any(|t| t == "-dx12"));
    assert!(!v.tokens.iter().any(|t| t == "-dx11"));
    assert!(v.manual && !v.dirty);

    let err = set_custom(&pool, &dir, &host, &id, "%command% -foo")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("%command%"), "{err}");
    let err = save_args(&pool, &dir, &host, &id).await.unwrap_err();
    assert!(matches!(err, crate::Error::ArgsManualSave));
}

#[tokio::test]
async fn kept_user_and_suppression_and_round_trip() {
    let (pool, dir, id) = engine_game("manual", "standalone", "bbbbbbbb").await;
    let host = test_host(&dir);
    sqlx::query("UPDATE games SET launch_options = ? WHERE id = ?")
        .bind("%command% -log -ResX=1280")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    set_arg(&pool, &dir, &host, &id, "log", None).await.unwrap();
    set_arg(&pool, &dir, &host, &id, "resx", Some("1920"))
        .await
        .unwrap();
    let v = view(&pool, &dir, &id).await.unwrap();
    let log = v.rows.iter().find(|r| r.id == "log").unwrap();
    let res = v.rows.iter().find(|r| r.id == "resx").unwrap();
    assert_eq!(log.mark, ArgMark::None);
    assert_eq!(res.mark, ArgMark::KeptUser);
    assert!(v.tokens.is_empty());

    let proton = dir.join("proton/proton");
    fs::create_dir_all(proton.parent().unwrap()).unwrap();
    fs::write(&proton, b"#!/bin/sh\n").unwrap();
    let py = proton
        .parent()
        .unwrap()
        .join("protonfixes/gamefixes-steam/814380.py");
    fs::create_dir_all(py.parent().unwrap()).unwrap();
    fs::write(&py, "").unwrap();
    let shipped = dir.join("share/args/protonfixes/games/814380.toml");
    fs::create_dir_all(shipped.parent().unwrap()).unwrap();
    fs::write(
        &shipped,
        "[[fix]]\nstem = \"log\"\nappended_value = \"-log\"\nfix_file = \"x.py\"\n",
    )
    .unwrap();
    let (pool, dir, id) = {
        let (pool, dir) = (pool, dir);
        let id = "steam:steam:814380".to_string();
        seed_game(
            &pool,
            SeedGame {
                id: &id,
                manager: "steam",
                store: "steam",
                game_id: "814380",
                name: Some("s"),
                proton: Some(proton.to_str().unwrap()),
                ..Default::default()
            },
        )
        .await;
        sqlx::query("UPDATE games SET detected_engine = 'unreal' WHERE id = ?")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        (pool, dir, id)
    };
    let v = view(&pool, &dir, &id).await.unwrap();
    assert!(v.rows.iter().all(|r| r.mark != ArgMark::Suppressed));
    fs::write(&py, "util.append_argument(\"-log\")\n").unwrap();
    let err = set_arg(&pool, &dir, &host, &id, "log", None)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::Error::ArgSuppressed(_)), "{err}");
    crate::game_args::store::upsert_arg(&pool, &id, "log", "", true)
        .await
        .unwrap();
    let v = view(&pool, &dir, &id).await.unwrap();
    let log = v.rows.iter().find(|r| r.id == "log").unwrap();
    assert_eq!(log.mark, ArgMark::Suppressed);
    assert!(!v.tokens.iter().any(|t| t == "-log"));
    assert!(!v.channel);

    let snap = crate::game_args::store::Snapshot {
        args: crate::game_args::store::stored_args(&pool, &id)
            .await
            .unwrap(),
        custom: String::new(),
    };
    crate::game_args::store::upsert_published(&pool, &id, "", &snap)
        .await
        .unwrap();
    set_arg(&pool, &dir, &host, &id, "dx12", None)
        .await
        .unwrap();
    discard(&pool, &dir, &host, &id).await.unwrap();
    let v = view(&pool, &dir, &id).await.unwrap();
    assert!(v.rows.iter().any(|r| r.id == "log" && r.enabled));
    crate::game_args::store::delete_published(&pool, &id)
        .await
        .unwrap();
    assert!(crate::game_args::store::published(&pool, &id)
        .await
        .unwrap()
        .is_none());
}
