use super::*;
use crate::apply::ApplyCtx;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

#[test]
fn scans_sideload_and_gog_installed() {
    let root = std::env::temp_dir().join(format!("tuxgt-heroic-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sideload_apps")).unwrap();
    fs::create_dir_all(root.join("gog_store")).unwrap();
    let cover = root.join("cover.jpg");
    fs::write(&cover, b"x").unwrap();
    fs::write(
            root.join("sideload_apps/library.json"),
            format!(
                r#"{{"games":[{{"runner":"sideload","app_name":"abc","title":"Side","is_installed":true,"art_square":"{}"}}]}}"#,
                cover.display()
            ),
        )
        .unwrap();
    fs::write(
        root.join("gog_store/installed.json"),
        r#"{"installed":[{"appName":"123","title":"GOG Game","install_path":"/tmp"}]}"#,
    )
    .unwrap();

    let recs = scan_roots(&[root.clone()]);
    let ids: Vec<String> = recs.iter().map(|r| r.id.to_string()).collect();
    assert!(ids.contains(&"heroic:standalone:abc".into()), "{ids:?}");
    assert!(ids.contains(&"heroic:gog:123".into()), "{ids:?}");
    let side = recs.iter().find(|r| r.id.game == "abc").unwrap();
    assert_eq!(side.cover_path.as_deref(), Some(cover.as_path()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn keeps_http_art_for_lazy_fetch() {
    let url = "https://cdn2.steamgriddb.com/grid/a.png";
    assert_eq!(
        art_path(Some(&Value::String(url.into()))),
        Some(PathBuf::from(url))
    );
}

#[test]
fn scan_keeps_remote_cover_url() {
    let root = std::env::temp_dir().join(format!("tuxgt-heroic-art-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("store_cache")).unwrap();
    fs::write(
            root.join("store_cache/gog_library.json"),
            r#"{"games":[{"runner":"gog","app_name":"42","title":"Art Game","is_installed":true,"art_square":"https://cdn2.steamgriddb.com/grid/a.png"}]}"#,
        )
        .unwrap();
    let recs = scan_roots(&[root.clone()]);
    let g = recs.iter().find(|r| r.id.game == "42").unwrap();
    assert_eq!(
        g.cover_path.as_deref(),
        Some(std::path::Path::new(
            "https://cdn2.steamgriddb.com/grid/a.png"
        ))
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn sideload_file_defaults_runner_when_absent() {
    let root = std::env::temp_dir().join(format!("tuxgt-heroic-norunner-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sideload_apps")).unwrap();
    fs::write(
        root.join("sideload_apps/library.json"),
        r#"{"games":[{"app_name":"norun","title":"No Runner","is_installed":true}]}"#,
    )
    .unwrap();
    let recs = scan_roots(&[root.clone()]);
    assert!(recs
        .iter()
        .any(|r| r.id.to_string() == "heroic:standalone:norun"));
    let _ = fs::remove_dir_all(&root);
}

fn test_ctx() -> ApplyCtx {
    let dir = std::env::temp_dir().join(format!("tuxgt-heroic-apply-{}", std::process::id()));
    ApplyCtx {
        data_dir: dir.clone(),
        launcher: PathBuf::from("/tmp/tuxgt-launcher"),
        ini: dir.join("tuxgt-launcher.ini"),
        game_dir: dir.join("runtime"),
        depot: dir.join("stage"),
    }
}

#[test]
fn ensure_adds_wrapper_only() {
    let ctx = test_ctx();
    let mut doc: Value =
        serde_json::from_str(r#"{"abc": {"winePrefix": "/p", "launcherArgs": "--x"}}"#).unwrap();
    let snap = {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx)
    };
    assert_eq!(snap.get("wrapperOptions"), Some(&Value::Null));
    let t = heroic_target(&mut doc, "abc").unwrap();
    let wrappers = t.get("wrapperOptions").and_then(Value::as_array).unwrap();
    assert_eq!(wrappers.len(), 1);
    assert_eq!(
        wrappers[0].get("exe").and_then(Value::as_str),
        Some("/tmp/tuxgt-launcher")
    );
    assert!(t.get("enviromentOptions").is_none());
    assert_eq!(t.get("launcherArgs").and_then(Value::as_str), Some("--x"));
}

#[test]
fn ensure_is_idempotent() {
    let ctx = test_ctx();
    let mut doc: Value = serde_json::from_str(r#"{"wrapperOptions": []}"#).unwrap();
    {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx);
    }
    let snap = {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx)
    };
    let t = heroic_target(&mut doc, "abc").unwrap();
    assert_eq!(
        t.get("wrapperOptions")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        1
    );
    assert_ne!(snap.get("wrapperOptions"), Some(&Value::Null));
}

#[test]
fn unensure_restores_snapshot() {
    let ctx = test_ctx();
    let mut doc: Value = serde_json::from_str(
        r#"{"winePrefix": "/p", "enviromentOptions": [{"key": "FOO", "value": "1"}]}"#,
    )
    .unwrap();
    let snap = {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx)
    };
    {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_unensure(t, &snap);
    }
    let t = heroic_target(&mut doc, "abc").unwrap();
    assert!(t.get("wrapperOptions").is_none());
    assert_eq!(
        t.get("enviromentOptions")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn strip_managed_reconstructs_clean_previous() {
    let ctx = test_ctx();
    let mut doc: Value = serde_json::from_str(
        r#"{"winePrefix": "/p", "enviromentOptions": [{"key": "FOO", "value": "1"}]}"#,
    )
    .unwrap();
    // Clean previous: absent wrapper key, user env only.
    let snap = {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx)
    };
    assert_eq!(snap.get("wrapperOptions"), Some(&Value::Null));
    // Wrapped state (lost-record previous): strip must recover it.
    let wrapped = {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx)
    };
    let clean = strip_managed(&wrapped, &ctx);
    assert_eq!(clean, snap);
    // Restore from the stripped snapshot actually unwraps.
    {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_unensure(t, &clean);
    }
    let t = heroic_target(&mut doc, "abc").unwrap();
    assert!(t.get("wrapperOptions").is_none());
    assert_eq!(
        t.get("enviromentOptions")
            .and_then(Value::as_array)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn flat_shape_edits_top_level() {
    let ctx = test_ctx();
    let mut doc: Value = serde_json::from_str(r#"{"winePrefix": "/p"}"#).unwrap();
    {
        let t = heroic_target(&mut doc, "abc").unwrap();
        heroic_ensure(t, &ctx);
    }
    assert!(doc.get("abc").is_none());
    assert!(doc
        .get("wrapperOptions")
        .and_then(Value::as_array)
        .is_some());
}

#[test]
fn hidden_list_overlay_and_per_game_keys() {
    let root = std::env::temp_dir().join(format!("tuxgt-heroic-hidden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("store_cache")).unwrap();
    fs::create_dir_all(root.join("store")).unwrap();
    fs::create_dir_all(root.join("sideload_apps")).unwrap();
    fs::write(
            root.join("store/config.json"),
            r#"{"games":{"hidden":[{"appName":"goghidden","title":"H"},{"appName":"sidehidden","title":"S"}]}}"#,
        )
        .unwrap();
    fs::write(
            root.join("store_cache/gog_library.json"),
            r#"{"games":[
                {"runner":"gog","app_name":"goghidden","title":"Overlay Hidden","is_installed":true},
                {"runner":"gog","app_name":"gogvisible","title":"Visible","is_installed":true},
                {"runner":"gog","app_name":"gogflag","title":"Flag","is_installed":true,"isHidden":true},
                {"runner":"gog","app_name":"gogstr","title":"Str","is_installed":true,"is_hidden":"1"},
                {"runner":"gog","app_name":"gognum","title":"Num","is_installed":true,"hidden":1}
            ]}"#,
        )
        .unwrap();
    fs::write(
            root.join("sideload_apps/library.json"),
            r#"{"games":[
                {"runner":"sideload","app_name":"sidehidden","title":"Side Hidden","is_installed":true},
                {"runner":"sideload","app_name":"sidevisible","title":"Side Visible","is_installed":true}
            ]}"#,
        )
        .unwrap();
    let recs = scan_roots(&[root.clone()]);
    let hidden_of = |app: &str| recs.iter().find(|r| r.id.game == app).unwrap().hidden;
    assert!(
        hidden_of("goghidden"),
        "games.hidden[] overlays a store row"
    );
    assert!(!hidden_of("gogvisible"));
    assert!(hidden_of("gogflag"), "isHidden");
    assert!(hidden_of("gogstr"), "is_hidden string");
    assert!(hidden_of("gognum"), "hidden number");
    assert!(
        hidden_of("sidehidden"),
        "games.hidden[] overlays a sideload"
    );
    assert!(!hidden_of("sidevisible"));
    let _ = fs::remove_dir_all(&root);
}
